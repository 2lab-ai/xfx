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
//! - **Whether it may be read** is the configured read table's to say, asked
//!   about the file the path resolved to before it is opened ([`load`]):
//!   upstream's attach asks no rule, and xfx's does, so a path the table keeps
//!   from `read_file` is not sent by being named in a paste.
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

use crate::permission::{RuleAction, RuleSet};

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
    /// copy written when it was pasted, named [`Self::snapshot_name`], which
    /// is what upstream names and links one by (`image_commands.zig:138-147`).
    pub path: PathBuf,
    pub media_type: MediaType,
    /// What was read when it was attached, and nothing read since.
    ///
    /// `Arc` because the same bytes are in the composer's badge, in the turn
    /// that sends them, and in the history every later request re-sends.
    pub bytes: Arc<[u8]>,
    /// The SHA-256 of [`Self::bytes`], lowercase hex.
    pub sha256: String,
    /// Whether [`Self::path`] is the copy written when it was pasted off the
    /// clipboard rather than a file the user named: the session then records
    /// it by its saved copy, which outlives the paste's.
    pub copied: bool,
}

impl Image {
    /// This image as a part of the user message that carries it, with its
    /// saved copy at `saved` when there is one.
    pub fn part(&self, saved: Option<Arc<Path>>) -> crate::gateway::protocol::ContentPart {
        crate::gateway::protocol::ContentPart::Image {
            id: self.id,
            media_type: self.media_type,
            data: Arc::clone(&self.bytes),
            saved,
        }
    }

    pub fn new(id: u32, path: PathBuf, media_type: MediaType, bytes: Arc<[u8]>) -> Self {
        let sha256 = digest(&bytes);
        Self {
            id,
            path,
            media_type,
            bytes,
            sha256,
            copied: false,
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

/// The `[Image #N]` `text` begins with, as its number and its length in
/// bytes (`matchImagePlaceholder`, `image_attachments.zig:1388-1403`): the
/// prefix, one or more digits, and `]`. A number too large for an id names no
/// image this session could have, and is not one.
pub fn placeholder_at(text: &str) -> Option<(u32, usize)> {
    const PREFIX: &str = "[Image #";
    let after = text.strip_prefix(PREFIX)?;
    let digits = after.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || after.as_bytes().get(digits) != Some(&b']') {
        return None;
    }
    let id = after[..digits].parse::<u32>().ok()?;
    Some((id, PREFIX.len() + digits + 1))
}

/// Every `[Image #N]` in `text`, by its number, in the order they stand
/// ([`placeholder_at`]).
pub fn placeholder_ids(text: &str) -> impl Iterator<Item = u32> + '_ {
    let mut rest = text;
    std::iter::from_fn(move || loop {
        let at = rest.find("[Image #")?;
        match placeholder_at(&rest[at..]) {
            Some((id, len)) => {
                rest = &rest[at + len..];
                return Some(id);
            }
            None => rest = &rest[at + 1..],
        }
    })
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
    /// A configured rule of the read table denies reading the file
    /// ([`load`]): the path the rule judged, made safe to paint. Upstream has
    /// no such refusal, because its attach asks no rule.
    Denied(String),
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
            Self::Denied(_) => "PermissionRuleDenied",
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
            Self::Denied(path) => write!(
                f,
                "failed to attach image: a configured permission rule denies reading `{path}`"
            ),
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
        let root = canonical_root(workspace);
        if !real.starts_with(&root) {
            return Err(AttachError::Other("PathOutsideWorkspace"));
        }
    }
    Ok(real)
}

/// The workspace as the filesystem names it, so a root reached through a
/// symlink of its own (`/tmp` on macOS) still contains its files.
fn canonical_root(workspace: &Path) -> PathBuf {
    workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
}

/// Whether the configured read table lets `real` -- a resolved, real path --
/// be read for an attachment. Asked before the file is opened, and again of
/// the file the open reached ([`load_judged`]).
///
/// It is judged as `read_file` is judged reading the same file
/// (`crate::tools::read`'s `admitted`): under the `read` category and the
/// `read_file` name, at the path a rule names it by -- relative inside the
/// workspace, absolute outside it ([`rule_path`]). Only a deny refuses:
///
/// - an `ask` asks nobody, because the attach is the person's own act and
///   they are who would be asked;
/// - in every permission mode, `yolo` too: a deny is a rule the person wrote,
///   and failing closed costs only the attachment.
///
/// Upstream's attach consults no rule (`load_inline_image_attachment`,
/// `image_attachments.zig:1880-1885`, into `readImageHeaderBytes`,
/// `:1825-1834`); copying that would let a path named in a paste send a file
/// the table keeps from the model.
///
/// [`rule_path`]: crate::permission::rules::rule_path
fn admitted(rules: &RuleSet, workspace: &Path, real: &Path) -> Result<(), AttachError> {
    use crate::permission::rules::{rule_path, READ};
    let candidate = rule_path(&canonical_root(workspace), real);
    match rules.decide(READ, Some("read_file"), &candidate) {
        Some(RuleAction::Deny) => Err(AttachError::Denied(
            crate::workspace::access::terminal_safe(&String::from_utf8_lossy(&candidate)),
        )),
        Some(RuleAction::Allow | RuleAction::Ask) | None => Ok(()),
    }
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
/// The file is judged by `lstat` before it is opened: anything but a regular
/// file -- a link included, since `path` is already the real path a link
/// would have been followed to -- is refused (`NotRegularFile`), and so is a
/// file over [`MAX_IMAGE_BYTES`], without being opened at all. Then it is
/// opened without following a link or waiting ([`open_regular`]) and judged
/// again by its own descriptor, so a link or a FIFO put in its place between
/// the two is refused as well rather than followed or waited on -- on the
/// thread that holds the terminal, for a paste. The read itself is bounded
/// too, so a file that grew in the meantime is refused rather than read past
/// the ceiling.
pub fn read(path: &Path) -> Result<(MediaType, Vec<u8>), AttachError> {
    read_admitted(path, |_| Ok(()))
}

/// [`read`], with `admit` asked about the file once it is open and before a
/// byte of it is read: what binds a judgment to the file actually read.
fn read_admitted(
    path: &Path,
    admit: impl FnOnce(&File) -> Result<(), AttachError>,
) -> Result<(MediaType, Vec<u8>), AttachError> {
    let judge = |metadata: &std::fs::Metadata| {
        if !metadata.is_file() {
            return Err(AttachError::Other("NotRegularFile"));
        }
        if metadata.len() > MAX_IMAGE_BYTES {
            return Err(AttachError::TooLarge);
        }
        Ok(())
    };
    judge(&std::fs::symlink_metadata(path).map_err(not_regular)?)?;
    let mut file = open_regular(path)?;
    let metadata = file.metadata().map_err(|err| AttachError::from_io(&err))?;
    judge(&metadata)?;
    admit(&file)?;
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

/// An I/O failure of the path a file was expected at, named as upstream's
/// vision open names it (`openVisionRegularFile`, `image_attachments.zig:
/// 414-418`, `:452-455`): a link where the file was (`ELOOP`), a file where a
/// directory was (`ENOTDIR`), and a socket, which cannot be opened at all
/// (`ENXIO` on Linux, `EOPNOTSUPP` on macOS), are all `NotRegularFile`.
fn not_regular(err: io::Error) -> AttachError {
    #[cfg(unix)]
    if let Some(libc::ELOOP | libc::ENOTDIR | libc::ENXIO | libc::EOPNOTSUPP) = err.raw_os_error() {
        return AttachError::Other("NotRegularFile");
    }
    AttachError::from_io(&err)
}

/// `path` opened for reading as upstream opens a vision source
/// (`openVisionRegularFile`, `image_attachments.zig:408-469`): `O_NOFOLLOW`,
/// so a link put at the real path is refused rather than followed;
/// `O_NONBLOCK`, so a FIFO with no writer opens at once instead of holding the
/// thread; `O_NOCTTY`, so a terminal never becomes this process's; and
/// `O_CLOEXEC`, which the standard library always sets. Then judged by its own
/// `fstat` -- refused unless it is a regular file -- and made blocking again.
#[cfg(unix)]
fn open_regular(path: &Path) -> Result<File, AttachError> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
        .map_err(not_regular)?;
    if !file.metadata().map_err(not_regular)?.is_file() {
        return Err(AttachError::Other("NotRegularFile"));
    }
    let flags = rustix::fs::fcntl_getfl(&file).map_err(|err| not_regular(err.into()))?;
    rustix::fs::fcntl_setfl(&file, flags & !rustix::fs::OFlags::NONBLOCK)
        .map_err(|err| not_regular(err.into()))?;
    Ok(file)
}

/// Without `O_NOFOLLOW` and `O_NONBLOCK`, the `lstat` [`read`] judged the path
/// by before this open is what keeps a link or a special file from being
/// opened.
#[cfg(not(unix))]
fn open_regular(path: &Path) -> Result<File, AttachError> {
    File::open(path).map_err(|err| AttachError::from_io(&err))
}

/// Where the file `file` is open on is, as the kernel names it now: every
/// link on the way already followed, whatever the path that was opened said
/// by then. `fcntl(F_GETPATH)` on Apple's systems.
#[cfg(target_vendor = "apple")]
fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let path = rustix::fs::getpath(file).map_err(io::Error::from)?;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(
        path.into_bytes(),
    )))
}

