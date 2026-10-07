//! An image a prompt carries: what `/image`, `/paste` and Ctrl-V attach.
//!
//! Upstream's attachment (`vercel-labs/fx@c1db9191
//! src/core/images/image_attachments.zig`) is three facts and one rule, and this
//! module keeps all four:
//!
//! - **What it is** comes from its first bytes, never from its name
//!   ([`MediaType::sniff`]; `detectMediaTypeFromBytes`,
//!   `image_data.zig:136-153`): a `.png` that is text is refused, and a
//!   screenshot saved without an extension is accepted.
//! - **How big it may be** is decided before a byte of it is read
//!   ([`MAX_IMAGE_BYTES`]; `readImageHeaderBytes`,
//!   `image_attachments.zig:1825-1834`), so a file the size of a disk is
//!   refused by its `stat`.
//! - **Which file** a typed path names is upstream's literal rule
//!   ([`resolve`]; `normalizePathInput`, `:1846-1866`, and
//!   `resolve_workspace_or_external_literal_path`,
//!   `src/core/workspace/pathing.zig:228-385`): relative to the workspace, `~`
//!   for the home directory, absolute as written -- and a path that is spelled
//!   relative to the workspace may not leave it through a symlink.
//! - **The bytes sent are the bytes attached.** The file is read once, at the
//!   moment it is attached, and its SHA-256 recorded with it
//!   (`captureImageAttachmentForAdmission`): replacing the file afterwards
//!   changes nothing the session sends, and a resumed session checks the copy
//!   it saved against that digest before sending it again.
//!
//! Nothing here knows about a composer, a wire or a log. The TUI attaches,
//! [`crate::gateway::protocol`] and [`crate::llmux::protocol`] encode, and
//! [`crate::session`] keeps the copy a resume sends.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// The most bytes one attached image may hold (`max_image_bytes`,
/// `image_attachments.zig:14`). llmux takes the same ceiling for a decoded
/// image (`2lab-ai/llmux@4b3d4a4 src/provider/responses_request.rs:44`).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// The topic every attachment notice is filed under
/// (`image_commands.zig:42`, `:56`).
pub const TOPIC: &str = "images";

/// `/image` with nothing after it (`image_commands.zig:24-27`): an error with
/// no topic.
pub const USAGE: &str = "usage: /image <path>";

/// What a path that names nothing is told (`image_commands.zig:37`).
pub const NOT_FOUND: &str = "image file not found";

/// What a file whose first bytes are no image this build reads is told
/// (`image_commands.zig:36`).
pub const UNSUPPORTED: &str = "unsupported image type";

/// What a file over [`MAX_IMAGE_BYTES`] is told (`image_too_large_notice`,
/// `image_attachments.zig:16`).
pub const TOO_LARGE: &str = "image exceeds the 20 MiB limit";

/// What a composer at its byte budget answers an attachment with: neutral,
/// because nothing failed -- there is no room (`writeInputFullNotice`,
/// `image_commands.zig:13-19`).
pub const INPUT_FULL: &str = "input is full; image not attached";

/// `/images` with nothing pending (`image_commands.zig:68`).
pub const NONE_PENDING: &str = "no pending images";

/// `/images clear` (`image_commands.zig:63`).
pub const CLEARED: &str = "cleared pending images";

/// `/paste` and Ctrl-V when the clipboard holds no image
/// (`image_commands.zig:90`): neutral.
pub const NO_CLIPBOARD_IMAGE: &str = "no image found on clipboard";

/// What a turn carrying an image is refused with when the catalog cannot say
/// whether the model reads images -- it does not list the model, or it could
/// not be read (`model_image_capability_unavailable_notice`,
/// `image_attachments.zig:18`, mapped at `app_process_runtime.zig:118`).
pub const CAPABILITY_UNKNOWN: &str = "Unable to verify image support for this model, so the \
     image was not sent. Try again later, choose another model, or remove the image.";

