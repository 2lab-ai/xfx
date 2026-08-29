//! Proofs, plans, and the one-use authorities that let one of them run.
//!
//! Every mutating tool call passes through the same five stages, and each stage
//! can only refuse or hand a *more specific* value to the next one:
//!
//! ```text
//! decode/validate -> prepare -> policy -> mint -> revalidate -> execute
//! ```
//!
//! **Prepare** is where an ambiguous request ("edit `notes.md`") becomes an
//! unambiguous fact: one canonical target, the identity and digest of exactly
//! the bytes that are there now, and exactly the bytes that would replace them.
//! Preparation writes nothing. That is the load-bearing property behind
//! "a decision cannot mutate its own target": by the time policy runs, the plan
//! is a value, and by the time execution runs, the plan is the only thing that
//! can be executed.
//!
//! **Mint** turns an allowed plan into an [`ExecutionAuthority`] carrying a
//! [`Nonce`] the session has never issued before. **Execution** consumes the
//! nonce first and revalidates second, so any outcome -- success, a stale
//! preimage, an I/O failure -- burns the authority. A retry has to go back
//! through policy rather than reusing an answer the user gave about a world that
//! has since changed
//! (`vercel-labs/fx@580a0c5d src/core/permissions/command_admission.zig:18-97`).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::tools::ToolLimits;
use crate::workspace::AccessScope;

use super::command::{classify, CommandEffect, DeniedEffect};

/// The most bytes a prepared command may contain
/// (`vercel-labs/fx@580a0c5d src/core/terminal/contracts.zig:8` is 64 KiB; xfx
/// uses upstream's *planning* bound instead, `command_effect.zig:5`).
const MAX_COMMAND_BYTES: usize = 8 * 1024;

// ---------------------------------------------------------------------------
// digests and identities
// ---------------------------------------------------------------------------

/// A SHA-256 digest of some bytes.
///
/// Content identity is what makes a preimage a proof. `st_mtime` is coarse on
/// some filesystems and forgeable on all of them, so it is corroborating
/// evidence rather than the check
/// (`vercel-labs/fx@580a0c5d src/core/workspace/read_tracker.zig:63-67`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hasher.finalize().into())
    }

    /// Lowercase hex, for a message a human reads.
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// The first 12 hex characters: enough to tell two files apart in a
    /// sentence, short enough to read.
    pub fn short(&self) -> String {
        self.hex()[..12].to_string()
    }

    fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.short())
    }
}

/// Who a filesystem object is, as opposed to what it is called.
///
/// A name can be re-pointed at a different object between two syscalls; a
/// device/inode pair cannot. Size and modification time are carried too, so a
/// mismatch can say *which* fact changed rather than only that something did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
    /// The permission bits, preserved across a replacement.
    pub mode: u32,
    pub size: u64,
    pub mtime_ns: i128,
}

impl FileIdentity {
    /// Reads the identity of whatever `metadata` describes.
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                device: metadata.dev(),
                inode: metadata.ino(),
                mode: metadata.mode() & 0o7777,
                size: metadata.size(),
                mtime_ns: i128::from(metadata.mtime()) * 1_000_000_000
                    + i128::from(metadata.mtime_nsec()),
            }
        }
        #[cfg(not(unix))]
        {
            // Without `st_dev`/`st_ino` the identity degrades to size and
            // modification time. That is weaker, and the platform note in
            // `mutate.rs` says so rather than implying parity.
            let mtime_ns = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|delta| delta.as_nanos() as i128)
                .unwrap_or(0);
            Self {
                device: 0,
                inode: 0,
                mode: 0o644,
                size: metadata.len(),
                mtime_ns,
            }
        }
    }

    /// Whether two identities name the same object with the same contents-shape.
    ///
    /// Deliberately strict: a same-inode file whose size or mtime moved is a
    /// file someone else wrote to, and the caller's preimage no longer describes
    /// it.
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }

    fn absorb(&self, hasher: &mut Sha256) {
        hasher.update(self.device.to_le_bytes());
        hasher.update(self.inode.to_le_bytes());
        hasher.update(self.mode.to_le_bytes());
        hasher.update(self.size.to_le_bytes());
        hasher.update(self.mtime_ns.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// read proofs
// ---------------------------------------------------------------------------

/// What one successful `read_file` established about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadRecord {
    pub identity: FileIdentity,
    /// The digest of the whole file on disk at read time.
    pub hash: ContentHash,
    /// Whether the *model* saw the whole file, as opposed to xfx having hashed
    /// it. A windowed or clipped read hashes the whole file but shows part of
    /// it, and only the second fact can authorize a rewrite.
    pub complete_view: bool,
}

/// Which files this session has read, and what they looked like.
///
/// Session-scoped and in memory. Nothing here is persisted: a proof is about
/// this process's view of the filesystem, and a proof reloaded from disk would
/// be a claim rather than an observation
/// (`vercel-labs/fx@580a0c5d src/core/workspace/read_tracker.zig:24-60`).
#[derive(Debug, Default)]
pub struct ReadTracker {
    entries: HashMap<PathBuf, ReadRecord>,
}

impl ReadTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the latest read of `path`, replacing any earlier one.
    pub fn record(&mut self, path: impl Into<PathBuf>, record: ReadRecord) {
        self.entries.insert(path.into(), record);
    }

    /// What this session knows about `path`, if anything.
    pub fn proof(&self, path: &Path) -> Option<&ReadRecord> {
        self.entries.get(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// nonces
// ---------------------------------------------------------------------------

/// A value issued at most once in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Nonce(u64);

impl Nonce {
    /// The next unused value.
    ///
    /// Process-wide rather than session-wide so that an authority minted by one
    /// session can never be mistaken for a different session's authority with
    /// the same number; presenting it elsewhere is [`AuthorityError::Unknown`],
    /// not an accidental match.
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Why an authority could not be spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    /// This session never issued it.
    Unknown,
    /// It has already been spent, successfully or not.
    Consumed,
}

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "this session did not issue that authority"),
            Self::Consumed => write!(f, "that authority has already been used once"),
        }
    }
}

impl std::error::Error for AuthorityError {}

/// Which authorities a session has issued, and which are already spent.
#[derive(Debug, Default)]
pub struct AuthorityLedger {
    issued: HashSet<Nonce>,
    consumed: HashSet<Nonce>,
}

impl AuthorityLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a freshly minted authority and returns its nonce.
    fn issue(&mut self) -> Nonce {
        let nonce = Nonce::next();
        self.issued.insert(nonce);
        nonce
    }

    /// Spends `nonce`, or says why it cannot be spent.
    pub fn consume(&mut self, nonce: Nonce) -> Result<(), AuthorityError> {
        if !self.issued.contains(&nonce) {
            return Err(AuthorityError::Unknown);
        }
        if !self.consumed.insert(nonce) {
            return Err(AuthorityError::Consumed);
        }
        Ok(())
    }

    pub fn issued_count(&self) -> usize {
        self.issued.len()
    }

    pub fn consumed_count(&self) -> usize {
        self.consumed.len()
    }
}

// ---------------------------------------------------------------------------
// mutation plans
// ---------------------------------------------------------------------------

/// Which mutating tool produced a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    Write,
    Edit,
    CreateFolder,
}

impl MutationKind {
    /// The advertised tool name, which is also the name a rule or grant uses.
    pub fn tool(self) -> &'static str {
        match self {
            Self::Write => "write_file",
            Self::Edit => "edit_file",
            Self::CreateFolder => "create_folder",
        }
    }
}

/// Which authorized root a target sits under.
///
/// `--add-dir` grants reading. Writing into an added root is a separate
/// decision, and `auto` does not make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetScope {
    PrimaryWorkspace,
    AdditionalRoot,
}

/// What was at the target when the plan was prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preimage {
    /// Nothing was there, and nothing may be there at execution time either.
    Absent,
    Present {
        identity: FileIdentity,
        hash: ContentHash,
    },
}

impl Preimage {
    fn absorb(&self, hasher: &mut Sha256) {
        match self {
            Self::Absent => hasher.update([0u8]),
            Self::Present { identity, hash } => {
                hasher.update([1u8]);
                identity.absorb(hasher);
                hasher.update(hash.bytes());
            }
        }
    }
}