/// [`opened_path`] on Linux: the link `/proc/self/fd/<fd>` is. A file unlinked
/// since it was opened reads ` (deleted)` after its old path, which no longer
/// names it, so it is refused as gone.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let path = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
    if path.as_os_str().as_bytes().ends_with(b" (deleted)") {
        return Err(io::ErrorKind::NotFound.into());
    }
    Ok(path)
}

/// [`opened_path`] where no call names an open file's path: unknown, so a
/// table with rules in it refuses the attachment rather than read a file it
/// could not judge.
#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
fn opened_path(_: &File) -> io::Result<PathBuf> {
    Err(io::ErrorKind::Unsupported.into())
}

/// `/image <path>`'s whole question: the path typed, as the file it names and
/// the bytes in it -- unless the configured read table `rules` denies reading
/// that file ([`AttachError::Denied`]), which is asked before it is opened.
pub fn load(
    workspace: &Path,
    home: Option<&Path>,
    rules: &RuleSet,
    input: &str,
) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
    load_literal(workspace, home, rules, &normalize(input))
}

/// [`load`] for a path that is already the file's name, quotes and escapes
/// undone -- a canonical `@"…"` word's payload (`load_user_image_literal`,
/// `image_attachments.zig:209-224`): resolved as it stands, never normalized a
/// second time.
pub fn load_literal(
    workspace: &Path,
    home: Option<&Path>,
    rules: &RuleSet,
    path: &str,
) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
    load_judged(workspace, home, rules, path, |_| {})
}

/// [`load_literal`], with `before_open` run between the table's judgment of
/// the resolved path and the open: nothing, in the product; in a test, the
/// swap of what the path names that the judgment must not be fooled by.
///
/// The table is asked twice, and either deny refuses: about the path resolved
/// -- every link followed -- before anything is opened, and about the file
/// the open reached, by its descriptor's own path ([`opened_path`]), before a
/// byte of it is read. The second is what holds when the first was told the
/// truth and the filesystem changed after: a directory on the way to the
/// resolved path swapped for a link reaches another file, and that file is the
/// one judged. (A link put at the resolved path itself is not followed at all:
/// [`read`] refuses it as `NotRegularFile`.) The open opens the resolved path, which a
/// person may still have spelled through links of their own; the judgment is
/// always about the file, never the spelling, as `read_file`'s is
/// ([`crate::permission::ReadPlan`]).
///
/// `read_file` judges once, and its `stat` and open are two more resolutions
/// of the same path (`crate::tools::read`, the bargain it states there); this
/// is the stronger of the two. With no rule in the table, nothing can deny,
/// and the second question is not asked.
fn load_judged(
    workspace: &Path,
    home: Option<&Path>,
    rules: &RuleSet,
    path: &str,
    before_open: impl FnOnce(&Path),
) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
    let path = resolve(workspace, home, path)?;
    admitted(rules, workspace, &path)?;
    before_open(&path);
    let (media_type, bytes) = read_admitted(&path, |file| {
        if rules.is_empty() {
            return Ok(());
        }
        let opened = opened_path(file).map_err(|err| AttachError::from_io(&err))?;
        admitted(rules, workspace, &opened)
    })?;
    Ok((path, media_type, bytes))
}

// ---------------------------------------------------------------------------
// what one request may carry
// ---------------------------------------------------------------------------

/// The most base64 one image may be on the wire (`max_encoded_image_bytes`,
/// `image_data.zig:5`): 5 MiB, so 3.75 MiB of image.
pub const MAX_ENCODED_BYTES: usize = 5 * 1024 * 1024;

