//! Changing files: `write_file`, `edit_file`, and `create_folder`.
//!
//! # The shape of one mutation
//!
//! ```text
//! decode -> validate -> locate -> read preimage -> plan -> policy -> mint
//!        -> consume -> revalidate -> stage -> revalidate -> rename -> fsync
//! ```
//!
//! Nothing before `stage` writes anything. That is what makes the permission
//! decision meaningful: by the time policy runs, the change is a value -- one
//! canonical target, the exact bytes that are there now, the exact bytes that
//! would replace them -- and the only thing execution can do is apply that
//! value or refuse.
//!
//! # Why a directory descriptor and not a path
//!
//! Task 3's [`crate::workspace::AccessScope`] proves a path was inside an
//! authorized root *at the moment `canonicalize` returned*, and its module docs
//! record the residual: between that proof and the `open` that follows, a
//! component of the path can be replaced. A mutation cannot live with that,
//! because the window is wider (it spans a permission decision, possibly a human
//! being asked a question) and the consequence is a write rather than a read.
//!
//! So this module never opens a mutation target by path. It walks the path one
//! component at a time from an authorized root's descriptor, with
//! `openat(..., O_NOFOLLOW)` at every step, keeps the parent descriptor open for
//! the whole operation, and does the final create/rename *relative to that
//! descriptor*. A component swapped after it was checked cannot be reached,
//! because the name is never resolved again. A symlink anywhere on the path is
//! refused rather than followed -- stricter than the read tools, deliberately:
//! following a link to read is recoverable, and following one to write is not.
//!
//! What this still does not cover is written down in
//! [`self::namespace`]'s "Residual" note rather than implied away.
//!
//! # Why a read is required first
//!
//! Replacing a file the model has not read discards content nobody looked at.
//! So an existing target needs a complete read proof from this session, and the
//! proof has to still be true: the digest recorded at read time is compared with
//! the file's digest now, and a mismatch is a refusal that says to read again
//! (`vercel-labs/fx@580a0c5d src/core/workspace/read_tracker.zig:9-20`).
//!
//! # What no mode may write
//!
//! Two directory names are refused *structurally*, before policy is consulted
//! and in every permission mode:
//! [`crate::workspace::PROTECTED_WRITE_DIRECTORY_NAMES`]. They are where the
//! authority is configured rather than where the work is: `.git/config` is
//! executable input to git, so a bounded workspace write followed by an
//! admitted `git status` or `git diff` would be an arbitrary command with no
//! approval anywhere on the path, and `.xfx` is the profile home an approval is
//! recorded in. `yolo` can still run an explicitly approved terminal command
//! that touches them -- the claim is about the typed file tools, which never
//! rewrite their own or git's authority metadata.

use serde_json::Value;

use crate::permission::{
    bounded_excerpt, ApprovalDiff, ContentHash, MutationExcerpt, MutationKind, MutationPlan,
    PolicyDecision, Preimage, ProposedAction,
};

use super::spec::{
    nonblank, object, required_string, InputSchema, PermissionKind, Property, PropertyKind,
    ToolContext, ToolInput, ToolResult, ToolSpec,
};

use namespace::Location;

// ---------------------------------------------------------------------------
// descriptions
// ---------------------------------------------------------------------------

/// Shared by every mutating tool: what a path may be, and what it may not.
const PATH_DESCRIPTION: &str = "Path to change, relative to the workspace root or absolute inside an authorized root. \
Every component is opened without following symbolic links, so a path through a link is refused rather than redirected. \
`..` components are refused; name the path from the workspace root instead.";

const WRITE_FILE_DESCRIPTION: &str = "Create a file, or replace an existing file's entire contents. \
An existing file must have been read in full with read_file first, and must not have changed since that read; \
otherwise the call is refused so that unseen content is never discarded. \
The replacement is staged in the same directory and renamed into place, so a reader never sees a half-written file, \
and the previous permission bits are preserved. \
When to use: add a new file, or intentionally rewrite a small or generated one. \
When NOT to use: a focused change to an existing file (use edit_file), creating directories (use create_folder), or deleting anything.";

const EDIT_FILE_DESCRIPTION: &str = "Replace exactly one occurrence of old_string with new_string in an existing UTF-8 text file. \
The file must have been read in full with read_file first and must not have changed since. \
old_string must appear exactly once: if it appears zero times or more than once the call is refused rather than guessing, \
so include enough surrounding text to make it unique. \
An edit whose result equals the current contents changes nothing and says so. \
When to use: a focused patch after reading the file. \
When NOT to use: whole-file rewrites (use write_file), ambiguous repeated text, or files you have not read.";

const CREATE_FOLDER_DESCRIPTION: &str = "Create a directory, including any missing parent directories. \
Existing directories are left alone and reported as already present. \
When to use: prepare a location for files you are about to write. \
When NOT to use: create files, inspect a directory (use list_files), or build speculative structure the task did not ask for.";

// ---------------------------------------------------------------------------
// specs
// ---------------------------------------------------------------------------

pub const WRITE_FILE: ToolSpec = ToolSpec::new(
    "write_file",
    WRITE_FILE_DESCRIPTION,
    PermissionKind::MutateFile,
    InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: PATH_DESCRIPTION,
                allowed: &[],
            },
            Property {
                name: "content",
                kind: PropertyKind::String,
                description: "The complete new contents of the file.",
                allowed: &[],
            },
        ],
        required: &["path", "content"],
    },
    decode_write_file,
    validate_write_file,
    execute_write_file,
);

