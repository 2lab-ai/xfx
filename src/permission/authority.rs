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
/// (`vercel-labs/fx@580a0c5d src/core/terminal/contracts.zig:8` is 64 KiB; fxr
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
    /// Whether the *model* saw the whole file, as opposed to fxr having hashed
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

/// Renders `text` on one line, bounded, with the clipping made visible.
///
/// Newlines and other control characters are escaped rather than printed: an
/// approval prompt that a payload can reflow is an approval prompt a payload can
/// disguise.
pub fn bounded_excerpt(text: &str) -> String {
    let mut out = String::new();
    let mut clipped = false;
    for ch in text.chars() {
        if out.len() >= MAX_EXCERPT_BYTES {
            clipped = true;
            break;
        }
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    if clipped {
        out.push('\u{2026}');
    }
    out
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
            fingerprint,
        }
    }

    /// The same plan, carrying the before/after a prompt will show.
    pub fn with_excerpt(mut self, excerpt: MutationExcerpt) -> Self {
        self.excerpt = Some(excerpt);
        self
    }

    /// What this change replaces and with what, when the tool could say.
    pub fn excerpt(&self) -> Option<&MutationExcerpt> {
        self.excerpt.as_ref()
    }

    /// A bounded, escaped preview of the bytes that will be written.
    pub fn preview(&self) -> String {
        match std::str::from_utf8(&self.after) {
            Ok(text) => bounded_excerpt(text),
            Err(_) => format!("<{} bytes of non-UTF-8 data>", self.after.len()),
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
            CommandEffect::DirectReadOnly { argv } => CommandRoute::Direct { argv: argv.clone() },
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
            hasher.update([b'=']);
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

/// The first operand of `argv` that exists and resolves outside the scope.
///
/// Only *existing* names are resolved. A word that names nothing -- a grep
/// pattern, a git revision, an argument to a test harness -- cannot be a path
/// out of the workspace, and refusing it would make the grammar useless. A word
/// that does exist is resolved with the same canonicalization the read tools
/// use, so an in-workspace symbolic link is followed and accepted while one that
/// escapes is refused.
///
/// Words beginning with `-` are skipped: the grammar already restricted the
/// flags a command may carry, and treating `--check` as a candidate path would
/// resolve whatever a file of that name happened to be.
///
/// Residual: this is [`AccessScope::resolve_existing`], so it carries that
/// function's documented TOCTOU limit. A command is not a mutation -- it does
/// not hold a descriptor across a decision -- and closing this would mean
/// resolving operands the child will resolve again for itself anyway.
fn escaping_operand(argv: &[String], scope: &AccessScope, cwd: &Path) -> Option<String> {
    for word in argv.iter().skip(1) {
        if word.starts_with('-') || word.is_empty() {
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
/// Built rather than inherited. fxr's own process holds a Gateway bearer token,
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
    fn an_unusable_working_directory_is_named_rather_than_silently_replaced() {
        let (_dir, scope) = scope();
        let limits = ToolLimits::default();
        let message = CommandPlan::prepare("pwd", &scope, Some("nope"), &limits)
            .expect_err("a missing cwd is not usable");
        assert!(message.contains("nope"), "{message}");
    }
}