/// The most pixels per side an image may have in a request of more than
/// [`STRICT_IMAGE_COUNT`] images, and in one of up to that many
/// (`max_image_dimension`, `max_single_image_dimension`, `strict_image_count`,
/// `image_data.zig:157-159`): the strictest provider's limits.
pub const MAX_DIMENSION: u32 = 2000;
pub const MAX_SINGLE_DIMENSION: u32 = 8000;
pub const STRICT_IMAGE_COUNT: usize = 20;

/// Whether `raw` bytes are within [`MAX_ENCODED_BYTES`] once encoded
/// (`fitsEncodedImageLimit`, `image_data.zig:180-183`).
pub fn fits_encoded(raw: usize) -> bool {
    raw.checked_add(2)
        .map(|padded| padded / 3)
        .and_then(|groups| groups.checked_mul(4))
        .is_some_and(|encoded| encoded <= MAX_ENCODED_BYTES)
}

/// The pixel limit for a request carrying `count` images
/// (`requestMaxDimension`, `image_data.zig:166-168`).
pub fn request_max_dimension(count: usize) -> u32 {
    if count > STRICT_IMAGE_COUNT {
        MAX_DIMENSION
    } else {
        MAX_SINGLE_DIMENSION
    }
}

/// An image's size in pixels, read from its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    fn exceeds(self, limit: u32) -> bool {
        self.width > limit || self.height > limit
    }
}

/// The pixel size `bytes` declare, without decoding a pixel
/// (`imageDimensions`, `image_data.zig:194-368`), or `None` for a header that
/// is truncated, malformed, or says zero.
pub fn dimensions(bytes: &[u8]) -> Option<Dimensions> {
    let sized = |width: u32, height: u32| {
        (width != 0 && height != 0).then_some(Dimensions { width, height })
    };
    let be16 = |at: usize| {
        Some(u32::from(u16::from_be_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
    let le16 = |at: usize| {
        Some(u32::from(u16::from_le_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
        ])))
    };
    let le24 = |at: usize| {
        Some(u32::from_le_bytes([
            *bytes.get(at)?,
            *bytes.get(at + 1)?,
            *bytes.get(at + 2)?,
            0,
        ]))
    };
    match MediaType::sniff(bytes)? {
        MediaType::Png => {
            if bytes.get(12..16)? != b"IHDR" {
                return None;
            }
            let be32 =
                |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
            sized(be32(16)?, be32(20)?)
        }
        MediaType::Gif => sized(le16(6)?, le16(8)?),
        MediaType::Webp => match bytes.get(12..16)? {
            b"VP8 " => {
                if bytes.get(23..26)? != b"\x9d\x01\x2a" {
                    return None;
                }
                sized(le16(26)? & 0x3fff, le16(28)? & 0x3fff)
            }
            b"VP8L" => {
                if *bytes.get(20)? != 0x2f {
                    return None;
                }
                let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
                sized((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
            }
            b"VP8X" => sized(le24(24)? + 1, le24(27)? + 1),
            _ => None,
        },
        MediaType::Jpeg => {
            // The marker walk to the first frame header, bounded as upstream
            // bounds it (`jpegDimensions`, `max_jpeg_segments`).
            let mut offset = 2usize;
            for _ in 0..4096 {
                if *bytes.get(offset)? != 0xff {
                    return None;
                }
                let marker = *bytes.get(offset + 1)?;
                match marker {
                    0xff => offset += 1,
                    0x01 | 0xd0..=0xd8 => offset += 2,
                    0xd9 | 0xda => return None,
                    _ => {
                        let length = be16(offset + 2)? as usize;
                        if length < 2 {
                            return None;
                        }
                        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                            return sized(be16(offset + 7)?, be16(offset + 5)?);
                        }
                        offset = offset.checked_add(2 + length)?;
                    }
                }
            }
            None
        }
    }
}

/// The extension a smaller copy of `media_type` is saved under
/// (`mediaTypeExtension`, `image_attachments.zig:869-880`): the saved copy
/// is a `.bin`, which hides the type from a tool that picks a decoder by name.
fn extension(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Png => ".png",
        MediaType::Jpeg => ".jpg",
        MediaType::Gif => ".gif",
        MediaType::Webp => ".webp",
    }
}

/// What the prompt says in place of an image a request does not carry
/// (`writeWithheldAttachmentNotice`, `image_attachments.zig:841-868`), in
/// upstream's sentences. Upstream's `source_ref` branch (a host reference
/// with no bytes) has no counterpart: an xfx image is always bytes.
///
/// The note tells the model to `read_file the copy`, and `read_file` returns
/// an image as upstream's does ([`read_file_image`]), on either wire.
pub fn withheld_note(
    id: u32,
    media_type: MediaType,
    size: Option<Dimensions>,
    limit: u32,
    saved: Option<&Path>,
) -> String {
    let mut note = match size {
        Some(size) => format!(
            "[Image #{id} not sent: {} is {}x{} pixels. This request permits at most {limit} per \
             side and 5 MiB encoded per image. ",
            media_type.label(),
            size.width,
            size.height
        ),
        None => format!("[Image #{id} not sent: its dimensions could not be verified. "),
    };
    match saved {
        Some(path) => note.push_str(&format!(
            "The original is saved at {}. Use an available image tool to save a smaller copy to \
             a new file ending in {}, then read_file the copy. If no image tool is available, \
             ask the user before installing one.]\n",
            path.display(),
            extension(media_type)
        )),
        None => {
            note.push_str("No local image file is available; ask the user for a smaller copy.]\n")
        }
    }
    note
}

/// `messages` as one request may carry them: every image too large to send
/// -- over [`MAX_ENCODED_BYTES`] once encoded, over the request's pixel limit
/// ([`request_max_dimension`] of how many images it carries), or with a size
/// its header does not say -- taken out, and its [`withheld_note`] put in
/// front of the text of the message it was on (`withholdOversizedAttachments`,
/// `image_attachments.zig:882-920`).
///
/// A projection of one request: the history and the log keep every image,
/// and a later request decides again.
pub fn withhold(
    mut messages: Vec<crate::gateway::protocol::Message>,
) -> Vec<crate::gateway::protocol::Message> {
    use crate::gateway::protocol::ContentPart;
    // Every image the request carries counts, a tool result's with the
    // user's (`countRequestImages`, `image_data.zig:171-178`).
    let count = messages
        .iter()
        .flat_map(|message| message.content.iter())
        .map(|part| match part {
            ContentPart::Image { .. } => 1,
            ContentPart::ToolResult { images, .. } => images.len(),
            _ => 0,
        })
        .sum::<usize>();
    if count == 0 {
        return messages;
    }
    let limit = request_max_dimension(count);
    for message in &mut messages {
        // A tool result's own images, with upstream's note for each in front
        // of its output (`withholdRequestToolImages`,
        // `execution_memory.zig:730-766`).
        for part in &mut message.content {
            let ContentPart::ToolResult { output, images, .. } = part else {
                continue;
            };
            let mut notes = String::new();
            images.retain(|image| {
                let size = dimensions(&image.data);
                if size.is_some_and(|size| !size.exceeds(limit)) && fits_encoded(image.data.len()) {
                    return true;
                }
                notes.push_str(&withheld_tool_note(image.media_type, size, limit));
                false
            });
            output.insert_str(0, &notes);
        }
        let mut notes = String::new();
        message.content.retain(|part| {
            let ContentPart::Image {
                id,
                media_type,
                data,
                saved,
            } = part
            else {
                return true;
            };
            let size = dimensions(data);
            if size.is_some_and(|size| !size.exceeds(limit)) && fits_encoded(data.len()) {
                return true;
            }
            notes.push_str(&withheld_note(
                *id,
                *media_type,
                size,
                limit,
                saved.as_deref(),
            ));
            false
        });
        if notes.is_empty() {
            continue;
        }
        match message.content.iter_mut().find_map(|part| match part {
            ContentPart::Text { text } => Some(text),
            _ => None,
        }) {
            Some(text) => text.insert_str(0, &notes),
            None => message.content.insert(0, ContentPart::Text { text: notes }),
        }
    }
    messages
}

/// What a tool result says in place of an image of its own that a request
/// does not carry (`withholdRequestToolImages`, `execution_memory.zig:745-756`).
/// Upstream's sentences; its host-reference branch has no counterpart here.
pub fn withheld_tool_note(media_type: MediaType, size: Option<Dimensions>, limit: u32) -> String {
    let mut note = match size {
        Some(size) => format!(
            "[Image not sent: {} is {}x{} pixels; this request permits at most {limit} per side \
             and 5 MiB encoded per image. ",
            media_type.label(),
            size.width,
            size.height
        ),
        None => "[Image not sent: its dimensions could not be verified. ".to_string(),
    };
    note.push_str(
        "If this tool result names a local file, use an available image tool to save a smaller \
         copy, then read_file the copy. If no tool or path is available, ask the user; ask \
         before installing software.]\n",
    );
    note
}

// ---------------------------------------------------------------------------
// an image a tool hands the model
// ---------------------------------------------------------------------------

/// An image a tool returned with its result: `read_file` of an image file
/// (`imageToolResult`, `read_file.zig:228-278`). The model sees it after the
/// result's text -- on the Gateway in a user message after the tool results,
/// on llmux inside the `tool_result` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolImage {
    pub media_type: MediaType,
    pub data: Arc<[u8]>,
    /// The file it was read from, as the tool named it.
    pub path: String,
}