/// What a turn carrying an image is refused with when the catalog lists the
/// model without `vision` and `file-input`.
///
/// Upstream's words for the same refusal when its second route -- a forced
/// `vision` tool call answered by another model -- is not available
/// (`error.SubscriptionNativeImageUnavailable`,
/// `src/core/agent/runtime/orchestrator.zig:7152-7155`, formatted by
/// `formatErrorBody`, `app_process_runtime.zig:114-126`). xfx does not have that
/// route (`.prd/tui-phase4/ssot.md` §Tensions), so this is what such a model is
/// always told here.
pub const NOT_NATIVE: &str = "request failed: SubscriptionNativeImageUnavailable";

/// One of the four image formats a provider is sent
/// (`image_data.zig:118-123`, `:136-153`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl MediaType {
    /// The MIME type, as every wire spells it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    /// The format a label names, or `None` for any other -- what a saved
    /// session's record is read back through.
    pub fn parse(label: &str) -> Option<Self> {
        [Self::Png, Self::Jpeg, Self::Gif, Self::Webp]
            .into_iter()
            .find(|media| media.label() == label)
    }

    /// The format `bytes` begin with, or `None` (`detectFormat`,
    /// `image_data.zig:145-153`).
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }
}

/// One attached image: the number its badge says, where it came from, and the
/// bytes it will be sent as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The number in its badge and in the prompt's `[Image #N]`. Never reused
    /// within a process (`allocateImageId`, `image_attachments.zig:1366-1370`).
    pub id: u32,
    /// The file it was read from, resolved -- or, for a clipboard image, the
    /// name its saved copy has in the session ([`Self::snapshot_name`]), which
    /// is what upstream names one by (`image_commands.zig:144-149`).
    pub path: PathBuf,
    pub media_type: MediaType,
    /// What was read when it was attached, and nothing read since.
    ///
    /// `Arc` because the same bytes are in the composer's badge, in the turn
    /// that sends them, and in the history every later request re-sends.
    pub bytes: Arc<[u8]>,
    /// The SHA-256 of [`Self::bytes`], lowercase hex.
    pub sha256: String,
}

impl Image {
    pub fn new(id: u32, path: PathBuf, media_type: MediaType, bytes: Arc<[u8]>) -> Self {
        let sha256 = digest(&bytes);
        Self {
            id,
            path,
            media_type,
            bytes,
            sha256,
        }
    }

    /// The last component of [`Self::path`]: what a notice and `/images` call
    /// it (`std.fs.path.basename`, `image_commands.zig:54`, `:76`).
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// The file a session keeps its copy in, inside its `images/` directory:
    /// upstream's `image-<id>-<first 16 hex of the digest>.bin`
    /// (`session_store.zig:102-118`).
    pub fn snapshot_name(&self) -> String {
        snapshot_name(self.id, &self.sha256)
    }
}

/// See [`Image::snapshot_name`].
pub fn snapshot_name(id: u32, sha256: &str) -> String {
    format!("image-{id}-{}.bin", &sha256[..sha256.len().min(16)])
}

/// What the prompt says where an image stands: `[Image #N]`
/// (`writeImagePlaceholder`, `image_attachments.zig:1376-1386`). The model is
/// sent this text with the image after it.
pub fn token(id: u32) -> String {
    format!("[Image #{id}]")
}

/// What the composer shows for it: `[Image N]` (`writeImageBadge`,
/// `image_attachments.zig:1336-1340`).
pub fn badge(id: u32) -> String {
    format!("[Image {id}]")
}

/// The SHA-256 of `bytes`, lowercase hex.
pub fn digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Why an image could not be attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    NotFound,
    Unsupported,
    TooLarge,
    /// Anything else, by upstream's name for it -- `InvalidPath`,
    /// `PathOutsideWorkspace`, `NotRegularFile`, `AccessDenied` -- which is
    /// what its notice prints (`failed to attach image: {s}`,
    /// `image_commands.zig:39`).
    Other(&'static str),
}