/// The largest excerpt or preview a prompt will show of one side of a change.
///
/// Long enough that a human can recognize what is being replaced, short enough
/// that a hostile model cannot use the approval prompt as a place to print a
/// screenful of text designed to be scrolled past.
pub const MAX_EXCERPT_BYTES: usize = 160;

/// What a change replaces, and with what, rendered for a human.
///
/// Only `edit_file` produces one: `write_file` replaces everything, so its
/// "before" is the whole previous file and the digest is the honest summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationExcerpt {
    pub before: String,
    pub after: String,
}

/// The largest one side of a screen-reviewed change may be, **after escaping**.
///
/// A question therefore carries at most twice this, and the number is a literal
/// rather than a function of the terminal: what it bounds is a payload on a
/// channel, and the channel is sized before anybody knows how tall the screen
/// is. Large enough that a real edit arrives whole -- upstream's own review
/// surface shows a file, not a sentence -- and small enough that a hostile model
/// cannot make one approval cost megabytes of queue.
pub const MAX_APPROVAL_DIFF_SIDE_BYTES: usize = 64 * 1024;

/// The mark a bounded rendering ends with when it had to stop early.
///
/// A rendering that stopped mid-word without saying so would read as the whole
/// of what xfx was about to do.
const CLIPPED: char = '\u{2026}';

/// A character named by its code point, the way source writes one: `\u{001B}`.
///
/// **The representation of a character that may not reach a terminal**, and it
/// is shared rather than invented per surface: the permission boundary spends
/// it on a control (`bounded_diff_side`, `bounded_excerpt`) and the review
/// screen spends it on a control *or* a bidirectional override
/// (`crate::tui::approval_screen`), and a reader who has learned to read one
/// has learned to read the other.
///
/// Three properties, and each is why it is this spelling rather than a symbol:
///
/// 1. **Injective.** One token per code point, so two characters a payload
///    could swap are never one string. A single replacement character for all
///    of them says "something was here" and hides *what*, which is the whole
///    question a review is asked.
/// 2. **Terminal-safe.** Every byte of it is printable ASCII, so what stands in
///    for a character a terminal would obey is itself nothing a terminal obeys.
/// 3. **Unambiguous next to the payload.** It begins with a backslash, and the
///    renderings that spend it escape a payload's own backslash, so text that
///    merely *spells* a code point is never read as one.
///
/// Four hex digits, upper case, because every scalar `char::is_control` admits
/// is at most `U+009F` -- and the width is fixed by the format rather than by
/// that fact, so a wider one would still be whole.
pub fn scalar_token(character: char) -> String {
    format!("\\u{{{:04X}}}", character as u32)
}

/// What one character of foreign text becomes when a prompt quotes it.
///
/// One answer for both bounded renderings, so the band's summary and the
/// screen's diff cannot escape by two different sets of rules. And it is a
/// *value* rather than a `push` because the cost has to be knowable **before**
/// it is spent: that is what lets [`bounded_diff_side`] cut between tokens
/// instead of at a byte index.
enum Escaped {
    /// A control character with a name, written the way source writes it.
    Token(&'static str),
    /// A control character with no name, written as its **code point**
    /// ([`scalar_token`]).
    Scalar(char),
    /// Everything else: the character itself.
    Char(char),
}

/// What a real line break becomes in a bounded rendering.
///
/// The one thing the two renderings disagree about, and they disagree because
/// their readers do. A rendering quoted **inside a sentence** cannot contain a
/// line break: the band's panel and the line shell's prompt both place it in
/// the middle of a row, and one arriving there would move the cursor in the
/// middle of a layout measured in cells. A rendering read by a surface that
/// **places rows** must keep it: a break is where a line ends, and a review
/// screen that lost them would show a file as one long run
/// (`super::super::tui::approval_screen`'s `safe_rows`, which splits on real
/// breaks before it escapes anything else and so never lets one reach a row).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Breaks {
    /// Named on the line, as `\n`.
    Named,
    /// Kept, as the break it is.
    Kept,
}

impl Escaped {
    fn of(character: char, breaks: Breaks) -> Self {
        match character {
            // **First, and it is what makes the rendering injective.** Every
            // other arm below spends a backslash, so a payload that already
            // contains one has to spend two -- otherwise a file whose lines
            // really end and a file that merely contains the two characters `\`
            // and `n` render as the same string, and a reviewer is shown a
            // change of every line in a file as a change of nothing.
            '\\' => Self::Token("\\\\"),
            '\n' if breaks == Breaks::Kept => Self::Char('\n'),
            '\n' => Self::Token("\\n"),
            // Named on both, because a CR is a cursor move rather than a line:
            // a reader is owed the fact that the file carries one, and a
            // surface that split on it would turn a CRLF into a blank row.
            '\r' => Self::Token("\\r"),
            '\t' => Self::Token("\\t"),
            // Everything else the terminal would act on rather than draw: `ESC`
            // and `BEL`, and the C1 range where a single scalar *is* a `CSI` or
            // an `OSC` on a terminal that decodes it.
            //
            // **Named by its code point, not blanked to one replacement
            // character.** One symbol for all sixty-two of them is the same
            // defect the escaped backslash above exists to prevent, one level
            // down: a file of `ESC` and a file of `BEL` are two different
            // files, and a review that showed them as one string would let a
            // model swap one for the other and have the screen call it a no-op.
            // A reader is owed *which* byte was there, not only that one was.
            other if other.is_control() => Self::Scalar(other),
            other => Self::Char(other),
        }
    }

    /// How many bytes this costs, asked before it is spent.
    ///
    /// Derived rather than remembered, including for [`Self::Scalar`], whose
    /// token is built to be measured: the bound is a promise about the bytes
    /// that leave, and a cost guessed here would be a bound measured against a
    /// number nothing produced.
    fn len(&self) -> usize {
        match self {
            Self::Token(token) => token.len(),
            Self::Scalar(character) => scalar_token(*character).len(),
            Self::Char(character) => character.len_utf8(),
        }
    }

    fn push_onto(&self, out: &mut String) {
        match self {
            Self::Token(token) => out.push_str(token),
            Self::Scalar(character) => out.push_str(&scalar_token(*character)),
            Self::Char(character) => out.push(*character),
        }
    }
}

/// Renders `text` on one line, bounded, with the clipping made visible.
///
/// Newlines and other control characters are escaped rather than printed: an
/// approval prompt that a payload can reflow is an approval prompt a payload can
/// disguise. **One line** is the whole difference from [`bounded_diff_side`]:
/// this is quoted inside a sentence, by the band's panel and by the line
/// shell's prompt, and a break arriving in the middle of a row measured in
/// cells would move the cursor there.
///
/// It is injective for the same reason that one is: the backslash it spends on
/// a name is itself escaped, so a payload that already contains `\n` as two
/// characters is not quoted as though its lines ended.
pub fn bounded_excerpt(text: &str) -> String {
    let mut out = String::new();
    let mut clipped = false;
    for character in text.chars() {
        if out.len() >= MAX_EXCERPT_BYTES {
            clipped = true;
            break;
        }
        Escaped::of(character, Breaks::Named).push_onto(&mut out);
    }
    if clipped {
        out.push(CLIPPED);
    }
    out
}