/// The text a user message carries the tool results' images under on the
/// Gateway (`tool_image_followup_text`, `vercel_protocol.zig:677`).
pub const TOOL_IMAGES_FOLLOWUP: &str = "Attached image(s) from the tool result.";

/// What a tool result says in place of its images for a model that does not
/// read them inline, on the Gateway (`projectToolImageMessages`,
/// `gateway_step.zig:256-283`): a model the catalog lists without `vision` and
/// `file-input`, with no `vision` tool to fall back on.
pub const TOOL_IMAGES_NOT_NATIVE: &str = "[Tool images were retained but not sent: this model does not accept inline images and no vision fallback is available. Ask the user to attach the image directly or switch to a vision-capable model.]\n";

/// The same, for a model whose image input nobody can vouch for: unlisted,
/// or the catalog unread. Upstream's sentence with this product's name.
pub const TOOL_IMAGES_UNKNOWN: &str = "[Tool images were retained but not sent: xfx could not confirm image input support for this model (the model is not listed in the model catalog, or the catalog is unavailable). This can recover later in the session, so a retry may succeed; otherwise ask the user to attach the image directly.]\n";

/// Every tool result's images in `request` taken out, `note` put in front of
/// each output that had any (`projectToolImageMessages`,
/// `gateway_step.zig:256-283`). `None` when there were none to take.
pub fn strip_tool_images(
    request: &crate::gateway::protocol::CompletionRequest,
    note: &str,
) -> Option<crate::gateway::protocol::CompletionRequest> {
    use crate::gateway::protocol::ContentPart;
    let carries = request.messages.iter().any(|message| {
        message.content.iter().any(
            |part| matches!(part, ContentPart::ToolResult { images, .. } if !images.is_empty()),
        )
    });
    if !carries {
        return None;
    }
    let mut stripped = request.clone();
    for message in &mut stripped.messages {
        for part in &mut message.content {
            if let ContentPart::ToolResult { output, images, .. } = part {
                if !images.is_empty() {
                    images.clear();
                    output.insert_str(0, note);
                }
            }
        }
    }
    Some(stripped)
}

/// What `read_file` returns for a file whose bytes are an image
/// (`imageToolResult`, `read_file.zig:228-278`), or `None` for bytes that are
/// not one -- which `read_file` names as binary.
///
/// An image the request could carry -- read whole, its size known, at most
/// 8000 pixels a side and 5 MiB once encoded -- is attached, the text saying
/// so; any other is not, and the text says why and what the model can do.
/// `complete` is whether `bytes` is the whole file.
pub fn read_file_image(
    display: &str,
    bytes: &[u8],
    file_size: u64,
    complete: bool,
) -> Option<(String, Option<ToolImage>)> {
    let media_type = MediaType::sniff(bytes)?;
    let size = dimensions(bytes);
    let reason = if !complete {
        Some("the file exceeds read_file's 10 MiB read limit")
    } else if size.is_none() {
        Some("the image dimensions could not be verified")
    } else if !fits_encoded(bytes.len()) {
        Some("the image exceeds the 5 MiB encoded attach limit")
    } else if size.is_some_and(|size| size.exceeds(MAX_SINGLE_DIMENSION)) {
        Some("the image exceeds 8000 pixels per side")
    } else {
        None
    };
    let label = media_type.label();
    Some(match reason {
        Some(reason) => (
            format!(
                "<path>{display}</path>\n<content>image not attached: {label} ({file_size} \
                 bytes); {reason}. Use an available image tool to save a smaller copy to a new \
                 file, then read_file the copy. If no image tool is available, ask the user \
                 before installing one.</content>"
            ),
            None,
        ),
        None => (
            format!(
                "<path>{display}</path>\n<content>image attached ({label}, {file_size} \
                 bytes)</content>"
            ),
            Some(ToolImage {
                media_type,
                data: Arc::from(bytes),
                path: display.to_string(),
            }),
        ),
    })
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

    /// A PNG signature and an `IHDR` declaring `width` x `height`: enough for
    /// the type and the size, not for a decoder (`testPngHeader`,
    /// `image_data.zig:370-379`).
    pub(crate) fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    /// What upstream sent for it: `data` of the first `file` part in
    /// `cap/fx-image-40x120-bodies/001-completion.json`.
    pub(crate) const SHOT_PNG_BASE64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR4nGP4z8AARwzEcQCukw/x0F8jngAAAABJRU5ErkJggg==";
}