impl AttachError {
    /// Upstream's error name for it, which `/paste` prints where `/image`
    /// prints a sentence (`failed to paste clipboard image: {s}`,
    /// `image_commands.zig:92`).
    pub fn name(&self) -> &'static str {
        match self {
            Self::NotFound => "FileNotFound",
            Self::Unsupported => "UnsupportedImageType",
            Self::TooLarge => "ImageTooLarge",
            Self::Other(name) => name,
        }
    }

    fn from_io(err: &io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            kind => Self::Other(io_name(kind)),
        }
    }
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str(NOT_FOUND),
            Self::Unsupported => f.write_str(UNSUPPORTED),
            Self::TooLarge => f.write_str(TOO_LARGE),
            Self::Other(name) => write!(f, "failed to attach image: {name}"),
        }
    }
}

/// Upstream's name for an I/O failure, as a notice prints it: the Zig error a
/// file operation fails with there, for the kinds a path can produce here.
pub fn io_name(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        io::ErrorKind::NotADirectory => "NotDir",
        io::ErrorKind::IsADirectory => "IsDir",
        io::ErrorKind::InvalidFilename => "NameTooLong",
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => "InvalidPath",
        _ => "Unexpected",
    }
}

/// A path as the user typed it, made into the path it means
/// (`normalizePathInput`, `image_attachments.zig:1846-1866`): trimmed, one
/// balanced pair of outer quotes taken off, and every backslash escape
/// resolved to the character it escapes -- the spelling a shell's drag and
/// drop produces for `My\ Screenshot.png`.
pub fn normalize(input: &str) -> String {
    let trimmed = input.trim_matches([' ', '\t', '\r', '\n']);
    let unquoted = strip_quotes(trimmed);
    let mut out = String::with_capacity(unquoted.len());
    let mut chars = unquoted.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
                continue;
            }
        }
        out.push(c);
    }
    // Upstream trims a second time at the old resolution boundary
    // (`loadUserImageAttachment`, `:203-206`): `"  shot.png"` is `shot.png`.
    out.trim_matches([' ', '\t', '\r', '\n']).to_string()
}

fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// The file `path` names (already [`normalize`]d), resolved the way upstream
/// resolves a literal attachment path (`resolve_literal_path`,
/// `pathing.zig:308-385`):
///
/// - absolute: itself;
/// - `~` or `~/…`: under the home directory (`HomeNotSet` without one), and
///   `~name` is refused as `InvalidPath`;
/// - anything else: under the workspace.
///
/// Then the real path. A path **spelled** inside the workspace must still be
/// inside it once its symlinks are followed (`PathOutsideWorkspace`); one the
/// user spelled outside it -- absolute, `~`, or `..` past the root -- is their
/// stated intent and is read where it is.
pub fn resolve(workspace: &Path, home: Option<&Path>, path: &str) -> Result<PathBuf, AttachError> {
    if path.is_empty() {
        return Err(AttachError::Other("InvalidPath"));
    }
    let (spelled, external) = if Path::new(path).is_absolute() {
        (lexical(Path::new(path)), true)
    } else if path == "~" || path.starts_with("~/") {
        let home = home.ok_or(AttachError::Other("HomeNotSet"))?;
        if !home.is_absolute() {
            return Err(AttachError::Other("InvalidPath"));
        }
        (lexical(&home.join(path[1..].trim_start_matches('/'))), true)
    } else if path.starts_with('~') {
        return Err(AttachError::Other("InvalidPath"));
    } else {
        let joined = lexical(&workspace.join(path));
        let inside = joined.starts_with(workspace);
        (joined, !inside)
    };
    let real = spelled
        .canonicalize()
        .map_err(|err| AttachError::from_io(&err))?;
    if !external {
        // The workspace as the filesystem names it, so a root reached through
        // a symlink of its own (`/tmp` on macOS) still contains its files.
        let root = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        if !real.starts_with(&root) {
            return Err(AttachError::Other("PathOutsideWorkspace"));
        }
    }
    Ok(real)
}