/// One side of a change, escaped **and then** cut to
/// [`MAX_APPROVAL_DIFF_SIDE_BYTES`], with its line structure kept.
///
/// The order is the guarantee. Cutting the raw text first and escaping the
/// remainder would measure a bound against bytes that do not exist yet: a
/// payload of control characters costs **eight bytes each** on the way out
/// ([`scalar_token`]), so 64 KiB of input would leave here as half a megabyte
/// on a channel sized for 64 KiB.
///
/// # A line break is structure, and it is kept
///
/// This side is read by a surface that **places rows**
/// (`crate::tui::approval_screen`), and on that surface a line break is where a
/// line ends. Named as `\n` it would be a file shown as one long wrapped run,
/// which is the shape a reviewer cannot read a change out of; kept, the review
/// reads as the file does. It is safe to keep for the same reason it is worth
/// keeping: that surface splits on real breaks **before** it paints anything,
/// so one never reaches a row it did not end -- and the channel between here
/// and there preserves it deliberately (`crate::tui::bridge`'s `inert`, which
/// exempts `\n` and `\r` and nothing else). Every other control is still named
/// or replaced, so nothing here can command a terminal.
///
/// # And the rendering is injective
///
/// Two rules make it so, and each was a collision before it was a rule.
///
/// Every arm that names a character spends a **backslash**, so a payload that
/// already contains one has to spend two ([`Escaped::of`]). Without that, a file
/// whose lines really end and a file that merely contains the two characters
/// `\` and `n` render as the same string -- and the screen that exists to show
/// a change would show a hundred-line file being replaced by one line of
/// literal escapes as a change of nothing at all.
///
/// And a control with no name is written as its **code point** rather than as
/// one symbol standing in for all of them ([`scalar_token`]). Without that, a
/// file of `ESC` and a file of `BEL` -- or the two C1 scalars that *are* a
/// `CSI` and an `OSC` on a terminal that decodes eight-bit controls -- render
/// as the same string, and the same no-op is shown for a change that swapped
/// every byte in the file. Sixty-five scalars are in that domain and the test
/// walks all of them rather than the pairs somebody thought of.
///
/// Three properties follow from cutting between [`Escaped`] tokens rather than
/// at a byte index, and all three are user-visible:
///
/// 1. The cut is on a character boundary, so a four-byte scalar cannot be halved
///    -- which a `String` cannot even represent.
/// 2. It is on a *token* boundary, so `\t` -- or the escaped backslash itself --
///    cannot be halved either. Half of one is a lone backslash, which reads as
///    an escape of whatever follows it.
/// 3. The mark is paid for **inside** the bound rather than appended after it,
///    which is why the room for it is remembered as the loop goes: only the loop
///    knows where the boundaries were.
pub fn bounded_diff_side(text: &str) -> String {
    let mut out = String::new();
    // The longest prefix, at a token boundary, that still leaves room for the
    // mark.
    let mut with_room_to_mark = 0usize;
    for character in text.chars() {
        let escaped = Escaped::of(character, Breaks::Kept);
        if out.len() + escaped.len() > MAX_APPROVAL_DIFF_SIDE_BYTES {
            out.truncate(with_room_to_mark);
            out.push(CLIPPED);
            return out;
        }
        escaped.push_onto(&mut out);
        if out.len() + CLIPPED.len_utf8() <= MAX_APPROVAL_DIFF_SIDE_BYTES {
            with_room_to_mark = out.len();
        }
    }
    out
}

/// What a prompt says about bytes it cannot show as text.
///
/// **A fact, not a rendering.** Bytes that are not UTF-8 have no text form, and
/// the two ways of pretending otherwise are both worse than saying so: dropping
/// them shows a file shorter than it is, and replacing each of them shows a
/// file whose contents are question marks. Either would put a reviewer in front
/// of a "before" that is not what is there.
///
/// One sentence for every surface that needs it -- the band's one-line preview
/// of the bytes a write would put down ([`MutationPlan::preview`]) and the
/// review screen's "before" for the file it would replace -- so a reader who
/// has learned what it means has learned it once.
pub fn non_text_summary(len: usize) -> String {
    format!("<{len} bytes of non-UTF-8 data>")
}

/// What a change replaces and with what, bounded for review on a screen.
///
/// Beside [`MutationExcerpt`] rather than in place of it, because they answer
/// two different questions. The excerpt is a sentence's worth of each side, and
/// it is what the band's one-line summary quotes; this is the whole of each
/// side up to [`MAX_APPROVAL_DIFF_SIDE_BYTES`], and it exists because the risk
/// of an edit lives in bytes the sentence had to leave out.
///
/// Both fields are already escaped and already bounded: the payload is made
/// inert at the permission boundary, where the change is known, rather than at
/// whichever surface happens to render it. A second surface added later
/// inherits the property by using this type.
///
/// **Inert is not the same as flat.** Real line breaks survive in both fields,
/// because they are the change's own structure and the surface that renders
/// this places rows; every other control is named or replaced, and the
/// rendering is injective, so two payloads that differ never arrive here as one
/// ([`bounded_diff_side`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalDiff {
    pub before: String,
    pub after: String,
}

impl ApprovalDiff {
    /// The two sides, each escaped and bounded.
    pub fn of(before: &str, after: &str) -> Self {
        Self {
            before: bounded_diff_side(before),
            after: bounded_diff_side(after),
        }
    }

    /// Whether this change is bigger than the band's own summary can show.
    ///
    /// The comparison is against [`MAX_EXCERPT_BYTES`] because that is exactly
    /// how much of each side the summary quotes: a change whose sides both fit
    /// in it has already been shown whole, and a second surface would repeat
    /// the band to hide the document behind it.
    ///
    /// Deliberately **not** a function of the terminal's height. A rule keyed on
    /// rows would review a one-word edit on a full screen the moment somebody
    /// made their window short, and leave a hundred-kilobyte replacement in two
    /// rows of summary on a tall one.
    pub fn wants_screen(&self) -> bool {
        self.before.len() > MAX_EXCERPT_BYTES || self.after.len() > MAX_EXCERPT_BYTES
    }
}

/// One exact filesystem change, decided and not yet made.
///
/// Every field is fixed at preparation time and none is public: a caller cannot
/// retarget a plan after policy has judged it, because there is no way to write
/// to one.
#[derive(Debug)]
pub struct MutationPlan {
    kind: MutationKind,
    target: PathBuf,
    display: String,
    scope: TargetScope,
    preimage: Preimage,
    /// The exact bytes that will replace the preimage. Empty for a directory.
    after: Vec<u8>,
    /// What an approval prompt shows. Derived from data the fingerprint already
    /// covers, so it is deliberately not part of the fingerprint itself.
    excerpt: Option<MutationExcerpt>,
    /// The same, for a review that has a screen rather than two rows. Held
    /// beside the excerpt rather than replacing it: the band still asks its
    /// one-line question whichever surface reviews the change.
    diff: Option<ApprovalDiff>,
    fingerprint: ContentHash,
}

impl MutationPlan {
    /// Builds a plan. Only the mutation executors call this, and only after the
    /// target has been resolved and its preimage read.
    pub fn new(
        kind: MutationKind,
        target: PathBuf,
        display: String,
        scope: TargetScope,
        preimage: Preimage,
        after: Vec<u8>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update([match kind {
            MutationKind::Write => 1u8,
            MutationKind::Edit => 2,
            MutationKind::CreateFolder => 3,
        }]);
        hasher.update(target.as_os_str().as_encoded_bytes());
        hasher.update([match scope {
            TargetScope::PrimaryWorkspace => 1u8,
            TargetScope::AdditionalRoot => 2,
        }]);
        preimage.absorb(&mut hasher);
        hasher.update((after.len() as u64).to_le_bytes());
        hasher.update(&after);
        let fingerprint = ContentHash(hasher.finalize().into());
        Self {
            kind,
            target,
            display,
            scope,
            preimage,
            after,
            excerpt: None,
            diff: None,
            fingerprint,
        }
    }

    /// The same plan, carrying the before/after a prompt will show.
    pub fn with_excerpt(mut self, excerpt: MutationExcerpt) -> Self {
        self.excerpt = Some(excerpt);
        self
    }

    /// The same plan, carrying the bounded diff a screen review would show.
    ///
    /// Not part of the fingerprint, for the reason the excerpt is not: it is a
    /// *rendering* of bytes the fingerprint already covers, and a plan whose
    /// identity moved when somebody changed how it is displayed would be a plan
    /// an authority could no longer be matched to.
    pub fn with_diff(mut self, diff: ApprovalDiff) -> Self {
        self.diff = Some(diff);
        self
    }

    /// What this change replaces and with what, when the tool could say.
    pub fn excerpt(&self) -> Option<&MutationExcerpt> {
        self.excerpt.as_ref()
    }

    /// The whole of both sides, bounded, when the tool could say.
    ///
    /// The two content mutations both can: an edit's pair is the strings the
    /// model sent, and a write's is the file it is replacing and the text
    /// replacing it -- which the executor has read, and holds a complete read
    /// proof for, before it plans anything. `create_folder` changes no content
    /// at all and is the one mutation with no pair a diff surface could show.
    pub fn diff(&self) -> Option<&ApprovalDiff> {
        self.diff.as_ref()
    }