pub const EDIT_FILE: ToolSpec = ToolSpec::new(
    "edit_file",
    EDIT_FILE_DESCRIPTION,
    PermissionKind::MutateFile,
    InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: PATH_DESCRIPTION,
                allowed: &[],
            },
            Property {
                name: "old_string",
                kind: PropertyKind::String,
                description: "The exact text to replace. Must occur exactly once in the file.",
                allowed: &[],
            },
            Property {
                name: "new_string",
                kind: PropertyKind::String,
                description: "The text to put in its place.",
                allowed: &[],
            },
        ],
        required: &["path", "old_string", "new_string"],
    },
    decode_edit_file,
    validate_edit_file,
    execute_edit_file,
);

pub const CREATE_FOLDER: ToolSpec = ToolSpec::new(
    "create_folder",
    CREATE_FOLDER_DESCRIPTION,
    PermissionKind::MutateFile,
    InputSchema {
        properties: &[Property {
            name: "path",
            kind: PropertyKind::String,
            description: PATH_DESCRIPTION,
            allowed: &[],
        }],
        required: &["path"],
    },
    decode_create_folder,
    validate_create_folder,
    execute_create_folder,
);

// ---------------------------------------------------------------------------
// inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteFileInput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditFileInput {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFolderInput {
    pub path: String,
}

fn decode_write_file(input: &Value) -> Result<ToolInput, String> {
    let object = object("write_file", input)?;
    Ok(ToolInput::WriteFile(WriteFileInput {
        path: required_string("write_file", object, "path")?,
        content: required_string("write_file", object, "content")?,
    }))
}

fn validate_write_file(input: &ToolInput) -> Result<(), String> {
    let ToolInput::WriteFile(input) = input else {
        return Err(mismatched("write_file"));
    };
    nonblank("write_file", "path", &input.path)
}

fn decode_edit_file(input: &Value) -> Result<ToolInput, String> {
    let object = object("edit_file", input)?;
    Ok(ToolInput::EditFile(EditFileInput {
        path: required_string("edit_file", object, "path")?,
        old_string: required_string("edit_file", object, "old_string")?,
        new_string: required_string("edit_file", object, "new_string")?,
    }))
}

fn validate_edit_file(input: &ToolInput) -> Result<(), String> {
    let ToolInput::EditFile(input) = input else {
        return Err(mismatched("edit_file"));
    };
    nonblank("edit_file", "path", &input.path)?;
    if input.old_string.is_empty() {
        // An empty needle matches everywhere, so "replace exactly one
        // occurrence" has no meaning for it.
        return Err("edit_file field `old_string` must not be empty".to_string());
    }
    Ok(())
}

fn decode_create_folder(input: &Value) -> Result<ToolInput, String> {
    let object = object("create_folder", input)?;
    Ok(ToolInput::CreateFolder(CreateFolderInput {
        path: required_string("create_folder", object, "path")?,
    }))
}

fn validate_create_folder(input: &ToolInput) -> Result<(), String> {
    let ToolInput::CreateFolder(input) = input else {
        return Err(mismatched("create_folder"));
    };
    nonblank("create_folder", "path", &input.path)
}

fn mismatched(tool: &str) -> String {
    format!("{tool} received arguments that belong to another tool")
}

// ---------------------------------------------------------------------------
// executors
// ---------------------------------------------------------------------------

fn execute_write_file(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::WriteFile(input) = input else {
        return ToolResult::failure(mismatched("write_file"));
    };
    let bytes = input.content.as_bytes();
    if let Err(reason) = bounded("write_file", "content", bytes.len(), context) {
        return ToolResult::failure(reason);
    }

    let located = match namespace::locate(context.scope(), &input.path, "write_file") {
        Ok(located) => located,
        Err(reason) => return ToolResult::failure(reason),
    };
    let preimage = match read_preimage(&located, "write_file", context) {
        Ok(preimage) => preimage,
        Err(reason) => return ToolResult::failure(reason),
    };
    if let Some(reason) = read_proof_missing("write_file", context, &located, &preimage) {
        return ToolResult::failure(reason);
    }

    let plan = MutationPlan::new(
        MutationKind::Write,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        preimage.summary(),
        bytes.to_vec(),
    );
    commit(context, located, plan, |plan| {
        format!(
            "Wrote {} ({} bytes)",
            plan.display(),
            plan.staged_bytes().len()
        )
    })
}

fn execute_edit_file(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::EditFile(input) = input else {
        return ToolResult::failure(mismatched("edit_file"));
    };
    for (field, value) in [
        ("old_string", &input.old_string),
        ("new_string", &input.new_string),
    ] {
        if let Err(reason) = bounded("edit_file", field, value.len(), context) {
            return ToolResult::failure(reason);
        }
    }

    let located = match namespace::locate(context.scope(), &input.path, "edit_file") {
        Ok(located) => located,
        Err(reason) => return ToolResult::failure(reason),
    };
    let preimage = match read_preimage(&located, "edit_file", context) {
        Ok(preimage) => preimage,
        Err(reason) => return ToolResult::failure(reason),
    };
    let TargetState::Present { bytes, .. } = &preimage else {
        return ToolResult::failure(format!(
            "edit_file cannot edit a file that does not exist: no such path: `{}`",
            input.path
        ));
    };
    if let Some(reason) = read_proof_missing("edit_file", context, &located, &preimage) {
        return ToolResult::failure(reason);
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return ToolResult::failure(format!(
            "edit_file cannot edit `{}`: it is not UTF-8 text",
            located.display()
        ));
    };
    let occurrences = text.matches(input.old_string.as_str()).count();
    match occurrences {
        0 => {
            return ToolResult::failure(format!(
                "edit_file made no change: old_string was not found in `{}`",
                located.display()
            ))
        }
        1 => {}
        many => {
            return ToolResult::failure(format!(
                "edit_file made no change: old_string appears {many} times in `{}`; include enough surrounding text for exactly one match",
                located.display()
            ))
        }
    }
    let after = text.replacen(input.old_string.as_str(), &input.new_string, 1);
    if let Err(reason) = bounded("edit_file", "result", after.len(), context) {
        return ToolResult::failure(reason);
    }

    // The excerpt is what an approval prompt shows. It is built from the exact
    // strings the model sent, bounded and escaped, so a human is asked about the
    // change rather than about the file's name.
    //
    // The diff beside it is the same pair, bounded far wider, for a review with
    // room for it. Both are built **here**, from the strings that produced the
    // staged bytes, rather than reconstructed by whatever renders the question:
    // a payload rebuilt at the surface would be a second reading of the change,
    // and only one of the two would be the one being authorized. The bound is
    // applied at this boundary for the same reason -- `max_mutation_bytes` is
    // four megabytes, and nothing downstream may be handed that.
    let plan = MutationPlan::new(
        MutationKind::Edit,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        preimage.summary(),
        after.into_bytes(),
    )
    .with_excerpt(MutationExcerpt {
        before: bounded_excerpt(&input.old_string),
        after: bounded_excerpt(&input.new_string),
    })
    .with_diff(ApprovalDiff::of(&input.old_string, &input.new_string));
    commit(context, located, plan, |plan| {
        format!("Edited {}", plan.display())
    })
}