/// `path` with `.` and `..` taken out by spelling alone (`std.fs.path.resolve`),
/// before anything is asked of the filesystem.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Reads the image at `path` -- a resolved, real path -- whole, or says why
/// it cannot be attached.
///
/// The size comes from `stat` and is judged before anything is read, so a file
/// over [`MAX_IMAGE_BYTES`] is refused without being opened for its bytes; the
/// read itself is bounded as well, so a file that grew between the two is
/// refused rather than read past the ceiling.
pub fn read(path: &Path) -> Result<(MediaType, Vec<u8>), AttachError> {
    let mut file = File::open(path).map_err(|err| AttachError::from_io(&err))?;
    let metadata = file.metadata().map_err(|err| AttachError::from_io(&err))?;
    if !metadata.is_file() {
        return Err(AttachError::Other("NotRegularFile"));
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(AttachError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    (&mut file)
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| AttachError::from_io(&err))?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(AttachError::TooLarge);
    }
    let media_type = MediaType::sniff(&bytes).ok_or(AttachError::Unsupported)?;
    Ok((media_type, bytes))
}

/// `/image <path>`'s whole question: the path typed, as the file it names and
/// the bytes in it.
pub fn load(
    workspace: &Path,
    home: Option<&Path>,
    input: &str,
) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
    let path = resolve(workspace, home, &normalize(input))?;
    let (media_type, bytes) = read(&path)?;
    Ok((path, media_type, bytes))
}

/// `bytes` as standard base64 with padding (RFC 4648 §4) -- what both wires
/// carry an image as.
///
/// Written here rather than taken from a crate: it is eighteen lines, the one
/// encoding this product needs, and the test below holds it to the exact
/// bytes upstream sent for the same file.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
pub(crate) mod fixture {
    /// The 4x4 red PNG upstream's capture attached (`shot.png`, 73 bytes):
    /// the bytes of the oracle the base64 test reads against.
    pub(crate) const SHOT_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x02, 0x00, 0x00, 0x00, 0x26,
        0x93, 0x09, 0x29, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x47, 0x0c, 0xc4, 0x71, 0x00, 0xae, 0x93, 0x0f, 0xf1, 0xd0, 0x5f, 0x23,
        0x9e, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// What upstream sent for it: `data` of the first `file` part in
    /// `cap/fx-image-40x120-bodies/001-completion.json`.
    pub(crate) const SHOT_PNG_BASE64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR4nGP4z8AARwzEcQCukw/x0F8jngAAAABJRU5ErkJggg==";
}

#[cfg(test)]
mod tests {
    use super::fixture::{SHOT_PNG, SHOT_PNG_BASE64};
    use super::*;

    #[test]
    fn base64_is_the_bytes_upstream_sent_for_the_same_file() {
        assert_eq!(base64(SHOT_PNG), SHOT_PNG_BASE64);
    }