    /// A bounded, escaped preview of the bytes that will be written.
    pub fn preview(&self) -> String {
        match std::str::from_utf8(&self.after) {
            Ok(text) => bounded_excerpt(text),
            Err(_) => non_text_summary(self.after.len()),
        }
    }

    /// The digest of the bytes that will be written.
    pub fn after_hash(&self) -> ContentHash {
        ContentHash::of(&self.after)
    }

    pub fn kind(&self) -> MutationKind {
        self.kind
    }

    /// The exact path that will change.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// How the target is named to the model and the user.
    pub fn display(&self) -> &str {
        &self.display
    }

    pub fn scope(&self) -> TargetScope {
        self.scope
    }

    pub fn preimage(&self) -> &Preimage {
        &self.preimage
    }

    /// The bytes staged for the target.
    pub fn staged_bytes(&self) -> &[u8] {
        &self.after
    }

    /// A digest over the whole plan: target, scope, preimage, and staged bytes.
    pub fn fingerprint(&self) -> ContentHash {
        self.fingerprint
    }

    /// Whether the plan would leave the file exactly as it found it.
    pub fn is_noop(&self) -> bool {
        match &self.preimage {
            Preimage::Absent => false,
            Preimage::Present { hash, .. } => *hash == ContentHash::of(&self.after),
        }
    }
}

/// A [`MutationPlan`] that has been authorized exactly once.
#[derive(Debug)]
pub struct PreparedMutation {
    plan: MutationPlan,
    nonce: Nonce,
    source: AllowSourceTag,
}

impl PreparedMutation {
    pub fn plan(&self) -> &MutationPlan {
        &self.plan
    }

    pub fn nonce(&self) -> Nonce {
        self.nonce
    }
}

// ---------------------------------------------------------------------------
// command plans
// ---------------------------------------------------------------------------

/// How an admitted command reaches the operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRoute {
    /// An exact argument vector, executed with no shell at all. Nothing in the
    /// command text can be re-interpreted, because nothing re-reads it.
    Direct { argv: Vec<String> },
    /// The platform shell, `-c`, and the exact command text. Only reachable
    /// after a human approval or an explicit rule.
    Shell { program: PathBuf },
}

/// One exact command, decided and not yet run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    command: String,
    cwd: PathBuf,
    display_cwd: String,
    environment: Vec<(String, String)>,
    effect: CommandEffect,
    route: CommandRoute,
    fingerprint: ContentHash,
}

impl CommandPlan {
    /// Resolves and classifies one command without running anything.
    ///
    /// `cwd` is resolved against the scope, so a command cannot run outside the
    /// roots the user authorized even before its own text is considered.
    pub fn prepare(
        command: &str,
        scope: &AccessScope,
        cwd: Option<&str>,
        limits: &ToolLimits,
    ) -> Result<Self, String> {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err("terminal exec requires a non-empty `command`".to_string());
        }
        if trimmed.len() > MAX_COMMAND_BYTES {
            return Err(format!(
                "terminal exec refuses a command of {} bytes; the bound is {MAX_COMMAND_BYTES}",
                trimmed.len()
            ));
        }

        let resolved_cwd = match cwd {
            None => scope.primary().to_path_buf(),
            Some(requested) => {
                let resolved = scope
                    .resolve_existing(requested)
                    .map_err(|err| format!("terminal exec cannot use that cwd: {err}"))?;
                if !resolved.absolute().is_dir() {
                    return Err(format!(
                        "terminal exec cwd `{requested}` is not a directory"
                    ));
                }
                resolved.absolute().to_path_buf()
            }
        };
        let display_cwd = scope.display_path(&resolved_cwd);

        // The text classifier already refused absolute operands and `..`
        // components. What it could not know is where a *relative* name
        // actually points: `notes.md` may be a symbolic link out of the
        // workspace, and a direct `cat` would follow it. That question needs
        // the scope, so it is answered here.
        let effect = match classify(trimmed) {
            CommandEffect::DirectReadOnly { argv } => {
                match escaping_operand(&argv, scope, &resolved_cwd) {
                    None => CommandEffect::DirectReadOnly { argv },
                    Some(_) => CommandEffect::Denied(DeniedEffect::OperandOutsideWorkspace),
                }
            }
            denied => denied,
        };
        let route = match &effect {
            CommandEffect::DirectReadOnly { argv } => CommandRoute::Direct {
                argv: hardened(argv),
            },
            CommandEffect::Denied(_) => CommandRoute::Shell {
                program: platform_shell(),
            },
        };

        let environment = minimal_environment();
        let mut hasher = Sha256::new();
        hasher.update(trimmed.as_bytes());
        hasher.update([0u8]);
        hasher.update(resolved_cwd.as_os_str().as_encoded_bytes());
        hasher.update([0u8]);
        for (name, value) in &environment {
            hasher.update(name.as_bytes());
            hasher.update(*b"=");
            hasher.update(value.as_bytes());
            hasher.update([0u8]);
        }
        match &route {
            CommandRoute::Direct { argv } => {
                hasher.update([1u8]);
                for word in argv {
                    hasher.update(word.as_bytes());
                    hasher.update([0u8]);
                }
            }
            CommandRoute::Shell { program } => {
                hasher.update([2u8]);
                hasher.update(program.as_os_str().as_encoded_bytes());
            }
        }
        let fingerprint = ContentHash(hasher.finalize().into());

        // `limits` bounds output and wall clock at execution time; nothing about
        // the plan depends on it, so it is read there rather than copied here.
        let _ = limits;

        Ok(Self {
            command: trimmed.to_string(),
            cwd: resolved_cwd,
            display_cwd,
            environment,
            effect,
            route,
            fingerprint,
        })
    }

    /// The command exactly as the model wrote it, trimmed.
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn display_cwd(&self) -> &str {
        &self.display_cwd
    }

    /// The complete environment the child will see. Nothing is inherited.
    pub fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    pub fn effect(&self) -> &CommandEffect {
        &self.effect
    }

    pub fn route(&self) -> &CommandRoute {
        &self.route
    }

    /// A digest over the command text, the working directory, the environment,
    /// and the route. Any change to any of them is a different plan.
    pub fn fingerprint(&self) -> ContentHash {
        self.fingerprint
    }
}

/// A [`CommandPlan`] that has been authorized exactly once.
#[derive(Debug)]
pub struct PreparedCommand {
    plan: CommandPlan,
    nonce: Nonce,
    source: AllowSourceTag,
}

impl PreparedCommand {
    pub fn plan(&self) -> &CommandPlan {
        &self.plan
    }

    pub fn nonce(&self) -> Nonce {
        self.nonce
    }
}

/// Overrides prepended to an admitted `git`, before its subcommand.
///
/// `core.fsmonitor` names a command git runs through a shell on `status` and on
/// anything else that refreshes the index, so a repository whose `.git/config`
/// already carries one would execute it on the route `auto` admits without
/// asking. `-c` outranks every configuration file, and `false` is the value that
/// disables both the built-in monitor and the hook form.
const GIT_CONFIG_OVERRIDES: &[&str] = &["-c", "core.fsmonitor=false"];

/// Options inserted directly after a `git` subcommand that accepts them.
///
/// `--no-ext-diff` disables `diff.external`; `--no-textconv` disables the
/// `diff.<driver>.textconv` a `.gitattributes` entry can select. Both are
/// commands run through a shell, and both are configured *inside the
/// repository*, which is exactly the kind of pre-existing state a read-only
/// route must not execute.
const GIT_DIFF_SAFETY: &[&str] = &["--no-ext-diff", "--no-textconv"];

/// The `git` subcommands that accept [`GIT_DIFF_SAFETY`].
const GIT_DIFF_SUBCOMMANDS: &[&str] = &["diff", "log", "show"];