fn execute_create_folder(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::CreateFolder(input) = input else {
        return ToolResult::failure(mismatched("create_folder"));
    };
    let located = match namespace::locate(context.scope(), &input.path, "create_folder") {
        Ok(located) => located,
        Err(reason) => return ToolResult::failure(reason),
    };

    // Checking whether the directory is already there is a read, so it happens
    // before any decision: creating nothing needs no authority.
    match namespace::directory_exists(&located) {
        Ok(true) => {
            return ToolResult::success(
                format!("{} already exists", located.display()),
                format!("{} already exists", located.display()),
            )
        }
        Ok(false) => {}
        Err(reason) => return ToolResult::failure(format!("create_folder {reason}")),
    }

    let plan = MutationPlan::new(
        MutationKind::CreateFolder,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        Preimage::Absent,
        Vec::new(),
    );
    commit(context, located, plan, |plan| {
        format!("Created {}", plan.display())
    })
}

/// Refuses a value larger than one mutation may carry.
fn bounded(tool: &str, field: &str, len: usize, context: &ToolContext) -> Result<(), String> {
    let limit = context.limits().max_mutation_bytes;
    if len > limit {
        return Err(format!(
            "{tool} refuses `{field}` of {len} bytes; one change may carry at most {limit} bytes"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// preimages and read proofs
// ---------------------------------------------------------------------------

/// What was at the target when the plan was prepared, with its bytes.
///
/// [`crate::permission::Preimage`] carries the identity and digest that travel
/// in the plan; this carries the content too, which the edit needs and the plan
/// deliberately does not keep.
enum TargetState {
    Absent,
    Present {
        identity: crate::permission::FileIdentity,
        hash: ContentHash,
        bytes: Vec<u8>,
    },
}

impl TargetState {
    fn summary(&self) -> Preimage {
        match self {
            Self::Absent => Preimage::Absent,
            Self::Present { identity, hash, .. } => Preimage::Present {
                identity: *identity,
                hash: *hash,
            },
        }
    }
}

/// Reads whatever is at the located target, without following a symlink.
///
/// Bounded by the same ceiling a complete `read_file` runs under, and bounded
/// against the file's stat rather than against what a read turned out to
/// produce, so an enormous target is refused before it is allocated.
fn read_preimage(
    located: &Location,
    tool: &str,
    context: &ToolContext,
) -> Result<TargetState, String> {
    match namespace::read_target(located, context.limits().max_read_bytes) {
        Ok(None) => Ok(TargetState::Absent),
        Ok(Some((identity, bytes))) => {
            let hash = ContentHash::of(&bytes);
            Ok(TargetState::Present {
                identity,
                hash,
                bytes,
            })
        }
        Err(reason) => Err(format!("{tool} {reason}")),
    }
}

/// Why this session may not replace an existing file, if it may not.
///
/// Three distinct answers, because they call for three different actions: read
/// it, read all of it, or read it again.
fn read_proof_missing(
    tool: &str,
    context: &ToolContext,
    located: &Location,
    preimage: &TargetState,
) -> Option<String> {
    let TargetState::Present { hash, .. } = preimage else {
        return None;
    };
    let display = located.display();
    let reads = context.reads();
    let Some(record) = reads.proof(located.full()) else {
        return Some(format!(
            "{tool} will not replace `{display}` because this session has not read it; call read_file on it first"
        ));
    };
    if !record.complete_view {
        return Some(format!(
            "{tool} will not replace `{display}` because this session has only read part of it; read the whole file first"
        ));
    }
    if record.hash != *hash {
        return Some(format!(
            "{tool} will not replace `{display}` because it changed after this session read it; read it again first"
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// the authorized commit
// ---------------------------------------------------------------------------

/// Runs one prepared plan through policy and, if allowed, applies it once.
///
/// `describe` renders the success line, so each tool says what it did in its own
/// words while every tool shares one authorization path.
fn commit(
    context: &ToolContext,
    located: Location,
    plan: MutationPlan,
    describe: impl Fn(&MutationPlan) -> String,
) -> ToolResult {
    let tool = plan.kind().tool();

    // A change that changes nothing is not a change. Reporting it as one would
    // make the model believe it had made progress it had not, and rewriting the
    // file would move its mtime for no reason.
    if plan.is_noop() {
        return ToolResult::success(
            format!("No changes to {}", plan.display()),
            format!("{} is already what was asked for", plan.display()),
        );
    }

    // The guard is released by the end of this statement: minting takes the same
    // lock, and holding it across both would deadlock the session.
    let decision = context
        .permissions()
        .decide(ProposedAction::Mutation(&plan));
    let source = match decision {
        PolicyDecision::Allow { source } => source,
        PolicyDecision::Deny { reason, .. } => {
            return ToolResult::failure(format!("{tool} was not permitted: {reason}"))
        }
        // `decide` resolves every prompt; this arm exists so that a future
        // decision variant cannot be silently treated as an approval.
        PolicyDecision::Prompt => {
            return ToolResult::failure(format!(
                "{tool} was not permitted: the approval was never resolved"
            ))
        }
    };

    let authority = context.permissions().mint_mutation(plan, source);
    // Spend first, check second. Whatever happens below -- a stale preimage, a
    // full disk, a panic-free error path -- this authority is already gone, so a
    // retry has to be authorized again rather than reusing an answer about a
    // world that has since moved.
    if let Err(err) = context.permissions().consume(&authority) {
        return ToolResult::revoked(format!("{tool} could not use its authority: {err}"));
    }

    // The race window, made observable. Nothing in the product installs an
    // interlude; a test does, to change the filesystem at exactly this instant.
    context.run_race_interlude();

    let plan = authority
        .mutation()
        .expect("a mutation authority carries a mutation plan");
    match namespace::apply(&located, plan) {
        Ok(()) => {
            let summary = describe(plan);
            ToolResult::success(summary.clone(), summary)
        }
        Err(namespace::ApplyError::Stale(reason)) => ToolResult::revoked(format!(
            "{tool} stopped: the authority for `{}` no longer describes the filesystem -- {reason}",
            plan.display()
        )),
        Err(namespace::ApplyError::Failed(reason)) => {
            ToolResult::failure(format!("{tool} failed: {reason}"))
        }
    }
}

/// Records a completed read so a later mutation can rest on it.
///
/// Called by `read_file`; kept here so the proof's producer and its consumer are
/// written next to each other.
pub(crate) fn record_read(
    context: &ToolContext,
    path: &std::path::Path,
    metadata: &std::fs::Metadata,
    bytes: &[u8],
    complete_view: bool,
) {
    context.reads().record(
        path.to_path_buf(),
        crate::permission::ReadRecord {
            identity: crate::permission::FileIdentity::from_metadata(metadata),
            hash: ContentHash::of(bytes),
            complete_view,
        },
    );
}

// ---------------------------------------------------------------------------
// the namespace boundary
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod namespace {
    //! Component-by-component traversal from an authorized root descriptor.
    //!
    //! Every `openat` carries `O_NOFOLLOW`, and the parent descriptor stays open
    //! from the moment the preimage is read until the `renameat` that commits
    //! the change. A name is therefore resolved exactly once, and re-pointing it
    //! afterwards cannot redirect anything: the descriptor still refers to the
    //! directory that was checked.
    //!
    //! # Residual
    //!
    //! Two windows remain, and neither is closed by anything POSIX offers:
    //!
    //! 1. The authorized root itself is opened *by path*. A root replaced
    //!    between `AccessScope` construction and this open would be followed.
    //!    The root is the directory the user pointed xfx at, so this is the same
    //!    trust the invocation already rests on.
    //! 2. Between the final identity/digest revalidation and `renameat` there is
    //!    a window in which the target could be replaced again. `renameat` is
    //!    atomic with respect to the name, so the result is never a partial
    //!    file, but a sufficiently fast writer can still lose an update. Closing
    //!    it would need a lock the filesystem does not provide.
    //!
    //! This is narrower than the read path's residual, not a claim of race
    //! freedom.

    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io::Write;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Component, Path, PathBuf};

    use rustix::fs::{Mode, OFlags};
    use rustix::io::Errno;

    use crate::permission::{FileIdentity, MutationKind, MutationPlan, Preimage, TargetScope};
    use crate::workspace::AccessScope;

    /// Permissions for a directory xfx creates, before the umask. `Mode`'s
    /// underlying integer type differs by platform, so the bits are named rather
    /// than written as an octal literal.
    fn new_directory_mode() -> Mode {
        Mode::RWXU | Mode::RGRP | Mode::XGRP | Mode::ROTH | Mode::XOTH
    }

    /// Permissions for a file xfx creates, before the umask.
    const NEW_FILE_MODE: u32 = 0o644;

    /// A target resolved down to a pinned parent directory and a final name.
    pub struct Location {
        /// The deepest directory on the path that already exists.
        parent: OwnedFd,
        /// Directories between `parent` and the final name that do not exist
        /// yet, in order. Creating them is part of the change, not of resolving
        /// it, so nothing here has been created.
        missing: Vec<OsString>,
        name: OsString,
        full: PathBuf,
        display: String,
        target_scope: TargetScope,
    }

    impl Location {
        /// The absolute path this location names.
        pub fn full(&self) -> &Path {
            &self.full
        }

        /// How the path is shown to the model.
        pub fn display(&self) -> &str {
            &self.display
        }

        pub fn target_scope(&self) -> TargetScope {
            self.target_scope
        }

        /// Whether the target can possibly exist yet.
        fn reachable(&self) -> bool {
            self.missing.is_empty()
        }

        fn parent(&self) -> BorrowedFd<'_> {
            self.parent.as_fd()
        }
    }

    /// Why applying a plan stopped.
    pub enum ApplyError {
        /// The world moved: the authority is no longer about this filesystem.
        Stale(String),
        /// Something else went wrong, and retrying could work.
        Failed(String),
    }

    /// Resolves `requested` to a pinned parent and a final name, creating nothing.
    pub fn locate(scope: &AccessScope, requested: &str, tool: &str) -> Result<Location, String> {
        let trimmed = requested.trim();
        if trimmed.is_empty() {
            return Err(format!(
                "{tool} refused the path: the path must not be empty"
            ));
        }
        let candidate = Path::new(trimmed);
        let anchored = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            scope.primary().join(candidate)
        };

        // `..` is refused rather than normalized. Normalizing it lexically would
        // be wrong through a symlink, and resolving it for real would mean
        // walking the path twice with two different answers.
        let mut components: Vec<OsString> = Vec::new();
        for component in anchored.components() {
            match component {
                Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
                Component::ParentDir => {
                    return Err(format!(
                        "{tool} refused the path: `{trimmed}` contains a `..` component; name the path from the workspace root"
                    ))
                }
                Component::Normal(name) => components.push(name.to_os_string()),
            }
        }
        if components.is_empty() {
            return Err(format!(
                "{tool} refused the path: `{trimmed}` names a root directory rather than a file"
            ));
        }

        // Rebuild the path from the components xfx accepted, so what is walked
        // and what is reported are the same string.
        let mut full = PathBuf::from("/");
        for component in &components {
            full.push(component);
        }

        let Some(root) = containing_root(scope, &full) else {
            return Err(format!(
                "{tool} refused the path: `{trimmed}` resolves outside the authorized workspace roots"
            ));
        };
        let target_scope = if root == scope.primary() {
            TargetScope::PrimaryWorkspace
        } else {
            TargetScope::AdditionalRoot
        };
        let relative: Vec<OsString> = full
            .strip_prefix(root)
            .map_err(|_| format!("{tool} refused the path: `{trimmed}` is not inside its root"))?
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_os_string()),
                _ => None,
            })
            .collect();
        if relative.is_empty() {
            return Err(format!(
                "{tool} refused the path: `{trimmed}` is an authorized root, not something to change"
            ));
        }

        // Before policy, in every mode: the file tools do not rewrite the
        // metadata that decides what xfx and git are allowed to do. This is
        // structural rather than a rule, so no mode, rule, or standing approval
        // can reach past it.
        if let Some(protected) = protected_component(&relative) {
            return Err(format!(
                "{tool} refused the path: `{trimmed}` passes through `{protected}`, which holds repository or xfx metadata; \
                 the file tools never change it in any permission mode, because a `{protected}` entry decides what later commands are allowed to run"
            ));
        }

        // The root is authorized and canonical, so it is opened by path. Every
        // component below it is opened relative to a descriptor.
        let mut parent = rustix::fs::open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|err| format!("{tool} cannot open the workspace root: {err}"))?;

        let last = relative.len() - 1;
        let mut missing: Vec<OsString> = Vec::new();
        for component in &relative[..last] {
            if !missing.is_empty() {
                missing.push(component.clone());
                continue;
            }
            match open_directory(parent.as_fd(), component.as_os_str()) {
                Ok(fd) => parent = fd,
                Err(Errno::NOENT) => missing.push(component.clone()),
                Err(err) => {
                    return Err(format!(
                        "{tool} {}",
                        describe(parent.as_fd(), component.as_os_str(), err)
                    ))
                }
            }
        }

        Ok(Location {
            parent,
            missing,
            name: relative[last].clone(),
            display: scope.display_path(&full),
            full,
            target_scope,
        })
    }

    /// The first component of `relative` that names write-protected metadata.
    ///
    /// Every component is checked, including the last: `create_folder .git` is
    /// as much a change to that authority as `write_file .git/config` is.
    fn protected_component(relative: &[OsString]) -> Option<String> {
        relative.iter().find_map(|component| {
            let name = component.to_string_lossy();
            crate::workspace::is_protected_write_directory(&name).then(|| name.into_owned())
        })
    }

    /// The authorized root that lexically contains `path`.
    fn containing_root<'a>(scope: &'a AccessScope, path: &Path) -> Option<&'a Path> {
        scope
            .roots()
            .find(|root| path == *root || path.starts_with(*root))
    }

    fn open_directory(parent: BorrowedFd<'_>, name: &OsStr) -> Result<OwnedFd, Errno> {
        rustix::fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
    }

    /// Turns a failed `openat` into a sentence that says what xfx found.
    ///
    /// The entry is inspected rather than the errno translated, because the two
    /// platforms xfx targets disagree: `openat(..., O_DIRECTORY | O_NOFOLLOW)`
    /// on a symbolic link reports `ELOOP` on Linux and `ENOTDIR` on macOS. A
    /// message derived from the errno alone would therefore call the same link
    /// two different things depending on where xfx was built.
    fn describe(parent: BorrowedFd<'_>, component: &OsStr, err: Errno) -> String {
        let name = component.to_string_lossy();
        match err {
            Errno::LOOP | Errno::NOTDIR => {
                if is_symlink(parent, component) {
                    format!("refused the path: `{name}` is a symbolic link")
                } else if err == Errno::NOTDIR {
                    format!("refused the path: `{name}` is not a directory")
                } else {
                    format!("refused the path: `{name}` has too many levels of symbolic links")
                }
            }
            Errno::NOENT => format!("refused the path: no such path: `{name}`"),
            Errno::ACCESS | Errno::PERM => {
                format!("refused the path: `{name}` cannot be opened: permission denied")
            }
            other => format!("refused the path: `{name}` cannot be opened: {other}"),
        }
    }

    /// Whether the entry named `component` in `parent` is a symbolic link.
    fn is_symlink(parent: BorrowedFd<'_>, component: &OsStr) -> bool {
        rustix::fs::statat(parent, component, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
            .map(|stat| {
                rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Symlink
            })
            .unwrap_or(false)
    }

    /// Reads the target without following a link, or reports that it is absent.
    ///
    /// `max_bytes` is checked against the *stat*, before a byte is allocated. A
    /// mutation has to hold the whole preimage in memory -- it is hashed, and an
    /// edit builds its postimage from it -- so an unbounded read here would let
    /// one `edit_file` on a large file exhaust xfx's memory before policy had
    /// even been asked. The bound is the same ceiling a complete `read_file`
    /// runs under, which is not a coincidence: an existing target needs a
    /// complete read proof, and a file above that ceiling can never have one, so
    /// this refuses at the top of the path what would have been refused at the
    /// bottom of it anyway.
    pub fn read_target(
        located: &Location,
        max_bytes: usize,
    ) -> Result<Option<(FileIdentity, Vec<u8>)>, String> {
        if !located.reachable() {
            return Ok(None);
        }
        let file = match rustix::fs::openat(
            located.parent(),
            located.name.as_os_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => File::from(fd),
            Err(Errno::NOENT) => return Ok(None),
            Err(err) => return Err(describe(located.parent(), located.name.as_os_str(), err)),
        };
        let metadata = file
            .metadata()
            .map_err(|err| format!("cannot inspect `{}`: {err}", located.display()))?;
        if metadata.is_dir() {
            return Err(format!(
                "refused the path: `{}` is a directory",
                located.display()
            ));
        }
        if metadata.len() > max_bytes as u64 {
            return Err(format!(
                "will not change `{}`: it is {} bytes and one change may only rest on a preimage of at most {max_bytes} bytes",
                located.display(),
                metadata.len()
            ));
        }
        // Exactly the size that was just admitted, so the read cannot grow past
        // the bound even if the file does between the stat and the read.
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let mut file = std::io::Read::take(file, max_bytes as u64);
        std::io::Read::read_to_end(&mut file, &mut bytes)
            .map_err(|err| format!("cannot read `{}`: {err}", located.display()))?;
        Ok(Some((FileIdentity::from_metadata(&metadata), bytes)))
    }

    /// Whether the final component is already a directory.
    pub fn directory_exists(located: &Location) -> Result<bool, String> {
        if !located.reachable() {
            return Ok(false);
        }
        match open_directory(located.parent(), located.name.as_os_str()) {
            Ok(_) => Ok(true),
            Err(Errno::NOENT) => Ok(false),
            Err(err) => Err(describe(located.parent(), located.name.as_os_str(), err)),
        }
    }

    /// Applies an authorized plan: revalidate, stage, revalidate, rename, sync.
    pub fn apply(located: &Location, plan: &MutationPlan) -> Result<(), ApplyError> {
        // Missing parents are created here rather than during resolution,
        // because creating them is part of the change the user authorized.
        let mut created: Option<OwnedFd> = None;
        for component in &located.missing {
            let current = created.as_ref().map_or(located.parent(), |fd| fd.as_fd());
            match rustix::fs::mkdirat(current, component.as_os_str(), new_directory_mode()) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(err) => {
                    return Err(ApplyError::Failed(describe(
                        current,
                        component.as_os_str(),
                        err,
                    )))
                }
            }
            match open_directory(current, component.as_os_str()) {
                Ok(fd) => created = Some(fd),
                Err(err) => {
                    return Err(ApplyError::Stale(describe(
                        current,
                        component.as_os_str(),
                        err,
                    )))
                }
            }
        }
        let parent = created.as_ref().map_or(located.parent(), |fd| fd.as_fd());

        if plan.kind() == MutationKind::CreateFolder {
            return match rustix::fs::mkdirat(parent, located.name.as_os_str(), new_directory_mode())
            {
                Ok(()) => sync_directory(parent),
                Err(Errno::EXIST) => Err(ApplyError::Stale(
                    "something was created at that path after the change was authorized"
                        .to_string(),
                )),
                Err(err) => Err(ApplyError::Failed(describe(
                    parent,
                    located.name.as_os_str(),
                    err,
                ))),
            };
        }

        let mode = revalidate(parent, located, plan)?;
        let staged = stage(parent, plan.staged_bytes(), mode)?;
        // Checked again with the replacement already on disk, so the window
        // between the last look and the rename is as small as it can be.
        revalidate(parent, located, plan)?;

        rustix::fs::renameat(
            parent,
            staged.name.as_os_str(),
            parent,
            located.name.as_os_str(),
        )
        .map_err(|err| ApplyError::Failed(format!("cannot replace the file: {err}")))?;
        staged.keep();
        sync_directory(parent)
    }

    /// Confirms the target still is what the plan says it was.
    ///
    /// Returns the permission bits to give the replacement: the target's own, so
    /// an executable script stays executable.
    fn revalidate(
        parent: BorrowedFd<'_>,
        located: &Location,
        plan: &MutationPlan,
    ) -> Result<u32, ApplyError> {
        let opened = rustix::fs::openat(
            parent,
            located.name.as_os_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        );
        match (plan.preimage(), opened) {
            (Preimage::Absent, Err(Errno::NOENT)) => Ok(NEW_FILE_MODE),
            (Preimage::Absent, Ok(_)) => Err(ApplyError::Stale(
                "the file did not exist when the change was authorized and exists now".to_string(),
            )),
            (Preimage::Present { .. }, Err(Errno::NOENT)) => Err(ApplyError::Stale(
                "the file existed when the change was authorized and is gone now".to_string(),
            )),
            (_, Err(err)) => Err(ApplyError::Stale(describe(
                parent,
                located.name.as_os_str(),
                err,
            ))),
            (Preimage::Present { identity, hash }, Ok(fd)) => {
                let file = File::from(fd);
                let metadata = file
                    .metadata()
                    .map_err(|err| ApplyError::Failed(format!("cannot inspect the file: {err}")))?;
                let current = FileIdentity::from_metadata(&metadata);
                if !current.matches(identity) {
                    return Err(ApplyError::Stale(
                        "the file's identity changed after the change was authorized".to_string(),
                    ));
                }
                let mut bytes = Vec::new();
                let mut file = file;
                std::io::Read::read_to_end(&mut file, &mut bytes)
                    .map_err(|err| ApplyError::Failed(format!("cannot re-read the file: {err}")))?;
                if crate::permission::ContentHash::of(&bytes) != *hash {
                    return Err(ApplyError::Stale(
                        "the file's contents changed after the change was authorized".to_string(),
                    ));
                }
                Ok(current.mode)
            }
        }
    }

    /// A file being written next to its target, removed unless it is kept.
    struct Staged<'a> {
        parent: BorrowedFd<'a>,
        name: OsString,
        committed: bool,
    }

    impl Staged<'_> {
        fn keep(mut self) {
            self.committed = true;
        }
    }

    impl Drop for Staged<'_> {
        /// A staging file that was never renamed is litter in the user's
        /// workspace, so every failure path removes it without having to
        /// remember to.
        fn drop(&mut self) {
            if !self.committed {
                let _ = rustix::fs::unlinkat(
                    self.parent,
                    self.name.as_os_str(),
                    rustix::fs::AtFlags::empty(),
                );
            }
        }
    }

    /// Writes `bytes` to a fresh file in the target's own directory.
    ///
    /// Same directory, because `rename` is only atomic within a filesystem, and
    /// a temporary directory can be on a different one.
    fn stage<'a>(
        parent: BorrowedFd<'a>,
        bytes: &[u8],
        mode: u32,
    ) -> Result<Staged<'a>, ApplyError> {
        let name = OsString::from(format!(
            ".xfx-stage-{}-{}",
            std::process::id(),
            STAGE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        // `O_EXCL` so a staging name that somehow already exists is an error
        // rather than something xfx silently overwrites.
        let fd = rustix::fs::openat(
            parent,
            name.as_os_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|err| ApplyError::Failed(format!("cannot stage the replacement: {err}")))?;
        let staged = Staged {
            parent,
            name,
            committed: false,
        };

        let mut file = File::from(fd);
        file.write_all(bytes)
            .map_err(|err| ApplyError::Failed(format!("cannot write the replacement: {err}")))?;
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .map_err(|err| ApplyError::Failed(format!("cannot set permissions: {err}")))?;
        // The bytes reach the disk before the name does, so a crash between the
        // two leaves the old file rather than an empty new one.
        file.sync_all()
            .map_err(|err| ApplyError::Failed(format!("cannot flush the replacement: {err}")))?;
        Ok(staged)
    }

    /// Makes the directory entry itself durable.
    fn sync_directory(parent: BorrowedFd<'_>) -> Result<(), ApplyError> {
        rustix::fs::fsync(parent)
            .map_err(|err| ApplyError::Failed(format!("cannot flush the directory: {err}")))
    }

    static STAGE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}