#[cfg(test)]
mod tests {
    use super::fixture::{SHOT_PNG, SHOT_PNG_BASE64};
    use super::*;

    use super::fixture::png_header;

    /// Upstream's JPEG fixture: SOI, APP0, DHT, a fill byte, a progressive
    /// frame header and EOI, so the walk crosses every kind of marker
    /// (`testJpeg`, `image_data.zig:380-397`).
    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00\xff\xc4\x00\x04\x00\x00\xff\xff\xc2\x00\x11\x08\x00\x00\x00\x00\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01\xff\xd9".to_vec();
        bytes[32..34].copy_from_slice(&height.to_be_bytes());
        bytes[34..36].copy_from_slice(&width.to_be_bytes());
        bytes
    }

    #[test]
    fn a_size_is_read_from_each_formats_header_and_a_bad_one_is_none() {
        let size = |width, height| Some(Dimensions { width, height });
        assert_eq!(dimensions(fixture::SHOT_PNG), size(4, 4));
        assert_eq!(dimensions(&png_header(3420, 2224)), size(3420, 2224));
        assert_eq!(dimensions(&jpeg(8001, 600)), size(8001, 600));
        assert_eq!(dimensions(b"GIF89a\x40\x01\xf0\x00"), size(320, 240));
        let mut vp8x = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        vp8x.extend_from_slice(&[0x3f, 0x1f, 0x00, 0x7f, 0x3e, 0x00]);
        assert_eq!(dimensions(&vp8x), size(8000, 16000));
        let mut vp8l = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        vp8l.extend_from_slice(&(99u32 | (49u32 << 14)).to_le_bytes());
        assert_eq!(dimensions(&vp8l), size(100, 50));
        let mut vp8 = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\x9d\x01\x2a".to_vec();
        vp8.extend_from_slice(&[0x80, 0x02, 0xe0, 0x01]);
        assert_eq!(dimensions(&vp8), size(640, 480));
        // Upstream's malformed and truncated headers (`image_data.zig:493-510`).
        assert_eq!(dimensions(&png_header(0, 10)), None);
        assert_eq!(dimensions(&png_header(10, 10)[..23]), None);
        let mut not_ihdr = png_header(10, 10);
        not_ihdr[12..16].copy_from_slice(b"IDAT");
        assert_eq!(dimensions(&not_ihdr), None);
        assert_eq!(dimensions(&jpeg(10, 10)[..35]), None);
        let mut scan_first = jpeg(10, 10);
        scan_first[3] = 0xda;
        assert_eq!(dimensions(&scan_first), None);
        let mut short_length = jpeg(10, 10);
        short_length[4..6].copy_from_slice(&1u16.to_be_bytes());
        assert_eq!(dimensions(&short_length), None);
        assert_eq!(dimensions(b"\xff\xd8\xff\xd9"), None);
        assert_eq!(dimensions(b"GIF89a\x01"), None);
        assert_eq!(dimensions(b"not an image"), None);
    }

    #[test]
    fn the_encoded_ceiling_and_the_pixel_limit_are_upstreams() {
        // `fitsEncodedImageLimit` (`image_data.zig:476-480`).
        let max_raw = MAX_ENCODED_BYTES / 4 * 3;
        assert!(fits_encoded(max_raw));
        assert!(!fits_encoded(max_raw + 1));
        assert!(!fits_encoded(usize::MAX));
        assert_eq!(request_max_dimension(1), 8000);
        assert_eq!(request_max_dimension(20), 8000);
        assert_eq!(request_max_dimension(21), 2000);
    }

    fn image_part(
        id: u32,
        bytes: Vec<u8>,
        saved: Option<&str>,
    ) -> crate::gateway::protocol::ContentPart {
        crate::gateway::protocol::ContentPart::Image {
            id,
            media_type: MediaType::sniff(&bytes).unwrap_or(MediaType::Png),
            data: Arc::from(bytes),
            saved: saved.map(|path| Arc::from(Path::new(path))),
        }
    }

    fn images_on(message: &crate::gateway::protocol::Message) -> Vec<u32> {
        message
            .content
            .iter()
            .filter_map(|part| match part {
                crate::gateway::protocol::ContentPart::Image { id, .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_image_a_request_cannot_carry_is_withheld_and_named_in_front_of_the_text() {
        // `withholdOversizedAttachments` and `writeWithheldAttachmentNotice`
        // (`image_attachments.zig:841-920`): the image stays out, the text
        // says which and why in upstream's sentences, and its neighbour that
        // fits is sent.
        use crate::gateway::protocol::Message;
        let wide = "/h/.xfx/sessions/s/images/image-2-0123456789abcdef.bin";
        let messages = vec![
            Message::system("rules"),
            Message::user_with_images(
                "[Image #1][Image #2] compare",
                [
                    image_part(1, fixture::SHOT_PNG.to_vec(), None),
                    image_part(2, png_header(8001, 10), Some(wide)),
                ],
            ),
        ];
        let sent = withhold(messages);
        assert_eq!(images_on(&sent[1]), [1]);
        assert_eq!(
            sent[1].text(),
            format!(
                "[Image #2 not sent: image/png is 8001x10 pixels. This request permits at most \
                 8000 per side and 5 MiB encoded per image. The original is saved at {wide}. Use \
                 an available image tool to save a smaller copy to a new file ending in .png, then \
                 read_file the copy. If no image tool is available, ask the user before installing \
                 one.]\n[Image #1][Image #2] compare"
            )
        );
        assert_eq!(
            sent[0].text(),
            "rules",
            "a message with no image is left alone"
        );
    }

    #[test]
    fn an_image_over_the_encoded_ceiling_or_of_no_known_size_is_withheld_too() {
        use crate::gateway::protocol::Message;
        let mut big = png_header(100, 100);
        big.resize(MAX_ENCODED_BYTES / 4 * 3 + 1, 0);
        let mut no_size = png_header(10, 10);
        no_size[12..16].copy_from_slice(b"XXXX");
        let sent = withhold(vec![Message::user_with_images(
            "[Image #3][Image #4] look",
            [image_part(3, big, None), image_part(4, no_size, None)],
        )]);
        assert!(images_on(&sent[0]).is_empty());
        assert_eq!(
            sent[0].text(),
            "[Image #3 not sent: image/png is 100x100 pixels. This request permits at most 8000 \
             per side and 5 MiB encoded per image. No local image file is available; ask the user \
             for a smaller copy.]\n[Image #4 not sent: its dimensions could not be verified. No \
             local image file is available; ask the user for a smaller copy.]\n[Image #3][Image \
             #4] look"
        );
        // A JPEG is saved under upstream's extension for one.
        let jpg = withheld_note(9, MediaType::Jpeg, None, 8000, Some(Path::new("/s/x.bin")));
        assert!(jpg.contains("a new file ending in .jpg,"), "{jpg}");
    }

    #[test]
    fn past_twenty_images_a_request_permits_two_thousand_pixels_a_side() {
        use crate::gateway::protocol::Message;
        let turn = |count: u32| {
            Message::user_with_images(
                "pictures",
                (1..=count).map(|id| image_part(id, png_header(2001, 10), None)),
            )
        };
        // Twenty: every one is within 8000.
        assert_eq!(images_on(&withhold(vec![turn(20)])[0]).len(), 20);
        // Twenty-one, counted across the whole request: none is within 2000.
        let sent = withhold(vec![
            turn(20),
            Message::assistant(Some("ok"), Vec::new()),
            turn(1),
        ]);
        assert!(images_on(&sent[0]).is_empty());
        assert!(images_on(&sent[2]).is_empty());
        assert!(
            sent[2].text().contains("at most 2000 per side"),
            "{}",
            sent[2].text()
        );
    }

    #[test]
    fn read_file_attaches_an_image_a_request_can_carry_and_says_why_not_otherwise() {
        // `imageToolResult` (`read_file.zig:228-278`) and its cases
        // (`:728-890`).
        let (text, image) =
            read_file_image("shot.png", fixture::SHOT_PNG, 73, true).expect("an image");
        assert_eq!(
            text,
            "<path>shot.png</path>\n<content>image attached (image/png, 73 bytes)</content>"
        );
        let image = image.expect("attached");
        assert_eq!(image.media_type, MediaType::Png);
        assert_eq!(&image.data[..], fixture::SHOT_PNG);
        let refused = |bytes: &[u8], complete: bool| {
            let (text, image) =
                read_file_image("p.png", bytes, bytes.len() as u64, complete).expect("an image");
            assert!(image.is_none(), "{text}");
            text
        };
        let edge = png_header(8000, 8000);
        assert!(read_file_image("e.png", &edge, 24, true)
            .expect("an image")
            .1
            .is_some());
        let wide = refused(&png_header(8001, 1), true);
        assert_eq!(
            wide,
            "<path>p.png</path>\n<content>image not attached: image/png (24 bytes); the image \
             exceeds 8000 pixels per side. Use an available image tool to save a smaller copy to \
             a new file, then read_file the copy. If no image tool is available, ask the user \
             before installing one.</content>"
        );
        let mut no_size = png_header(10, 10);
        no_size[12..16].copy_from_slice(b"IDAT");
        assert!(refused(&no_size, true).contains("the image dimensions could not be verified."));
        let mut big = png_header(10, 10);
        big.resize(MAX_ENCODED_BYTES / 4 * 3 + 1, 0);
        assert!(refused(&big, true).contains("the image exceeds the 5 MiB encoded attach limit."));
        assert!(refused(fixture::SHOT_PNG, false)
            .contains("the file exceeds read_file's 10 MiB read limit."));
        assert_eq!(read_file_image("a.bin", b"\x00\x01binary", 8, true), None);
    }

    fn tool_message(images: Vec<ToolImage>) -> crate::gateway::protocol::Message {
        crate::gateway::protocol::Message::tool_result_with_images(
            "c1",
            "read_file",
            "seen",
            images,
        )
    }

    fn tool_image(bytes: Vec<u8>) -> ToolImage {
        ToolImage {
            media_type: MediaType::Png,
            data: Arc::from(bytes),
            path: "x.png".to_string(),
        }
    }

    fn tool_parts(message: &crate::gateway::protocol::Message) -> (String, usize) {
        match &message.content[0] {
            crate::gateway::protocol::ContentPart::ToolResult { output, images, .. } => {
                (output.clone(), images.len())
            }
            other => panic!("not a result: {other:?}"),
        }
    }

    #[test]
    fn a_tool_image_counts_toward_the_request_and_is_withheld_with_upstreams_tool_note() {
        // `countRequestImages` counts both kinds (`image_data.zig:171-178`);
        // `withholdRequestToolImages` writes its own note
        // (`execution_memory.zig:730-766`).
        use crate::gateway::protocol::Message;
        let users = Message::user_with_images(
            "many",
            (1..=20).map(|id| image_part(id, fixture::SHOT_PNG.to_vec(), None)),
        );
        let sent = withhold(vec![
            users,
            tool_message(vec![tool_image(png_header(2001, 10))]),
        ]);
        assert_eq!(images_on(&sent[0]).len(), 20, "the user images fit 2000");
        assert_eq!(
            tool_parts(&sent[1]),
            (
                "[Image not sent: image/png is 2001x10 pixels; this request permits at most 2000 \
                 per side and 5 MiB encoded per image. If this tool result names a local file, \
                 use an available image tool to save a smaller copy, then read_file the copy. If \
                 no tool or path is available, ask the user; ask before installing software.]\n\
                 seen"
                    .to_string(),
                0
            )
        );
        // Alone in a request, the same image is within 8000 and goes.
        let alone = withhold(vec![tool_message(vec![tool_image(png_header(2001, 10))])]);
        assert_eq!(tool_parts(&alone[0]), ("seen".to_string(), 1));
    }

    #[test]
    fn a_model_that_cannot_read_tool_images_gets_upstreams_sentence_in_their_place() {
        use crate::gateway::protocol::{CompletionRequest, Message, ToolChoice};
        let request = CompletionRequest {
            model: "m".to_string(),
            messages: vec![
                Message::user("read it"),
                tool_message(vec![tool_image(fixture::SHOT_PNG.to_vec())]),
            ],
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
        };
        let stripped = strip_tool_images(&request, TOOL_IMAGES_NOT_NATIVE).expect("stripped");
        assert_eq!(
            tool_parts(&stripped.messages[1]),
            (format!("{TOOL_IMAGES_NOT_NATIVE}seen"), 0)
        );
        assert_eq!(
            tool_parts(&request.messages[1]).1,
            1,
            "the history is left alone"
        );
        let plain = CompletionRequest {
            messages: vec![Message::user("hi")],
            ..request
        };
        assert!(strip_tool_images(&plain, TOOL_IMAGES_UNKNOWN).is_none());
    }

    #[test]
    fn a_placeholder_is_the_prefix_digits_and_a_bracket_and_nothing_else() {
        let ids = |text: &str| placeholder_ids(text).collect::<Vec<_>>();
        assert_eq!(ids("[Image #1] and [Image #23]"), [1, 23]);
        assert_eq!(
            ids("[Image 1] [Image #] [Image #x] [Image #2"),
            Vec::<u32>::new()
        );
        assert_eq!(ids("[Image #[Image #4]"), [4]);
        assert_eq!(ids("[Image #99999999999] [Image #5]"), [5]);
    }

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

    /// [`super::load`] under an empty read table: every test of where a path
    /// leads and what is read there. The table's own are
    /// [`a_path_the_read_table_denies_is_refused_before_it_is_opened`] and
    /// the one after it.
    fn load(
        workspace: &Path,
        home: Option<&Path>,
        input: &str,
    ) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
        super::load(workspace, home, &RuleSet::default(), input)
    }

    /// A table of `read`-category rules, or of `category` rules: each pattern
    /// with its action, in order.
    fn table(category: &str, patterns: &[(&str, RuleAction)]) -> RuleSet {
        RuleSet::new(
            patterns
                .iter()
                .map(|(pattern, action)| {
                    crate::permission::ConfiguredRule::new(category, *pattern, *action)
                })
                .collect(),
        )
    }

    #[test]
    fn a_path_the_read_table_denies_is_refused_before_it_is_opened() {
        // Judged where it resolves, by the path a rule names it by: relative
        // inside the workspace -- a `..` that comes back in included --
        // absolute outside it, `~` under the home directory.
        let tree = tree();
        let secret = tree.outside.join("secret.png");
        let home = tree.home.join("home.png");
        let rules = table(
            "read",
            &[
                ("shot.png", RuleAction::Deny),
                (secret.to_str().expect("utf-8"), RuleAction::Deny),
                (home.to_str().expect("utf-8"), RuleAction::Deny),
            ],
        );
        let load = |input: &str| super::load(&tree.workspace, Some(&tree.home), &rules, input);
        let denied = |shown: &str| Err(AttachError::Denied(shown.to_string()));
        assert_eq!(load("shot.png"), denied("shot.png"));
        assert_eq!(load("sub/../shot.png"), denied("shot.png"));
        assert_eq!(
            load("../outside/secret.png"),
            denied(secret.to_str().expect("utf-8"))
        );
        assert_eq!(load("~/home.png"), denied(home.to_str().expect("utf-8")));
        assert_eq!(
            load_literal(&tree.workspace, Some(&tree.home), &rules, "shot.png"),
            denied("shot.png")
        );
        // A file the table does not name still loads beside them.
        assert!(load("sub/deep.png").is_ok());
        // Asked before the file is opened: one nobody may read is the rule's
        // refusal, not the open's `AccessDenied`.
        let locked = tree.workspace.join("locked.png");
        std::fs::write(&locked, SHOT_PNG).expect("locked");
        std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000))
            .expect("chmod");
        assert_eq!(
            super::load(
                &tree.workspace,
                None,
                &table("read", &[("locked.png", RuleAction::Deny)]),
                "locked.png"
            ),
            denied("locked.png")
        );
        assert_eq!(
            AttachError::Denied("shot.png".to_string()).to_string(),
            "failed to attach image: a configured permission rule denies reading `shot.png`"
        );
    }

    #[test]
    fn a_link_to_a_file_the_table_denies_is_judged_at_the_file_it_reaches() {
        // A link at a path the table allows, to a file it denies, is the file
        // it reaches: the link itself, and a linked directory on the way.
        let tree = tree();
        std::fs::create_dir(tree.workspace.join("private")).expect("private");
        std::fs::write(tree.workspace.join("private").join("secret.png"), SHOT_PNG)
            .expect("secret");
        std::os::unix::fs::symlink(
            tree.workspace.join("private").join("secret.png"),
            tree.workspace.join("link.png"),
        )
        .expect("a link");
        std::os::unix::fs::symlink(tree.workspace.join("private"), tree.workspace.join("via"))
            .expect("a linked directory");
        let rules = table("read", &[("private/*", RuleAction::Deny)]);
        let denied = Err(AttachError::Denied("private/secret.png".to_string()));
        for input in ["link.png", "via/secret.png"] {
            assert_eq!(
                super::load(&tree.workspace, None, &rules, input),
                denied,
                "{input}"
            );
        }
        // Named by the file it reaches, never by the link's own spelling: a
        // deny on `shot.png` refuses `in.png -> shot.png` as `shot.png`.
        std::os::unix::fs::symlink(
            tree.workspace.join("shot.png"),
            tree.workspace.join("in.png"),
        )
        .expect("a link");
        assert_eq!(
            super::load(
                &tree.workspace,
                None,
                &table("read", &[("shot.png", RuleAction::Deny)]),
                "in.png"
            ),
            Err(AttachError::Denied("shot.png".to_string()))
        );
    }

    /// `name` in the workspace replaced, in place, by a link to `target`.
    fn swapped_for_link(name: &Path, target: &Path) {
        let aside = name.with_extension("aside");
        std::fs::rename(name, &aside).expect("moved aside");
        std::os::unix::fs::symlink(target, name).expect("a link");
    }

    #[test]
    fn a_link_swapped_in_after_the_judgment_is_never_followed() {
        // The table judged `shot.png`, which it allows; by the open it is a
        // link to a file the table denies. The real path is a file's, so a
        // link there is refused rather than followed -- by the `lstat`, and by
        // the open itself for a link put there after that.
        let tree = tree();
        std::fs::create_dir(tree.workspace.join("private")).expect("private");
        let secret = tree.workspace.join("private").join("secret.png");
        std::fs::write(&secret, SHOT_PNG).expect("secret");
        let rules = table("read", &[("private/*", RuleAction::Deny)]);
        let outcome = load_judged(&tree.workspace, None, &rules, "shot.png", |path| {
            swapped_for_link(path, &secret)
        });
        assert_eq!(
            outcome.map(|(path, ..)| path),
            Err(AttachError::Other("NotRegularFile")),
            "a link swapped in between the judgment and the open was read"
        );
        assert_eq!(
            open_regular(&tree.workspace.join("shot.png")).map(|_| ()),
            Err(AttachError::Other("NotRegularFile")),
            "the open followed a link"
        );
    }

    #[test]
    fn a_directory_swapped_on_the_way_is_judged_at_what_was_opened() {
        // An ancestor of the judged path replaced by a link to a directory
        // holding a file of the same name the table denies.
        let tree = tree();
        std::fs::create_dir(tree.workspace.join("pics")).expect("pics");
        std::fs::write(tree.workspace.join("pics").join("a.png"), SHOT_PNG).expect("a");
        std::fs::create_dir(tree.workspace.join("private")).expect("private");
        std::fs::write(tree.workspace.join("private").join("a.png"), SHOT_PNG).expect("secret");
        let rules = table("read", &[("private/*", RuleAction::Deny)]);
        let private = tree.workspace.join("private");
        let outcome = load_judged(&tree.workspace, None, &rules, "pics/a.png", |_| {
            swapped_for_link(&tree.workspace.join("pics"), &private)
        });
        assert_eq!(
            outcome.map(|(path, ..)| path),
            Err(AttachError::Denied("private/a.png".to_string())),
            "a directory swapped in between the judgment and the open was read through"
        );
    }

    #[test]
    fn a_directory_swapped_for_one_the_table_allows_is_read_through() {
        // The second question refuses what the table denies, not every change.
        let tree = tree();
        std::fs::create_dir(tree.workspace.join("pics")).expect("pics");
        std::fs::write(tree.workspace.join("pics").join("a.png"), SHOT_PNG).expect("a");
        std::fs::create_dir(tree.workspace.join("other")).expect("other");
        std::fs::write(tree.workspace.join("other").join("a.png"), SHOT_PNG).expect("a");
        let rules = table("read", &[("private/*", RuleAction::Deny)]);
        let other = tree.workspace.join("other");
        let outcome = load_judged(&tree.workspace, None, &rules, "pics/a.png", |_| {
            swapped_for_link(&tree.workspace.join("pics"), &other)
        });
        assert_eq!(outcome.map(|(.., bytes)| bytes), Ok(SHOT_PNG.to_vec()));
    }

    #[test]
    fn the_table_is_asked_as_read_file_would_be_and_only_a_deny_refuses() {
        let tree = tree();
        let load = |rules: &RuleSet| {
            super::load(&tree.workspace, None, rules, "shot.png").map(|(path, ..)| path)
        };
        let shot = Ok(tree.workspace.join("shot.png"));
        // The tool's own name decides as its category does; another tool's
        // category does not.
        assert!(matches!(
            load(&table("read_file", &[("shot.png", RuleAction::Deny)])),
            Err(AttachError::Denied(_))
        ));
        assert!(matches!(
            load(&table("*", &[("*", RuleAction::Deny)])),
            Err(AttachError::Denied(_))
        ));
        assert_eq!(load(&table("edit", &[("*", RuleAction::Deny)])), shot);
        // An ask is no refusal: the person attaching is who it would ask.
        assert_eq!(load(&table("read", &[("*", RuleAction::Ask)])), shot);
        // The last rule that matches decides, as for every call.
        assert_eq!(
            load(&table(
                "read",
                &[("*", RuleAction::Deny), ("shot.png", RuleAction::Allow)]
            )),
            shot
        );
        // A path is painted as the filesystem spells it, minus what a
        // terminal would obey.
        let odd = "odd\u{1b}[2J.png";
        std::fs::write(tree.workspace.join(odd), SHOT_PNG).expect("odd");
        assert_eq!(
            super::load(
                &tree.workspace,
                None,
                &table("read", &[(odd, RuleAction::Deny)]),
                odd
            ),
            Err(AttachError::Denied("odd\\u{001B}[2J.png".to_string()))
        );
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

    /// `open` of the FIFO at `fifo`, on another thread, with ten seconds to
    /// answer in: `None` when it did not, after the FIFO has been given a
    /// writer so the open that was waiting for one can return and its thread
    /// end.
    #[cfg(unix)]
    fn answered<T: Send + 'static>(
        fifo: &Path,
        open: impl FnOnce(&Path) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        let target = fifo.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(open(&target));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(outcome) => Some(outcome),
            Err(_) => {
                drop(std::fs::OpenOptions::new().write(true).open(fifo));
                None
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_named_like_an_image_is_refused_without_waiting_for_a_writer() {
        // A pasted `x.png` that is a FIFO: an open that waits for a writer
        // would hold the thread that holds the terminal for as long as none
        // comes. `mkfifo(1)`, as `tests/tool_loop.rs` makes one.
        let tree = tree();
        let fifo = tree.workspace.join("x.png");
        let status = crate::process::status(std::process::Command::new("mkfifo").arg(&fifo))
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo failed: {status:?}");
        let outcome =
            answered(&fifo, read).expect("reading a FIFO named x.png waited for a writer");
        assert_eq!(outcome, Err(AttachError::Other("NotRegularFile")));
        // And as a path is attached, with no table and with one.
        let workspace = tree.workspace.clone();
        let loaded = answered(&fifo, move |_| {
            [
                RuleSet::default(),
                table("read", &[("private/*", RuleAction::Deny)]),
            ]
            .map(|rules| super::load(&workspace, None, &rules, "x.png").map(|_| ()))
        })
        .expect("attaching a FIFO named x.png waited for a writer");
        assert_eq!(
            loaded,
            [const { Err(AttachError::Other("NotRegularFile")) }; 2]
        );
        // The open itself, for a FIFO put in a file's place after that `stat`:
        // it does not wait, and the descriptor's own `fstat` refuses it.
        let opened = answered(&fifo, |path| open_regular(path).map(|_| ()))
            .expect("opening a FIFO waited for a writer");
        assert_eq!(opened, Err(AttachError::Other("NotRegularFile")));
        // A socket cannot be opened at all, and is no file either.
        let socket = tree.workspace.join("s.png");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("a socket");
        assert_eq!(
            open_regular(&socket).map(|_| ()),
            Err(AttachError::Other("NotRegularFile"))
        );
        assert_eq!(read(&socket), Err(AttachError::Other("NotRegularFile")));
        // A regular file opened this way reads as one: blocking again.
        let file = open_regular(&tree.workspace.join("shot.png")).expect("a file");
        let flags = rustix::fs::fcntl_getfl(&file).expect("flags");
        assert!(!flags.contains(rustix::fs::OFlags::NONBLOCK));
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