/// The argv a direct plan really executes.
///
/// Only `git` is rewritten, and only by adding options that *remove* behaviour.
/// The command text the model wrote is untouched -- it is what a rule or a grant
/// is keyed on, and what the user is shown -- so this changes what runs, not
/// what was agreed.
///
/// # What this does not cover
///
/// A `filter.<driver>.clean` selected by a `.gitattributes` entry is also a
/// shell command, and `git diff` runs it on a worktree file. Git offers no
/// single switch for it, and enumerating drivers is not a boundary. The real
/// control for that is the other half of this change: the typed file tools
/// refuse to write `.git` at all, so xfx cannot be the thing that installs one.
/// A repository that already carries a hostile `.gitattributes` and a matching
/// `.git/config` is a repository whose contents the user has already chosen to
/// open.
fn hardened(argv: &[String]) -> Vec<String> {
    if argv.first().map(String::as_str) != Some("git") {
        return argv.to_vec();
    }
    let mut out: Vec<String> = Vec::with_capacity(argv.len() + 4);
    out.push(argv[0].clone());
    out.extend(GIT_CONFIG_OVERRIDES.iter().map(|word| word.to_string()));
    if let Some(subcommand) = argv.get(1) {
        out.push(subcommand.clone());
        if GIT_DIFF_SUBCOMMANDS.contains(&subcommand.as_str()) {
            out.extend(GIT_DIFF_SAFETY.iter().map(|word| word.to_string()));
        }
        out.extend(argv[2..].iter().cloned());
    }
    out
}

/// The first word of `argv` that exists and resolves outside the scope.
///
/// Only *existing* names are resolved. A word that names nothing -- a grep
/// pattern, a git revision, an argument to a test harness -- cannot be a path
/// out of the workspace, and refusing it would make the grammar useless. A word
/// that does exist is resolved with the same canonicalization the read tools
/// use, so an in-workspace symbolic link is followed and accepted while one that
/// escapes is refused.
///
/// # Why every word, including the ones that look like flags
///
/// An earlier version skipped anything starting with `-`, on the theory that the
/// grammar had already vetted the flags. It had not vetted these:
///
/// - `cat -- -escape.txt`. Past `--` there are no flags, only operands, and
///   `-escape.txt` is a perfectly legal filename.
/// - `cat '-escape.txt'`. A quoted word takes the operand branch of the grammar,
///   and quoting is not visible here -- by the time an argv exists, `'-x'` and
///   `-x` are the same string.
///
/// Tracking the `--` separator would close the first and not the second. So
/// every word after the executable is resolved. The cost is that a *flag* which
/// happens to name an existing escaping symlink refuses the command; that is
/// both vanishingly rare and the safe direction to be wrong in.
///
/// Residual: this is [`AccessScope::resolve_existing`], so it carries that
/// function's documented TOCTOU limit. A command is not a mutation -- it does
/// not hold a descriptor across a decision -- and closing this would mean
/// resolving operands the child will resolve again for itself anyway.
fn escaping_operand(argv: &[String], scope: &AccessScope, cwd: &Path) -> Option<String> {
    for word in argv.iter().skip(1) {
        if word.is_empty() {
            continue;
        }
        // Resolved against the command's own working directory, which is what
        // the child will do, rather than against the workspace root.
        let candidate = cwd.join(word);
        match scope.resolve_existing(&candidate.to_string_lossy()) {
            Ok(_) => {}
            Err(crate::workspace::PathError::OutsideScope { .. }) => return Some(word.clone()),
            // Anything else -- absent, unreadable, blank -- is not evidence of
            // an escape, and the child will produce its own error for it.
            Err(_) => {}
        }
    }
    None
}

/// The complete environment a command runs with.
///
/// Built rather than inherited. xfx's own process holds a Gateway bearer token,
/// and a child that inherited it could exfiltrate it; a child that never sees it
/// cannot. `PATH` is carried because a direct plan names programs rather than
/// absolute paths, and locale is pinned so command output is stable across
/// machines.
fn minimal_environment() -> Vec<(String, String)> {
    let path = std::env::var("PATH")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string());
    let home = std::env::var("HOME").unwrap_or_default();
    // Sorted by name, so the fingerprint does not depend on the order the
    // variables happened to be read in.
    vec![
        ("HOME".to_string(), home),
        ("LANG".to_string(), "C".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
        ("PATH".to_string(), path),
    ]
}

/// The shell a reviewed command runs under.
fn platform_shell() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("cmd.exe")
    } else {
        PathBuf::from("/bin/sh")
    }
}

// ---------------------------------------------------------------------------
// authorities
// ---------------------------------------------------------------------------

/// Which decision admitted an action, recorded on the authority it produced.
///
/// Kept as a separate tag from the public `AllowSource` so that this module,
/// which owns the plans, does not have to depend on the policy module that owns
/// the vocabulary.
pub(crate) type AllowSourceTag = super::policy::AllowSource;

/// Permission to perform exactly one prepared action, exactly once.
#[derive(Debug)]
pub enum ExecutionAuthority {
    Mutation(PreparedMutation),
    Command(PreparedCommand),
}

impl ExecutionAuthority {
    pub(crate) fn mint_mutation(plan: MutationPlan, source: AllowSourceTag, nonce: Nonce) -> Self {
        Self::Mutation(PreparedMutation {
            plan,
            nonce,
            source,
        })
    }

    pub(crate) fn mint_command(plan: CommandPlan, source: AllowSourceTag, nonce: Nonce) -> Self {
        Self::Command(PreparedCommand {
            plan,
            nonce,
            source,
        })
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            Self::Mutation(prepared) => prepared.nonce,
            Self::Command(prepared) => prepared.nonce,
        }
    }

    /// The decision this authority came from.
    pub fn source(&self) -> AllowSourceTag {
        match self {
            Self::Mutation(prepared) => prepared.source,
            Self::Command(prepared) => prepared.source,
        }
    }

    /// The digest of the plan this authority covers.
    pub fn fingerprint(&self) -> ContentHash {
        match self {
            Self::Mutation(prepared) => prepared.plan.fingerprint(),
            Self::Command(prepared) => prepared.plan.fingerprint(),
        }
    }

    /// The mutation plan, when this authority covers one.
    pub fn mutation(&self) -> Option<&MutationPlan> {
        match self {
            Self::Mutation(prepared) => Some(&prepared.plan),
            Self::Command(_) => None,
        }
    }

    /// The command plan, when this authority covers one.
    pub fn command(&self) -> Option<&CommandPlan> {
        match self {
            Self::Command(prepared) => Some(&prepared.plan),
            Self::Mutation(_) => None,
        }
    }
}