    #[test]
    fn base64_pads_every_remainder_as_rfc_4648_does() {
        // RFC 4648 §10's vectors.
        for (input, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), encoded, "{input:?}");
        }
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn the_type_comes_from_the_first_bytes_and_never_from_the_name() {
        assert_eq!(MediaType::sniff(SHOT_PNG), Some(MediaType::Png));
        assert_eq!(
            MediaType::sniff(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(MediaType::Jpeg)
        );
        assert_eq!(MediaType::sniff(b"GIF87a...."), Some(MediaType::Gif));
        assert_eq!(MediaType::sniff(b"GIF89a...."), Some(MediaType::Gif));
        assert_eq!(
            MediaType::sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(MediaType::Webp)
        );
        // Short of a whole signature, or another format's.
        assert_eq!(MediaType::sniff(b"\x89PNG\r\n\x1a"), None);
        assert_eq!(MediaType::sniff(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(MediaType::sniff(b"BM......"), None);
        assert_eq!(MediaType::sniff(b""), None);
        for media in [
            MediaType::Png,
            MediaType::Jpeg,
            MediaType::Gif,
            MediaType::Webp,
        ] {
            assert_eq!(MediaType::parse(media.label()), Some(media));
        }
        assert_eq!(MediaType::parse("image/bmp"), None);
    }

    #[test]
    fn a_typed_path_loses_its_quotes_and_escapes_as_upstream_reads_it() {
        assert_eq!(normalize("  shot.png \t"), "shot.png");
        assert_eq!(normalize("\"my shot.png\""), "my shot.png");
        assert_eq!(normalize("'my shot.png'"), "my shot.png");
        assert_eq!(normalize("my\\ shot.png"), "my shot.png");
        // Unbalanced quotes are part of the name.
        assert_eq!(normalize("\"shot.png"), "\"shot.png");
        // A trailing lone backslash is kept as itself.
        assert_eq!(normalize("shot\\"), "shot\\");
        assert_eq!(normalize("\"  shot.png  \""), "shot.png");
    }

    #[test]
    fn the_badge_and_the_token_are_two_spellings_of_one_number() {
        assert_eq!(badge(3), "[Image 3]");
        assert_eq!(token(3), "[Image #3]");
        let image = Image::new(
            12,
            PathBuf::from("/w/shot.png"),
            MediaType::Png,
            Arc::from(SHOT_PNG),
        );
        assert_eq!(image.name(), "shot.png");
        assert_eq!(image.sha256.len(), 64);
        assert_eq!(
            image.snapshot_name(),
            format!("image-12-{}.bin", &image.sha256[..16])
        );
    }

    /// A workspace with `shot.png`, `notes.txt`, a directory, and a home
    /// beside it holding `home.png`.
    struct Tree {
        _root: tempfile::TempDir,
        workspace: PathBuf,
        home: PathBuf,
        outside: PathBuf,
    }

    fn tree() -> Tree {
        let root = tempfile::tempdir().expect("a root");
        let base = root.path().canonicalize().expect("canonical");
        let workspace = base.join("work");
        let home = base.join("home");
        let outside = base.join("outside");
        for dir in [&workspace, &home, &outside] {
            std::fs::create_dir_all(dir).expect("a directory");
        }
        std::fs::write(workspace.join("shot.png"), SHOT_PNG).expect("shot");
        std::fs::write(workspace.join("notes.txt"), "not an image").expect("notes");
        std::fs::create_dir(workspace.join("sub")).expect("sub");
        std::fs::write(workspace.join("sub").join("deep.png"), SHOT_PNG).expect("deep");
        std::fs::write(home.join("home.png"), SHOT_PNG).expect("home");
        std::fs::write(outside.join("secret.png"), SHOT_PNG).expect("outside");
        Tree {
            _root: root,
            workspace,
            home,
            outside,
        }
    }

    #[test]
    fn a_relative_path_is_read_from_the_workspace() {
        let tree = tree();
        let (path, media, bytes) =
            load(&tree.workspace, Some(&tree.home), "shot.png").expect("attached");
        assert_eq!(path, tree.workspace.join("shot.png"));
        assert_eq!(media, MediaType::Png);
        assert_eq!(bytes, SHOT_PNG);
        let (path, ..) =
            load(&tree.workspace, Some(&tree.home), "./sub/../sub/deep.png").expect("attached");
        assert_eq!(path, tree.workspace.join("sub/deep.png"));
    }

    #[test]
    fn a_tilde_is_the_home_directory_and_a_tilde_name_is_refused() {
        let tree = tree();
        let (path, ..) = load(&tree.workspace, Some(&tree.home), "~/home.png").expect("attached");
        assert_eq!(path, tree.home.join("home.png"));
        assert_eq!(
            load(&tree.workspace, Some(&tree.home), "~other/home.png"),
            Err(AttachError::Other("InvalidPath"))
        );
        assert_eq!(
            load(&tree.workspace, None, "~/home.png"),
            Err(AttachError::Other("HomeNotSet"))
        );
    }

    #[test]
    fn an_absolute_path_or_one_spelled_past_the_root_is_read_where_it_is() {
        let tree = tree();
        let absolute = tree.outside.join("secret.png");
        let (path, ..) = load(
            &tree.workspace,
            Some(&tree.home),
            absolute.to_str().expect("utf-8"),
        )
        .expect("attached");
        assert_eq!(path, absolute);
        let (path, ..) =
            load(&tree.workspace, Some(&tree.home), "../outside/secret.png").expect("attached");
        assert_eq!(path, absolute);
    }

    #[test]
    fn a_path_spelled_inside_the_workspace_may_not_leave_it_through_a_symlink() {
        // The security rule upstream's resolver keeps (`ensurePathInsideWorkspace`,
        // `pathing.zig:327`, `:1082-1085`): `link.png` is spelled as a file of the
        // workspace, so what it reads must be one.
        let tree = tree();
        std::os::unix::fs::symlink(
            tree.outside.join("secret.png"),
            tree.workspace.join("link.png"),
        )
        .expect("a symlink");
        assert_eq!(
            load(&tree.workspace, Some(&tree.home), "link.png"),
            Err(AttachError::Other("PathOutsideWorkspace"))
        );
        assert_eq!(
            AttachError::Other("PathOutsideWorkspace").to_string(),
            "failed to attach image: PathOutsideWorkspace"
        );
        // A symlink that stays inside is the file it names.
        std::os::unix::fs::symlink(
            tree.workspace.join("shot.png"),
            tree.workspace.join("in.png"),
        )
        .expect("a symlink");
        let (path, ..) = load(&tree.workspace, Some(&tree.home), "in.png").expect("attached");
        assert_eq!(path, tree.workspace.join("shot.png"));
    }

    #[test]
    fn every_refusal_says_upstreams_words() {
        let tree = tree();
        let refused = |input: &str| {
            load(&tree.workspace, Some(&tree.home), input)
                .expect_err(input)
                .to_string()
        };
        assert_eq!(refused("nope.png"), "image file not found");
        assert_eq!(refused("notes.txt"), "unsupported image type");
        assert_eq!(refused("sub"), "failed to attach image: NotRegularFile");
        assert_eq!(refused(""), "failed to attach image: InvalidPath");
        // Over the ceiling by one byte, with a real PNG signature: refused by
        // its size, not its contents.
        let huge = tree.workspace.join("huge.png");
        let file = std::fs::File::create(&huge).expect("huge");
        file.set_len(MAX_IMAGE_BYTES + 1).expect("sparse");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&huge)
            .and_then(|mut file| std::io::Write::write_all(&mut file, &SHOT_PNG[..8]))
            .expect("a signature");
        assert_eq!(refused("huge.png"), "image exceeds the 20 MiB limit");
        // And exactly at it, accepted.
        file.set_len(MAX_IMAGE_BYTES).expect("sparse");
        assert!(load(&tree.workspace, Some(&tree.home), "huge.png").is_ok());
    }

    #[test]
    fn the_bytes_are_read_once_and_a_later_change_to_the_file_is_not_them() {
        let tree = tree();
        let (_, _, bytes) = load(&tree.workspace, Some(&tree.home), "shot.png").expect("attached");
        let image = Image::new(
            1,
            tree.workspace.join("shot.png"),
            MediaType::Png,
            Arc::from(bytes),
        );
        std::fs::write(tree.workspace.join("shot.png"), b"GIF89a replaced").expect("replace");
        assert_eq!(&*image.bytes, SHOT_PNG);
        assert_eq!(image.sha256, digest(SHOT_PNG));
    }
}