#[cfg(not(unix))]
mod namespace {
    //! The mutation path needs `openat`, `renameat`, and `O_NOFOLLOW` to be safe
    //! against a path component being replaced mid-operation. Those are POSIX
    //! calls with no portable equivalent here, and a version without them would
    //! be a weaker guarantee wearing the same name. So the tools decode,
    //! validate, and refuse, and `docs/parity.md` records the platform.

    use std::path::Path;

    use crate::permission::{FileIdentity, MutationPlan, TargetScope};
    use crate::workspace::AccessScope;

    const UNSUPPORTED: &str =
        "refused the path: xfx changes files only on Unix, where a target can be opened relative to a verified directory";

    pub struct Location {
        full: std::path::PathBuf,
        display: String,
    }

    impl Location {
        pub fn full(&self) -> &Path {
            &self.full
        }

        pub fn display(&self) -> &str {
            &self.display
        }

        pub fn target_scope(&self) -> TargetScope {
            TargetScope::PrimaryWorkspace
        }
    }

    pub enum ApplyError {
        Stale(String),
        Failed(String),
    }

    pub fn locate(_scope: &AccessScope, _requested: &str, tool: &str) -> Result<Location, String> {
        Err(format!("{tool} {UNSUPPORTED}"))
    }

    pub fn read_target(
        _located: &Location,
        _max_bytes: usize,
    ) -> Result<Option<(FileIdentity, Vec<u8>)>, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn directory_exists(_located: &Location) -> Result<bool, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn apply(_located: &Location, _plan: &MutationPlan) -> Result<(), ApplyError> {
        Err(ApplyError::Failed(UNSUPPORTED.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_decoder_names_the_field_it_wanted() {
        assert_eq!(
            decode_write_file(&json!({ "path": "a" })).unwrap_err(),
            "write_file requires the string field `content`"
        );
        assert_eq!(
            decode_edit_file(&json!({ "path": "a", "old_string": "b" })).unwrap_err(),
            "edit_file requires the string field `new_string`"
        );
        assert_eq!(
            decode_create_folder(&json!({})).unwrap_err(),
            "create_folder requires the string field `path`"
        );
    }

    #[test]
    fn an_empty_old_string_is_refused_because_it_matches_everywhere() {
        let input = decode_edit_file(&json!({
            "path": "a.txt",
            "old_string": "",
            "new_string": "x",
        }))
        .expect("decodes");
        assert_eq!(
            validate_edit_file(&input).unwrap_err(),
            "edit_file field `old_string` must not be empty"
        );
    }

    #[test]
    fn a_blank_path_is_refused_by_every_mutating_tool() {
        for input in [
            decode_write_file(&json!({ "path": "  ", "content": "x" })).unwrap(),
            decode_edit_file(&json!({ "path": " ", "old_string": "a", "new_string": "b" }))
                .unwrap(),
            decode_create_folder(&json!({ "path": "\t" })).unwrap(),
        ] {
            let spec = match &input {
                ToolInput::WriteFile(_) => WRITE_FILE,
                ToolInput::EditFile(_) => EDIT_FILE,
                _ => CREATE_FOLDER,
            };
            let message = match &input {
                ToolInput::WriteFile(_) => validate_write_file(&input),
                ToolInput::EditFile(_) => validate_edit_file(&input),
                _ => validate_create_folder(&input),
            }
            .unwrap_err();
            assert!(message.contains(spec.name()), "{message}");
            assert!(message.contains("must not be empty"), "{message}");
        }
    }

    #[test]
    fn every_mutating_spec_declares_that_it_mutates() {
        for spec in [WRITE_FILE, EDIT_FILE, CREATE_FOLDER] {
            assert_eq!(spec.permission(), PermissionKind::MutateFile);
            assert!(spec.permission().requires_authority());
        }
    }

    #[test]
    fn the_path_description_states_the_rules_the_tools_actually_enforce() {
        for disclosure in ["symbolic links", "`..`", "workspace root"] {
            assert!(
                PATH_DESCRIPTION.contains(disclosure),
                "the path description omits {disclosure}: {PATH_DESCRIPTION}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // what the question a mutation asks carries
    // -----------------------------------------------------------------------

    /// A prompter that refuses everything and keeps every question it was asked.
    ///
    /// Refusing rather than allowing keeps these cases about the *question*: a
    /// tool that went on to write would make the assertion depend on the
    /// filesystem as well as on the approval channel.
    #[derive(Clone, Default)]
    struct Recorder(std::sync::Arc<std::sync::Mutex<Vec<crate::permission::ApprovalRequest>>>);

    impl Recorder {
        fn last(&self) -> crate::permission::ApprovalRequest {
            self.0
                .lock()
                .expect("the log")
                .last()
                .cloned()
                .expect("the tool asked")
        }
    }

    impl crate::permission::ApprovalPrompter for Recorder {
        fn request(
            &mut self,
            request: &crate::permission::ApprovalRequest,
        ) -> std::io::Result<crate::permission::ApprovalAnswer> {
            self.0.lock().expect("the log").push(request.clone());
            Ok(crate::permission::ApprovalAnswer::Deny)
        }
    }

    /// A workspace holding `notes.txt`, with the read proof a mutation needs,
    /// and a context that asks `recorder` before it changes anything.
    fn asking(contents: &str) -> (tempfile::TempDir, ToolContext, Recorder) {
        let dir = tempfile::tempdir().expect("temporary workspace");
        std::fs::write(dir.path().join("notes.txt"), contents).expect("the fixture file");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("scope");
        // The scope's own root, not the temporary directory's: a proof is keyed
        // by the canonical path the executor resolves to, and on macOS the two
        // differ by a `/private` prefix.
        let path = scope.primary().join("notes.txt");
        let recorder = Recorder::default();
        let context = ToolContext::new(scope).with_permissions(
            crate::permission::PermissionSession::new(crate::permission::PermissionMode::Ask)
                .with_prompter(Box::new(recorder.clone())),
        );
        let metadata = std::fs::metadata(&path).expect("the fixture's metadata");
        record_read(&context, &path, &metadata, contents.as_bytes(), true);
        (dir, context, recorder)
    }

    #[test]
    fn an_edit_asks_with_the_exact_bounded_before_and_after_it_would_write() {
        // The whole risk of an edit is which bytes leave and which arrive, and
        // the 160-byte summary can only quote the beginning of each. The screen
        // review gets the exact pair, bounded once at the permission boundary
        // rather than trusted to whatever renders it.
        let (_dir, context, recorder) = asking("alpha and more\n");
        let input = decode_edit_file(&json!({
            "path": "notes.txt",
            "old_string": "alpha",
            "new_string": "beta\ngamma",
        }))
        .expect("decodes");

        let result = execute_edit_file(&input, &context);
        assert!(!result.ok, "the fixture prompter denies: {}", result.output);

        let diff = recorder.last().diff.expect("an edit carries its diff");
        assert_eq!(diff.before, "alpha");
        assert_eq!(diff.after, "beta\\ngamma");
    }

    #[test]
    fn an_edit_larger_than_the_bound_is_cut_before_it_leaves_the_permission_boundary() {
        // One bound, at the boundary that builds the question. A tool that
        // handed the whole `old_string` on and left the cut to the renderer
        // would put four megabytes -- `ToolLimits::max_mutation_bytes` -- on a
        // channel sized for two sides of 64 KiB.
        let contents = format!("{}\n", "a".repeat(100_000));
        let (_dir, context, recorder) = asking(&contents);
        let input = decode_edit_file(&json!({
            "path": "notes.txt",
            "old_string": "a".repeat(100_000),
            "new_string": "b".repeat(100_000),
        }))
        .expect("decodes");

        let result = execute_edit_file(&input, &context);
        assert!(!result.ok, "the fixture prompter denies: {}", result.output);

        let diff = recorder.last().diff.expect("an edit carries its diff");
        assert_eq!(diff.before.len(), 65_536);
        assert_eq!(diff.after.len(), 65_536);
        assert!(diff.before.ends_with('\u{2026}'));
        assert!(diff.after.ends_with('\u{2026}'));
    }

    #[test]
    fn a_write_and_a_create_have_no_before_and_after_to_review_so_the_question_stays_in_the_band() {
        // `write_file` replaces everything, so its "before" is the whole
        // previous file and the honest summary of it is the digest the prompt
        // already names; `create_folder` changes no content at all. Neither has
        // a pair a diff screen could show, so neither asks for one.
        let (_dir, context, recorder) = asking("alpha\n");
        let write = decode_write_file(&json!({ "path": "notes.txt", "content": "beta\n" }))
            .expect("decodes");
        let written = execute_write_file(&write, &context);
        assert!(
            !written.ok,
            "the fixture prompter denies: {}",
            written.output
        );
        assert!(
            recorder.last().diff.is_none(),
            "a write invented a diff the tool cannot honestly produce"
        );

        let create = decode_create_folder(&json!({ "path": "made" })).expect("decodes");
        let created = execute_create_folder(&create, &context);
        assert!(
            !created.ok,
            "the fixture prompter denies: {}",
            created.output
        );
        assert!(recorder.last().diff.is_none(), "a create_folder had a diff");
    }
}