/// Issues the next nonce for a ledger. Only [`super::PermissionSession`] calls
/// this, which is why minting cannot happen outside a session.
pub(crate) fn issue(ledger: &mut AuthorityLedger) -> Nonce {
    ledger.issue()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> (tempfile::TempDir, AccessScope) {
        let dir = tempfile::tempdir().expect("temporary root");
        let scope = AccessScope::primary_only(dir.path()).expect("a usable root");
        (dir, scope)
    }

    #[test]
    fn a_digest_is_stable_and_distinguishes_its_input() {
        assert_eq!(ContentHash::of(b"a"), ContentHash::of(b"a"));
        assert_ne!(ContentHash::of(b"a"), ContentHash::of(b"b"));
        assert_eq!(ContentHash::of(b"").hex().len(), 64);
        assert_eq!(ContentHash::of(b"").short().len(), 12);
        // The empty-input digest is the published SHA-256 constant, so a wrong
        // hasher would fail here rather than silently hashing consistently.
        assert_eq!(
            ContentHash::of(b"").hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_nonce_is_never_issued_twice() {
        let mut ledger = AuthorityLedger::new();
        let first = ledger.issue();
        let second = ledger.issue();
        assert_ne!(first, second);
        assert_eq!(ledger.issued_count(), 2);
    }

    #[test]
    fn a_ledger_spends_a_nonce_once_and_refuses_a_stranger() {
        let mut ledger = AuthorityLedger::new();
        let nonce = ledger.issue();
        assert_eq!(ledger.consume(nonce), Ok(()));
        assert_eq!(ledger.consume(nonce), Err(AuthorityError::Consumed));
        assert_eq!(ledger.consumed_count(), 1);

        let mut other = AuthorityLedger::new();
        assert_eq!(other.consume(nonce), Err(AuthorityError::Unknown));
    }

    #[test]
    fn a_read_tracker_keeps_the_latest_record_for_a_path() {
        let mut tracker = ReadTracker::new();
        assert!(tracker.is_empty());
        let identity = FileIdentity {
            device: 1,
            inode: 2,
            mode: 0o644,
            size: 3,
            mtime_ns: 4,
        };
        tracker.record(
            PathBuf::from("/a"),
            ReadRecord {
                identity,
                hash: ContentHash::of(b"one"),
                complete_view: false,
            },
        );
        tracker.record(
            PathBuf::from("/a"),
            ReadRecord {
                identity,
                hash: ContentHash::of(b"two"),
                complete_view: true,
            },
        );
        assert_eq!(tracker.len(), 1);
        let record = tracker.proof(Path::new("/a")).expect("a record");
        assert!(record.complete_view);
        assert_eq!(record.hash, ContentHash::of(b"two"));
        assert!(tracker.proof(Path::new("/b")).is_none());
    }

    #[test]
    fn a_mutation_fingerprint_changes_with_every_part_of_the_plan() {
        let identity = FileIdentity {
            device: 1,
            inode: 2,
            mode: 0o644,
            size: 3,
            mtime_ns: 4,
        };
        let preimage = Preimage::Present {
            identity,
            hash: ContentHash::of(b"old"),
        };
        let base = MutationPlan::new(
            MutationKind::Edit,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            preimage,
            b"new".to_vec(),
        );
        let other_target = MutationPlan::new(
            MutationKind::Edit,
            PathBuf::from("/w/b.txt"),
            "b.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            preimage,
            b"new".to_vec(),
        );
        let other_bytes = MutationPlan::new(
            MutationKind::Edit,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            preimage,
            b"other".to_vec(),
        );
        let other_preimage = MutationPlan::new(
            MutationKind::Edit,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"new".to_vec(),
        );
        assert_ne!(base.fingerprint(), other_target.fingerprint());
        assert_ne!(base.fingerprint(), other_bytes.fingerprint());
        assert_ne!(base.fingerprint(), other_preimage.fingerprint());
        // The plan names one exact target and one exact replacement, and both
        // are readable back out: an authority that could not say what it covers
        // would not be auditable.
        assert_eq!(base.target(), Path::new("/w/a.txt"));
        assert_eq!(base.staged_bytes(), b"new");
        assert_eq!(base.kind(), MutationKind::Edit);
    }

    // -----------------------------------------------------------------------
    // the bounded diff a screen review is given
    // -----------------------------------------------------------------------

    #[test]
    fn a_diff_side_is_escaped_before_it_is_cut_so_the_bound_is_in_bytes_that_really_exist() {
        // The order is the whole guarantee. Cutting the raw text first and
        // escaping afterwards would let a payload of pure control bytes leave
        // the bound by a factor of three, because each of them becomes a
        // three-byte replacement character *after* the cut has been measured.
        let side = bounded_diff_side(&"\u{1b}".repeat(40_000));
        assert!(
            side.len() <= 65_536,
            "the escaped side outran the literal 64 KiB bound at {} bytes",
            side.len()
        );
        assert!(
            !side.contains('\u{1b}'),
            "an escape survived the bounding: {:?}",
            side.chars().take(8).collect::<String>()
        );
        assert!(side.ends_with('\u{2026}'), "the cut was silent");
    }

    #[test]
    fn a_diff_side_is_cut_on_a_character_boundary_and_never_inside_one() {
        // Every character here is four bytes, so a cut placed by byte index
        // alone would land inside one three times out of four -- and a `String`
        // cannot even hold that, so the failure would be a panic in front of a
        // user rather than a wrong screen.
        let side = bounded_diff_side(&"\u{1f642}".repeat(20_000));
        assert_eq!(side.len(), 65_535);
        assert_eq!(side.chars().filter(|c| *c == '\u{1f642}').count(), 16_383);
        assert!(side.ends_with('\u{2026}'));
    }

    #[test]
    fn a_diff_side_never_cuts_one_of_its_own_escape_tokens_in_half() {
        // A tab leaves this function as **two** characters, and half of one is
        // a lone backslash sitting in front of the ellipsis -- which reads as
        // an escape of the mark that says the text was cut.
        //
        // A tab rather than a line break, and the difference is this side's
        // contract: a real break is **kept** here rather than named, because
        // the surface this side is built for places rows and a break is where a
        // row ends ([`bounded_diff_side`]). The band's own one-line quotation
        // still names it, and is pinned separately.
        let side = bounded_diff_side(&"\t".repeat(40_000));
        assert_eq!(side.len(), 65_535);
        assert_eq!(side.matches("\\t").count(), 32_766);
        assert!(side.ends_with('\u{2026}'));
        assert!(
            !side.trim_end_matches('\u{2026}').ends_with('\\'),
            "the cut left half an escape token: {:?}",
            side.chars().rev().take(4).collect::<String>()
        );

        // A line break costs one byte on this side, so the same count of them
        // is nowhere near the bound and arrives whole.
        let breaks = bounded_diff_side(&"\n".repeat(40_000));
        assert_eq!(breaks.len(), 40_000);
        assert!(!breaks.contains('\u{2026}'), "a side that fits was cut");

        // **And the longest token there is**, which is the one a cut is most
        // likely to land inside: a control named by its code point costs eight
        // bytes, and half of `\u{001B}` is a lone backslash or a `{` with no
        // `}` -- text that reads as an escape of whatever follows it, or as a
        // code point nobody wrote.
        let named = bounded_diff_side(&"\u{1b}".repeat(40_000));
        assert!(named.len() <= 65_536, "{} bytes", named.len());
        assert!(named.ends_with('\u{2026}'), "the cut was silent");
        let body = named.trim_end_matches('\u{2026}');
        assert_eq!(
            body.len() % 8,
            0,
            "the cut left part of a code-point token: {:?}",
            body.chars().rev().take(10).collect::<String>()
        );
        assert_eq!(body.matches("\\u{001B}").count(), body.len() / 8);
    }

    #[test]
    fn a_cut_diff_side_carries_exactly_one_ellipsis_and_pays_for_it_inside_the_bound() {
        // An ellipsis appended *after* the bound was filled is a side one byte
        // -- or three -- longer than the number the channel was sized for.
        let side = bounded_diff_side(&"x".repeat(100_000));
        assert_eq!(
            side.len(),
            65_536,
            "the mark was paid for outside the literal 64 KiB bound"
        );
        assert_eq!(side.chars().filter(|c| *c == '\u{2026}').count(), 1);
        assert_eq!(side.matches('x').count(), 65_533);
    }

    #[test]
    fn a_diff_side_that_fits_is_neither_cut_nor_marked() {
        // The common case, and the one an ellipsis on every side would make a
        // lie: a small change must arrive whole and say so by not saying
        // anything.
        // The line breaks are **kept** and everything else a terminal would act
        // on is named: this side is read by a surface that places rows, and a
        // break is where a row ends. A `\r` is still named, because a CR is a
        // cursor move rather than a line and a reader is owed the fact that the
        // file carries one.
        let side = bounded_diff_side("alpha\n\tbeta\r\n");
        assert_eq!(side, "alpha\n\\tbeta\\r\n");
        assert!(!side.contains('\u{2026}'));

        // And the exact boundary, which is the case an off-by-one lands on: a
        // side that fills the bound to the last byte is not too long for it.
        let exact = bounded_diff_side(&"x".repeat(65_536));
        assert_eq!(exact.len(), 65_536);
        assert!(
            !exact.contains('\u{2026}'),
            "a side that fits exactly was reported as cut"
        );

        // **A control at the boundary**, which is the case a cost asked *after*
        // the character was spent would miss: the eight bytes it becomes are
        // eight bytes the bound has to know about **before** they are added,
        // and a payload that ends on one exhausts the loop rather than breaking
        // out of it -- so a cost read from the character instead of from its
        // token simply returns a side the channel was not sized for.
        for filler in [65_529usize, 65_530, 65_535] {
            let mixed = bounded_diff_side(&format!("{}\u{1b}", "x".repeat(filler)));
            assert!(
                mixed.len() <= 65_536,
                "a control at byte {filler} left here as {} bytes",
                mixed.len()
            );
            assert!(
                mixed.ends_with('\u{2026}'),
                "a side that had to be cut at byte {filler} did not say so"
            );
        }

        // **And the byte after it**, which is the same off-by-one from the
        // other direction and the one a bound asked *after* a character was
        // spent would miss: the payload runs out one byte past the bound, so a
        // check made too late has nothing left to break on and simply returns a
        // side the channel was not sized for.
        for over in [65_537usize, 65_538, 65_539] {
            let past = bounded_diff_side(&"x".repeat(over));
            assert!(
                past.len() <= 65_536,
                "a {over}-byte payload left here as {} bytes",
                past.len()
            );
            assert!(
                past.ends_with('\u{2026}'),
                "a side that had to be cut did not say so at {over} bytes"
            );
        }
    }

    #[test]
    fn the_bands_own_excerpt_still_escapes_and_marks_exactly_as_it_did() {
        // The two bounded renderings share their escape rules
        // ([`Escaped`]), so the summary the band quotes is pinned here in its
        // own right: a change made for the screen's payload must not silently
        // move what the line shell and the panel have always shown.
        assert_eq!(bounded_excerpt("alpha\n\tbeta\r"), "alpha\\n\\tbeta\\r");
        // The escape is named by its code point rather than blanked to one
        // symbol, which is what keeps two controls a payload can swap apart.
        assert_eq!(bounded_excerpt("\u{1b}[2J"), "\\u{001B}[2J");
        assert_eq!(bounded_excerpt(""), "");

        let cut = bounded_excerpt(&"x".repeat(400));
        assert!(cut.ends_with('\u{2026}'), "{cut:?}");
        assert_eq!(cut.matches('x').count(), 160);

        // 160 characters is the last input that is *not* cut: the check is made
        // before a character is added, so the mark appears only when there was
        // something left to add.
        let whole = bounded_excerpt(&"x".repeat(160));
        assert_eq!(whole.len(), 160);
        assert!(!whole.contains('\u{2026}'), "{whole:?}");
    }

    #[test]
    fn a_line_break_and_the_two_characters_that_spell_one_are_not_the_same_change() {
        // **The collision this rendering exists to not have.** A payload whose
        // line breaks are real and a payload that merely contains a backslash
        // and an `n` are two different files, and a review surface that showed
        // them as one string would let a model replace a hundred-line file with
        // one line of literal escapes and have the screen call it a no-op.
        //
        // The exact pair a reviewer found, at the size it was found at.
        let real = "A\n".repeat(100);
        let literal = "A\\n".repeat(100);
        assert_ne!(real, literal, "the fixture is not two different payloads");

        let diff = ApprovalDiff::of(&real, &literal);

        assert_ne!(
            diff.before, diff.after,
            "a change of every line break in a file rendered as a change of nothing"
        );
        // And the difference is the **structure**, not a spelling: the side
        // whose breaks are real keeps them, so the surface that places rows can
        // still see where the lines were.
        assert_eq!(diff.before.matches('\n').count(), 100);
        assert!(
            !diff.after.contains('\n'),
            "a payload with no line break in it was given one: {:?}",
            &diff.after[..16]
        );
    }

    #[test]
    fn two_payloads_that_differ_are_never_rendered_as_one_side() {
        // Injectivity, over exactly the characters the rendering spends a
        // backslash on. Every pair here differs by one character, and each
        // would collide with its neighbour under an escaping that wrote a
        // backslash without escaping the backslash itself.
        let payloads = [
            "a\nb", "a\\nb", "a\\\\nb", "a\tb", "a\\tb", "a\\\\tb", "a\rb", "a\\rb", "\\", "\\\\",
            "\\n", "\n", "",
        ];
        let mut seen: Vec<(&str, String)> = Vec::new();
        for payload in payloads {
            let rendered = bounded_diff_side(payload);
            if let Some((other, _)) = seen.iter().find(|(_, side)| side == &rendered) {
                panic!("{payload:?} and {other:?} render as the same side: {rendered:?}");
            }
            seen.push((payload, rendered));
        }

        // The same claim for the band's own one-line quotation, which spends
        // backslashes for the same reason and had the same collision.
        let mut seen: Vec<(&str, String)> = Vec::new();
        for payload in payloads {
            let rendered = bounded_excerpt(payload);
            if let Some((other, _)) = seen.iter().find(|(_, side)| side == &rendered) {
                panic!("{payload:?} and {other:?} are quoted identically: {rendered:?}");
            }
            seen.push((payload, rendered));
        }
    }

    #[test]
    fn a_backslash_costs_what_it_costs_inside_the_bound_rather_than_past_it() {
        // The worst case for a rendering that escapes the escape: a payload of
        // nothing but backslashes doubles. The bound is a promise about the
        // bytes that **leave**, so it has to be measured after that doubling --
        // a side cut to 64 KiB of input and then doubled would be 128 on a
        // channel sized for 64.
        let side = bounded_diff_side(&"\\".repeat(100_000));
        assert!(
            side.len() <= 65_536,
            "the doubling was paid for past the bound, at {} bytes",
            side.len()
        );
        assert_eq!(side.chars().filter(|c| *c == '\u{2026}').count(), 1);
        // And every backslash arrived as a whole token: an odd count is a lone
        // one, which reads as an escape of whatever follows it -- here, of the
        // mark that says the text was cut.
        let body = side.trim_end_matches('\u{2026}');
        assert!(body.chars().all(|character| character == '\\'), "{body:?}");
        assert_eq!(
            body.len() % 2,
            0,
            "the cut left half of an escaped backslash"
        );

        let quoted = bounded_excerpt(&"\\".repeat(400));
        assert!(quoted.len() <= 160 + '\u{2026}'.len_utf8());
        assert!(quoted.ends_with('\u{2026}'));
    }

    #[test]
    fn two_different_control_bytes_are_not_the_same_change() {
        // **The second collision, and it is the same defect one level down.**
        // Escaping every unnamed control to one replacement character makes the
        // rendering injective on line breaks and not on anything else: a file
        // of `ESC` and a file of `BEL` are two different files, and a review
        // that showed them as one string would let a model replace one with the
        // other and have the screen call it a no-op.
        //
        // The two pairs a reviewer named, at the size that reaches the screen.
        for (before, after) in [
            ("\u{1b}", "\u{7}"),
            // The C1 pair, which is worse: a single scalar that *is* a `CSI` on
            // a terminal that decodes eight-bit controls, against one that is
            // an `OSC`.
            ("\u{9b}", "\u{9d}"),
        ] {
            let old = before.repeat(161);
            let new = after.repeat(161);
            let diff = ApprovalDiff::of(&old, &new);
            assert_ne!(
                diff.before, diff.after,
                "{before:?} replaced by {after:?} rendered as a change of nothing"
            );
            assert!(
                diff.wants_screen(),
                "a change this size is not one the band's summary can show"
            );
        }
    }

    #[test]
    fn every_control_scalar_has_a_representation_of_its_own_and_none_of_them_is_a_control() {
        // **The whole domain, not the pairs somebody thought of.** A test that
        // named `ESC` and `BEL` would pass for a rendering that collapsed the
        // other sixty-three, so this walks every scalar Rust accepts and asks
        // the two questions that matter of each: is what leaves here unique,
        // and is it something a terminal draws rather than obeys.
        let controls: Vec<char> = (0..=0x10_ffffu32)
            .filter_map(char::from_u32)
            .filter(|character| character.is_control())
            .collect();
        assert_eq!(
            controls.len(),
            65,
            "the control domain is not what this test walked"
        );

        let mut seen: Vec<(char, String)> = Vec::new();
        for control in &controls {
            let rendered = bounded_diff_side(&control.to_string());
            if let Some((other, _)) = seen.iter().find(|(_, side)| side == &rendered) {
                panic!("{control:?} and {other:?} render as the same side: {rendered:?}");
            }
            // The **one** control that leaves here as itself, and it is the
            // change's own structure rather than something a terminal obeys in
            // the middle of a row: the surface that places rows splits on it
            // before it paints anything (`super::super::tui::approval_screen`).
            if *control == '\n' {
                assert_eq!(rendered, "\n");
            } else {
                assert!(
                    !rendered.chars().any(char::is_control),
                    "{control:?} left as something a terminal acts on: {rendered:?}"
                );
            }
            seen.push((*control, rendered));
        }

        // The same, for the band's one-line quotation -- which has no structure
        // to keep, so **no** control survives it at all.
        let mut seen: Vec<(char, String)> = Vec::new();
        for control in &controls {
            let quoted = bounded_excerpt(&control.to_string());
            assert!(
                !quoted.chars().any(char::is_control),
                "{control:?} survived the band's own quotation: {quoted:?}"
            );
            if let Some((other, _)) = seen.iter().find(|(_, side)| side == &quoted) {
                panic!("{control:?} and {other:?} are quoted identically: {quoted:?}");
            }
            seen.push((*control, quoted));
        }

        // And a control's representation is never something a payload could
        // have written itself: the literal spelling of it is escaped again,
        // because the backslash it starts with is.
        for control in &controls {
            let spelled = format!("\\u{{{:04X}}}", *control as u32);
            assert_ne!(
                bounded_diff_side(&spelled),
                bounded_diff_side(&control.to_string()),
                "a payload spelling {control:?} out is not told apart from the control itself"
            );
            assert_ne!(
                bounded_excerpt(&spelled),
                bounded_excerpt(&control.to_string()),
                "a quoted payload spelling {control:?} out is not told apart from it"
            );
        }
    }

    #[test]
    fn a_control_byte_in_a_diff_cannot_become_a_csi_or_an_osc() {
        // Both introducers, because they are two different bytes: `ESC [` is
        // the seven-bit CSI and `U+009B` is the eight-bit one a terminal in a
        // single-byte locale decodes on its own.
        let side = bounded_diff_side("\u{1b}[2J\u{9b}31m\u{9d}0;pwned\u{7}\u{0}");
        assert!(
            !side.chars().any(char::is_control),
            "a control character reached the payload: {side:?}"
        );
        for injected in ['\u{1b}', '\u{9b}', '\u{9d}', '\u{7}', '\u{0}'] {
            assert!(!side.contains(injected), "{injected:?} survived: {side:?}");
        }
        // **Each of them by name.** One symbol for all four would say that
        // something was there and hide which, and a review is asked exactly
        // that question.
        for (injected, named) in [
            ('\u{1b}', "\\u{001B}"),
            ('\u{9b}', "\\u{009B}"),
            ('\u{9d}', "\\u{009D}"),
            ('\u{7}', "\\u{0007}"),
            ('\u{0}', "\\u{0000}"),
        ] {
            assert!(
                side.contains(named),
                "{injected:?} was not named on the payload: {side:?}"
            );
        }
        assert!(
            side.contains("[2J") && side.contains("pwned"),
            "the visible text was dropped instead of being disarmed: {side:?}"
        );
    }

    #[test]
    fn both_sides_of_a_diff_are_bounded_so_one_question_carries_at_most_128_kib() {
        let diff = ApprovalDiff::of(&"a".repeat(200_000), &"b".repeat(200_000));
        assert_eq!(diff.before.len(), 65_536);
        assert_eq!(diff.after.len(), 65_536);
        assert!(diff.before.len() + diff.after.len() <= 131_072);
    }

    #[test]
    fn a_change_the_bands_own_summary_could_not_have_shown_asks_for_a_screen() {
        // 160 bytes is what the inline summary quotes of each side, and the
        // literal is written here rather than imported: the rule this pins is
        // "bigger than what the band already shows", and a test that imported
        // the number would follow it wherever it went instead of noticing.
        assert!(!ApprovalDiff::of(&"a".repeat(160), &"b".repeat(160)).wants_screen());
        assert!(ApprovalDiff::of(&"a".repeat(161), "b").wants_screen());
        assert!(ApprovalDiff::of("a", &"b".repeat(161)).wants_screen());
        assert!(!ApprovalDiff::of("", "").wants_screen());
    }

    #[test]
    fn a_plan_carries_the_diff_a_screen_would_show_without_changing_what_it_is_a_plan_for() {
        // The diff is a *rendering* of data the fingerprint already covers, so
        // attaching one must not move the identity of the change -- the same
        // rule the excerpt is held to.
        let plan = MutationPlan::new(
            MutationKind::Edit,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"new".to_vec(),
        );
        let fingerprint = plan.fingerprint();
        assert!(
            plan.diff().is_none(),
            "a plan invents a diff nobody gave it"
        );

        let plan = plan.with_diff(ApprovalDiff::of("alpha\n", "beta"));
        let carried = plan.diff().expect("the plan kept the diff");
        // The break is kept rather than named on this side: see
        // [`bounded_diff_side`].
        assert_eq!(carried.before, "alpha\n");
        assert_eq!(carried.after, "beta");
        assert_eq!(
            plan.fingerprint(),
            fingerprint,
            "a rendering changed the identity of the change it renders"
        );
    }

    #[test]
    fn a_plan_that_would_write_back_what_it_read_is_a_noop() {
        let identity = FileIdentity {
            device: 1,
            inode: 2,
            mode: 0o644,
            size: 3,
            mtime_ns: 4,
        };
        let same = MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Present {
                identity,
                hash: ContentHash::of(b"same"),
            },
            b"same".to_vec(),
        );
        assert!(same.is_noop());

        let creating = MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            Vec::new(),
        );
        // Creating an empty file is a change even though the bytes are empty.
        assert!(!creating.is_noop());
    }

    #[test]
    fn a_command_plan_never_inherits_the_process_environment() {
        let (_dir, scope) = scope();
        let plan = CommandPlan::prepare("pwd", &scope, None, &ToolLimits::default())
            .expect("a plannable command");
        let names: Vec<&str> = plan
            .environment()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["HOME", "LANG", "LC_ALL", "PATH"]);
        assert_eq!(plan.display_cwd(), ".");
    }

    #[test]
    fn a_command_that_is_empty_or_oversized_is_refused_before_it_is_classified() {
        let (_dir, scope) = scope();
        let limits = ToolLimits::default();
        assert!(CommandPlan::prepare("   ", &scope, None, &limits).is_err());
        let long = "a".repeat(MAX_COMMAND_BYTES + 1);
        let message = CommandPlan::prepare(&long, &scope, None, &limits).expect_err("too long");
        assert!(
            message.contains(&MAX_COMMAND_BYTES.to_string()),
            "{message}"
        );
    }

    #[test]
    fn an_admitted_git_runs_with_the_repositorys_own_hooks_and_diff_drivers_off() {
        let (_dir, scope) = scope();
        let limits = ToolLimits::default();

        let status = CommandPlan::prepare("git status --short", &scope, None, &limits)
            .expect("a plannable command");
        let CommandRoute::Direct { argv } = status.route() else {
            panic!("`git status` is a direct plan");
        };
        assert_eq!(
            argv,
            &["git", "-c", "core.fsmonitor=false", "status", "--short"]
        );

        // The diff family gets the two options that turn off the commands a
        // repository can configure for it.
        let diff = CommandPlan::prepare("git diff --stat", &scope, None, &limits).expect("a plan");
        let CommandRoute::Direct { argv } = diff.route() else {
            panic!("`git diff` is a direct plan");
        };
        assert_eq!(
            argv,
            &[
                "git",
                "-c",
                "core.fsmonitor=false",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--stat"
            ]
        );

        // The agreement is unchanged: the text a rule or a grant is keyed on,
        // and the sentence a human was shown, are still what the model wrote.
        assert_eq!(diff.command(), "git diff --stat");

        // Nothing else is rewritten.
        let other = CommandPlan::prepare("cargo --version", &scope, None, &limits).expect("a plan");
        let CommandRoute::Direct { argv } = other.route() else {
            panic!("`cargo --version` is a direct plan");
        };
        assert_eq!(argv, &["cargo", "--version"]);
    }

    #[test]
    fn an_unusable_working_directory_is_named_rather_than_silently_replaced() {
        let (_dir, scope) = scope();
        let limits = ToolLimits::default();
        let message = CommandPlan::prepare("pwd", &scope, Some("nope"), &limits)
            .expect_err("a missing cwd is not usable");
        assert!(message.contains("nope"), "{message}");
    }
}
