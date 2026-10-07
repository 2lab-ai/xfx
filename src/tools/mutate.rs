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
    bounded_excerpt, ApprovalDiff, ContentHash, Decided, MutationExcerpt, MutationKind,
    MutationPlan, PolicyDecision, Preimage, ProposedAction,
};

use super::undo::{Mark, Taken, TrackedChange, Undone};

use super::spec::{
    nonblank, object, required_string, Activity, InputSchema, LineChanges, Outcome, PermissionKind,
    Presentation, Property, PropertyKind, ToolContext, ToolInput, ToolResult, ToolSpec,
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
    Presentation::new(Activity::Write, "Writing", "Wrote", "file"),
    InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: PATH_DESCRIPTION,
                allowed: &[],
                array: None,
            },
            Property {
                name: "content",
                kind: PropertyKind::String,
                description: "The complete new contents of the file.",
                allowed: &[],
                array: None,
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
    Presentation::new(Activity::Edit, "Editing", "Edited", "file"),
    InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: PATH_DESCRIPTION,
                allowed: &[],
                array: None,
            },
            Property {
                name: "old_string",
                kind: PropertyKind::String,
                description: "The exact text to replace. Must occur exactly once in the file.",
                allowed: &[],
                array: None,
            },
            Property {
                name: "new_string",
                kind: PropertyKind::String,
                description: "The text to put in its place.",
                allowed: &[],
                array: None,
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
    Presentation::new(Activity::Write, "Creating", "Created", "folder"),
    InputSchema {
        properties: &[Property {
            name: "path",
            kind: PropertyKind::String,
            description: PATH_DESCRIPTION,
            allowed: &[],
            array: None,
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
    let Prepared {
        located,
        plan,
        changes,
        before,
    } = match prepare_write_file(input, context, Proof::Required) {
        Ok(prepared) => prepared,
        Err(result) => return result,
    };
    commit(context, located, plan, changes, before, |plan| {
        format!(
            "Wrote {} ({} bytes)",
            plan.display(),
            plan.staged_bytes().len()
        )
    })
}

/// A change resolved, read and planned, and not made: what policy judges and
/// what `/permissions remember` keys a rule on.
pub(crate) struct Prepared {
    located: Location,
    pub(crate) plan: MutationPlan,
    changes: Option<LineChanges>,
    /// The bytes the change replaces, for `/undo` ([`commit`]).
    before: Option<Vec<u8>>,
}

/// Whether preparing a change requires the session's read proof.
///
/// A call the model makes does: an existing file it has not read in full is
/// one whose contents it would discard unseen. A rule a person is making does
/// not -- the proof is about what the model knows, not about what may run,
/// and upstream's preparation has no such step (`preparePermissionStateAction`,
/// `tool_admission.zig:2257-2302`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Proof {
    Required,
    NotRequired,
}

/// `write_file`, up to the decision.
pub(crate) fn prepare_write_file(
    input: &WriteFileInput,
    context: &ToolContext,
    proof: Proof,
) -> Result<Prepared, ToolResult> {
    let bytes = input.content.as_bytes();
    bounded("write_file", "content", bytes.len(), context).map_err(ToolResult::failure)?;

    let located = namespace::locate(context.scope(), &input.path, "write_file")
        .map_err(ToolResult::failure)?;
    let preimage = read_preimage(&located, "write_file", context).map_err(ToolResult::failure)?;
    if proof == Proof::Required {
        if let Some(reason) = read_proof_missing("write_file", context, &located, &preimage) {
            return Err(ToolResult::failure(reason));
        }
    }
    // What the row under the turn counts (`Wrote c.txt +2`). Only for a
    // preimage that is text: lines of bytes that are not UTF-8 are not lines,
    // and a count made of them would be a number about nothing.
    let changes = match &preimage {
        TargetState::Absent => Some(LineChanges::between("", &input.content)),
        TargetState::Present { bytes, .. } => std::str::from_utf8(bytes)
            .ok()
            .map(|before| LineChanges::between(before, &input.content)),
    };

    // The pair a screen review shows, built **here**, from the preimage this
    // executor read and the exact text that would replace it -- the same two
    // things the staged bytes and the fingerprint are built from, rather than a
    // second reading of the file by whatever renders the question. A whole-file
    // write is the largest change this product makes; the band's 160-byte
    // summary can only quote the beginning of what would arrive and says
    // nothing at all about what would leave.
    //
    // Bounded and escaped at this boundary, like an edit's, because the
    // preimage may be a quarter of a megabyte and the content four
    // (`ToolLimits`), and nothing downstream may be handed either whole.
    let plan = MutationPlan::new(
        MutationKind::Write,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        preimage.summary(),
        bytes.to_vec(),
    )
    .with_rule_targets(located.rule_targets())
    .with_state_binding(located.state_binding(&[input.path.as_bytes(), input.content.as_bytes()]))
    .with_diff(ApprovalDiff::around_change(
        &preimage.as_text(),
        &input.content,
    ));
    Ok(Prepared {
        located,
        plan,
        changes,
        before: preimage.into_before(),
    })
}

fn execute_edit_file(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::EditFile(input) = input else {
        return ToolResult::failure(mismatched("edit_file"));
    };
    let Prepared {
        located,
        plan,
        changes,
        before,
    } = match prepare_edit_file(input, context, Proof::Required) {
        Ok(prepared) => prepared,
        Err(result) => return result,
    };
    commit(context, located, plan, changes, before, |plan| {
        format!("Edited {}", plan.display())
    })
}

/// `edit_file`, up to the decision.
pub(crate) fn prepare_edit_file(
    input: &EditFileInput,
    context: &ToolContext,
    proof: Proof,
) -> Result<Prepared, ToolResult> {
    for (field, value) in [
        ("old_string", &input.old_string),
        ("new_string", &input.new_string),
    ] {
        bounded("edit_file", field, value.len(), context).map_err(ToolResult::failure)?;
    }

    let located = namespace::locate(context.scope(), &input.path, "edit_file")
        .map_err(ToolResult::failure)?;
    let preimage = read_preimage(&located, "edit_file", context).map_err(ToolResult::failure)?;
    let TargetState::Present { bytes, .. } = &preimage else {
        return Err(ToolResult::failure(format!(
            "edit_file cannot edit a file that does not exist: no such path: `{}`",
            input.path
        )));
    };
    if proof == Proof::Required {
        if let Some(reason) = read_proof_missing("edit_file", context, &located, &preimage) {
            return Err(ToolResult::failure(reason));
        }
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(ToolResult::failure(format!(
            "edit_file cannot edit `{}`: it is not UTF-8 text",
            located.display()
        )));
    };
    let occurrences = text.matches(input.old_string.as_str()).count();
    match occurrences {
        0 => {
            return Err(ToolResult::failure(format!(
                "edit_file made no change: old_string was not found in `{}`",
                located.display()
            )))
        }
        1 => {}
        many => {
            return Err(ToolResult::failure(format!(
                "edit_file made no change: old_string appears {many} times in `{}`; include enough surrounding text for exactly one match",
                located.display()
            )))
        }
    }
    let after = text.replacen(input.old_string.as_str(), &input.new_string, 1);
    bounded("edit_file", "result", after.len(), context).map_err(ToolResult::failure)?;
    // Counted over the whole file, as upstream's review diff is, rather than
    // over the two strings: an `old_string` that ends mid-line changes the
    // line it ends on, and only the file says which one that is.
    let changes = Some(LineChanges::between(text, &after));

    // The excerpt is what an approval prompt shows. It is built from the exact
    // strings the model sent, bounded and escaped, so a human is asked about the
    // change rather than about the file's name.
    //
    // The diff beside it is the part of the **file** the change touches, before
    // and after, bounded far wider, for a review that numbers the lines it
    // shows: upstream's review is a diff of the file with the unchanged lines
    // around the change (`vercel-labs/fx@c1db919 src/core/output/diff.zig:497-640`),
    // and the strings the model sent carry neither a line number nor a
    // neighbour (`ApprovalDiff::around_change` says why a window). Both
    // are built **here**, from the text that produced the staged bytes, rather
    // than reconstructed by whatever renders the question: a payload rebuilt at
    // the surface would be a second reading of the change, and only one of the
    // two would be the one being authorized. The bound is applied at this
    // boundary for the same reason -- `max_mutation_bytes` is four megabytes,
    // and nothing downstream may be handed that.
    let diff = ApprovalDiff::around_change(text, &after);
    let plan = MutationPlan::new(
        MutationKind::Edit,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        preimage.summary(),
        after.into_bytes(),
    )
    .with_rule_targets(located.rule_targets())
    .with_state_binding(located.state_binding(&[
        input.path.as_bytes(),
        input.old_string.as_bytes(),
        input.new_string.as_bytes(),
    ]))
    .with_excerpt(MutationExcerpt {
        before: bounded_excerpt(&input.old_string),
        after: bounded_excerpt(&input.new_string),
    })
    .with_diff(diff);
    Ok(Prepared {
        located,
        plan,
        changes,
        before: preimage.into_before(),
    })
}

fn execute_create_folder(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::CreateFolder(input) = input else {
        return ToolResult::failure(mismatched("create_folder"));
    };
    let Prepared {
        located,
        plan,
        changes,
        before,
    } = match prepare_create_folder(input, context) {
        Ok(prepared) => prepared,
        Err(result) => return result,
    };
    commit(context, located, plan, changes, before, |plan| {
        format!("Created {}", plan.display())
    })
}

/// `create_folder`, up to the decision. A directory that is already there is
/// answered here, as a success with nothing to decide: creating nothing needs
/// no authority.
pub(crate) fn prepare_create_folder(
    input: &CreateFolderInput,
    context: &ToolContext,
) -> Result<Prepared, ToolResult> {
    let located = namespace::locate(context.scope(), &input.path, "create_folder")
        .map_err(ToolResult::failure)?;

    // Checking whether the directory is already there is a read, so it happens
    // before any decision: creating nothing needs no authority.
    match namespace::directory_exists(&located) {
        Ok(true) => {
            return Err(ToolResult::success(
                format!("{} already exists", located.display()),
                format!("{} already exists", located.display()),
            ))
        }
        Ok(false) => {}
        Err(reason) => return Err(ToolResult::failure(format!("create_folder {reason}"))),
    }

    let plan = MutationPlan::new(
        MutationKind::CreateFolder,
        located.full().to_path_buf(),
        located.display().to_string(),
        located.target_scope(),
        Preimage::Absent,
        Vec::new(),
    )
    .with_rule_targets(located.rule_targets())
    .with_state_binding(located.state_binding(&[input.path.as_bytes()]));
    Ok(Prepared {
        located,
        plan,
        changes: None,
        // No preimage: a folder is not something `/undo` takes back
        // ([`commit`] tracks file writes and edits only).
        before: None,
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
    /// What is at the target now, as a review surface may show it.
    ///
    /// Three answers, and each is a fact rather than a rendering:
    ///
    /// * nothing is there -- the empty string, which the review screen says as
    ///   `(nothing)`. A fabricated before would be a diff against a file that
    ///   does not exist;
    /// * text -- the text, which the permission boundary then escapes and
    ///   bounds like any other side ([`crate::permission::ApprovalDiff::of`]);
    /// * bytes that are not UTF-8 -- what they are and how many
    ///   ([`crate::permission::non_text_summary`]), because they have no text
    ///   form: shown lossily they would read as a file whose contents are
    ///   replacement characters, and a reviewer would be approving a diff
    ///   against something that is not there.
    fn as_text(&self) -> String {
        match self {
            Self::Absent => String::new(),
            Self::Present { bytes, .. } => match std::str::from_utf8(bytes) {
                Ok(text) => text.to_string(),
                Err(_) => crate::permission::non_text_summary(bytes.len()),
            },
        }
    }

    fn summary(&self) -> Preimage {
        match self {
            Self::Absent => Preimage::Absent,
            Self::Present { identity, hash, .. } => Preimage::Present {
                identity: *identity,
                hash: *hash,
            },
        }
    }

    /// What `/undo` would put back: the bytes, or `None` for a target that
    /// did not exist ([`super::undo::TrackedChange::before`]).
    fn into_before(self) -> Option<Vec<u8>> {
        match self {
            Self::Absent => None,
            Self::Present { bytes, .. } => Some(bytes),
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
///
/// `before` is what was at the target, for `/undo` ([`undo_last`]): the bytes
/// the preimage was read as, or `None` for nothing. Read by a file write or
/// edit that **landed**, and by nothing else -- a refused, failed or empty
/// change changed nothing to take back, and a folder is not tracked, as
/// upstream tracks `write_file` and `edit_file` alone
/// (`publishCommittedTracker`, `vercel-labs/fx@c1db9191
/// src/core/app/app_callbacks.zig:1125-1170`).
fn commit(
    context: &ToolContext,
    located: Location,
    plan: MutationPlan,
    changes: Option<LineChanges>,
    before: Option<Vec<u8>>,
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
        )
        .ending(Outcome::Unchanged);
    }

    // The guard is released by the end of this statement: minting takes the same
    // lock, and holding it across both would deadlock the session.
    //
    // `feedback` is what the user said while answering, and it is owned by this
    // call from here to whichever result it returns. Everything below the
    // decision can fail, and the sentence was said about a call that was
    // attempted, so every one of those exits carries it.
    let Decided { decision, feedback } = context
        .permissions()
        .decide_with_feedback(ProposedAction::Mutation(&plan));
    let source = match decision {
        PolicyDecision::Allow { source } => source,
        PolicyDecision::Deny { reason, .. } => {
            return ToolResult::failure(format!("{tool} was not permitted: {reason}"))
                .ending(Outcome::Denied)
                .with_feedback(feedback)
        }
        // `decide_with_feedback` resolves every prompt; this arm exists so that
        // a future decision variant cannot be silently treated as an approval.
        PolicyDecision::Prompt => {
            return ToolResult::failure(format!(
                "{tool} was not permitted: the approval was never resolved"
            ))
            .ending(Outcome::Denied)
            .with_feedback(feedback)
        }
    };

    // Not passed into minting: an authority is issued for the plan that was
    // judged, and a sentence is not part of it.
    let authority = context.permissions().mint_mutation(plan, source);
    // Spend first, check second. Whatever happens below -- a stale preimage, a
    // full disk, a panic-free error path -- this authority is already gone, so a
    // retry has to be authorized again rather than reusing an answer about a
    // world that has since moved.
    if let Err(err) = context.permissions().consume(&authority) {
        return ToolResult::revoked(format!("{tool} could not use its authority: {err}"))
            .with_feedback(feedback);
    }

    // The race window, made observable. Nothing in the product installs an
    // interlude; a test does, to change the filesystem at exactly this instant.
    context.run_race_interlude();

    let plan = authority
        .mutation()
        .expect("a mutation authority carries a mutation plan");
    match namespace::apply(&located, plan) {
        // Carried only by the change that was made: a refused or failed one
        // added and removed nothing, whatever it would have.
        Ok(()) => {
            if matches!(plan.kind(), MutationKind::Write | MutationKind::Edit) {
                context.changes().push(TrackedChange {
                    path: plan.target().to_path_buf(),
                    before,
                    after: ContentHash::of(plan.staged_bytes()),
                });
            }
            let summary = describe(plan);
            ToolResult::success(summary.clone(), summary)
                .with_changes(changes)
                .with_change(plan.diff().cloned())
                .with_feedback(feedback)
        }
        Err(namespace::ApplyError::Stale(reason)) => ToolResult::revoked(format!(
            "{tool} stopped: the authority for `{}` no longer describes the filesystem -- {reason}",
            plan.display()
        ))
        .with_feedback(feedback),
        Err(namespace::ApplyError::Failed(reason)) => {
            ToolResult::failure(format!("{tool} failed: {reason}")).with_feedback(feedback)
        }
    }
}

// ---------------------------------------------------------------------------
// taking a change back
// ---------------------------------------------------------------------------

/// `/undo`: takes the newest change off `context`'s journal and puts the file
/// back as it was before it (`undoLast`, `vercel-labs/fx@c1db9191
/// src/core/workspace/change_tracker.zig:53-81`).
///
/// The change is taken off whatever happens next, as upstream's is: one that
/// cannot be reversed is reported and not offered again.
///
/// # No question is asked, and every structural rule still holds
///
/// The person typed `/undo`; nothing the model proposed is being judged, so
/// there is no policy decision to make and no authority to mint -- upstream's
/// command runs `undoLast` straight from its handler
/// (`commandUndoLast`, `app_commands.zig:1242-1290`) in every permission mode.
/// What a write is held to *structurally* is held here as well, because it is
/// the same path to the same file: the target is located again from the
/// conversation's own scope ([`namespace::locate`] -- inside an authorized
/// root, no `..`, nothing under `.git` or `.xfx`, and no symbolic link on the
/// way), read without following a link, and replaced by a staged file renamed
/// into its own directory with its permission bits kept, or unlinked there.
///
/// # Nothing nobody has seen is discarded
///
/// A write in this crate refuses to replace a file this session has not read
/// as it stands ([`read_proof_missing`]). An undo replaces a file too, so it is
/// held to the same rule: the file must still be what the change left there,
/// or what this session last read of it. Anything else -- the person edited it
/// since, another program rewrote it, a link stands where the file was -- is
/// `Could not undo`, and the file is left as it is. **Stricter than upstream**,
/// whose `writeFileAtomic` replaces whatever is there (its own test replaces a
/// symbolic link with the preimage, `change_tracker.zig:501-537`).
///
/// A read proof is left as it is afterwards. The model last saw what it read;
/// if the file is that again its next edit may rest on that, and if not the
/// edit is refused until it reads the file again -- which is the rule working,
/// not a gap in it.
pub(crate) fn undo_last(context: &ToolContext) -> Undone {
    undo_through(context, Mark::LATEST)
}

/// [`undo_last`], as of `mark`: the newest change tracked at or before it is
/// the one taken back, and anything tracked since stays on the journal
/// ([`crate::tools::undo::ChangeJournal::take_through`]).
///
/// What an `/undo` typed while a turn ran is done with, once the turn is over:
/// the change it takes back is the one that was newest at the keystroke, as
/// upstream's -- done at the keystroke -- is. A change the turn made since to
/// the **same file** is not discarded to do it: that undo is `Could not undo`,
/// the file is left as the later change made it, and the change is spent like
/// any other that could not be reversed. The later change stays for the next
/// `/undo`.
pub(crate) fn undo_through(context: &ToolContext, mark: Mark) -> Undone {
    // Released before the filesystem is touched: the journal's lock guards a
    // list, never I/O.
    let Some(Taken { change, superseded }) = context.changes().take_through(mark) else {
        return Undone::Nothing;
    };
    let reversed = if superseded {
        Err(
            "a later change of this session touched the file after the undo was asked for"
                .to_string(),
        )
    } else {
        take_back(context, &change)
    };
    match reversed {
        Ok(()) if change.before.is_some() => Undone::Restored(change.path),
        Ok(()) => Undone::Deleted(change.path),
        Err(reason) => Undone::Unavailable {
            path: change.path,
            reason,
        },
    }
}

/// Puts `change`'s target back, or says why it cannot.
fn take_back(context: &ToolContext, change: &TrackedChange) -> Result<(), String> {
    let requested = change
        .path
        .to_str()
        .ok_or_else(|| "the recorded path is not text".to_string())?;
    let located = namespace::locate(context.scope(), requested, "undo")?;
    let now = namespace::read_target(&located, context.limits().max_read_bytes)?;
    let expected = match &now {
        None => Preimage::Absent,
        Some((identity, bytes)) => {
            let hash = ContentHash::of(bytes);
            let seen = hash == change.after
                || context
                    .reads()
                    .proof(located.full())
                    .is_some_and(|record| record.complete_view && record.hash == hash);
            if !seen {
                return Err(format!(
                    "`{}` is no longer what this session left there or last read",
                    located.display()
                ));
            }
            Preimage::Present {
                identity: *identity,
                hash,
            }
        }
    };
    match (&change.before, &now) {
        (Some(before), _) => namespace::restore(&located, &expected, before),
        // Already gone is the state the undo asks for (`change_tracker.zig:69-77`).
        (None, None) => Ok(()),
        (None, Some(_)) => namespace::remove(&located, &expected),
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
        /// What a configured rule is matched against for this change
        /// ([`MutationPlan::rule_targets`]).
        rule_targets: Vec<Vec<u8>>,
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

        /// Every path a configured rule judges this change at, in upstream's
        /// order, canonical and workspace-relative.
        pub fn rule_targets(&self) -> Vec<Vec<u8>> {
            self.rule_targets.clone()
        }

        /// What a saved-session rule's key binds for a call whose typed input
        /// is `fields`: each field length-prefixed, then the device and inode
        /// of the deepest directory on the path that exists -- the one the
        /// change is made in, held open by descriptor -- as upstream's key
        /// binds every target directory's identity
        /// (`permissionStateKeyForPreparedFileMutation`,
        /// `tool_admission.zig:2328-2340`). A directory swapped for another
        /// under the same name is another action.
        pub fn state_binding(&self, fields: &[&[u8]]) -> Vec<u8> {
            let mut binding = Vec::new();
            for value in fields {
                crate::permission::session_rules::field(&mut binding, value);
            }
            // Through `std`'s own reading of the same `fstat`, which names
            // both numbers `u64` on every platform (`st_dev` is signed on
            // some), on a duplicate of the descriptor so the walk's stays put.
            let (device, inode) = self
                .parent
                .try_clone()
                .map(std::fs::File::from)
                .and_then(|directory| directory.metadata())
                .map(|metadata| {
                    use std::os::unix::fs::MetadataExt;
                    (metadata.dev(), metadata.ino())
                })
                .unwrap_or((0, 0));
            crate::permission::session_rules::field(&mut binding, &device.to_be_bytes());
            crate::permission::session_rules::field(&mut binding, &inode.to_be_bytes());
            binding
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

        let rule_targets = rule_targets(scope, root, &relative, &missing);
        Ok(Location {
            parent,
            rule_targets,
            missing,
            name: relative[last].clone(),
            display: scope.display_path(&full),
            full,
            target_scope,
        })
    }

    /// The paths a configured rule judges a change at: the target, its
    /// parent, then every other directory the change creates, deepest first
    /// (`assembleFileTargetDrafts`, `vercel-labs/fx@c1db9191
    /// src/core/permissions/permissions.zig:738-801`).
    ///
    /// **Spelled as the filesystem spells them.** The walk above opened every
    /// existing component without following a link, so no link is left to
    /// resolve -- but on a filesystem that ignores case, `SECRET/a` and
    /// `secret/a` are one file and only one of them is what a rule says.
    /// Upstream matches the path its resolver `realpath`ed, so this asks the
    /// same question of the part that exists (and of the target itself when
    /// it exists and is not a link); a directory still to be created is named
    /// as written, as upstream names it. A lookup that fails falls back to the
    /// path as written, which is also what the walk just proved exists.
    fn rule_targets(
        scope: &AccessScope,
        root: &Path,
        relative: &[OsString],
        missing: &[OsString],
    ) -> Vec<Vec<u8>> {
        let last = relative.len() - 1;
        let existing = &relative[..last - missing.len()];
        let mut lexical = root.to_path_buf();
        for component in existing {
            lexical.push(component);
        }
        let deepest = std::fs::canonicalize(&lexical).unwrap_or(lexical);
        let mut created: Vec<PathBuf> = Vec::with_capacity(missing.len());
        let mut directory = deepest.clone();
        for component in missing {
            directory.push(component);
            created.push(directory.clone());
        }
        let parent = created.last().cloned().unwrap_or(deepest);
        let mut target = parent.join(&relative[last]);
        if missing.is_empty() {
            if let Ok(metadata) = std::fs::symlink_metadata(&target) {
                if !metadata.file_type().is_symlink() {
                    if let Ok(canonical) = std::fs::canonicalize(&target) {
                        target = canonical;
                    }
                }
            }
        }
        let primary = scope.primary();
        let mut targets = vec![
            crate::permission::rules::rule_path(primary, &target),
            crate::permission::rules::rule_path(primary, &parent),
        ];
        for directory in created.iter().rev().skip(1) {
            targets.push(crate::permission::rules::rule_path(primary, directory));
        }
        targets
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

        let mode = revalidate(parent, located, plan.preimage())?;
        let staged = stage(parent, plan.staged_bytes(), mode)?;
        // Checked again with the replacement already on disk, so the window
        // between the last look and the rename is as small as it can be.
        revalidate(parent, located, plan.preimage())?;

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

    /// Puts `bytes` back at the target, for `/undo`: the same revalidate,
    /// stage, revalidate, rename and sync a write is applied with, against
    /// what the caller found there (`expected`), and **never** by creating a
    /// directory -- a directory on the path that has gone since the change
    /// means the file cannot be put back where it was, and upstream refuses
    /// that too (`change_tracker.zig:399-423`, "does not create parents").
    pub fn restore(located: &Location, expected: &Preimage, bytes: &[u8]) -> Result<(), String> {
        if !located.reachable() {
            return Err(format!(
                "a directory on the path to `{}` is gone",
                located.display()
            ));
        }
        let parent = located.parent();
        let mode = revalidate(parent, located, expected).map_err(ApplyError::into_reason)?;
        let staged = stage(parent, bytes, mode).map_err(ApplyError::into_reason)?;
        revalidate(parent, located, expected).map_err(ApplyError::into_reason)?;
        rustix::fs::renameat(
            parent,
            staged.name.as_os_str(),
            parent,
            located.name.as_os_str(),
        )
        .map_err(|err| format!("cannot replace the file: {err}"))?;
        staged.keep();
        sync_directory(parent).map_err(ApplyError::into_reason)
    }

    /// Removes the target, for `/undo` of a change that created it: once the
    /// target is still what the caller found there (`expected`), unlinked
    /// relative to its pinned directory -- which removes a name and never
    /// follows one.
    pub fn remove(located: &Location, expected: &Preimage) -> Result<(), String> {
        let parent = located.parent();
        revalidate(parent, located, expected).map_err(ApplyError::into_reason)?;
        match rustix::fs::unlinkat(
            parent,
            located.name.as_os_str(),
            rustix::fs::AtFlags::empty(),
        ) {
            // Gone between the look and the unlink is gone.
            Ok(()) | Err(Errno::NOENT) => {}
            Err(err) => return Err(format!("cannot remove the file: {err}")),
        }
        sync_directory(parent).map_err(ApplyError::into_reason)
    }

    impl ApplyError {
        fn into_reason(self) -> String {
            match self {
                Self::Stale(reason) | Self::Failed(reason) => reason,
            }
        }
    }

    /// Confirms the target still is what `expected` says it was.
    ///
    /// Returns the permission bits to give the replacement: the target's own, so
    /// an executable script stays executable.
    fn revalidate(
        parent: BorrowedFd<'_>,
        located: &Location,
        expected: &Preimage,
    ) -> Result<u32, ApplyError> {
        let opened = rustix::fs::openat(
            parent,
            located.name.as_os_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        );
        match (expected, opened) {
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

        pub fn rule_targets(&self) -> Vec<Vec<u8>> {
            Vec::new()
        }

        pub fn state_binding(&self, _fields: &[&[u8]]) -> Vec<u8> {
            Vec::new()
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

    pub fn restore(
        _located: &Location,
        _expected: &crate::permission::Preimage,
        _bytes: &[u8],
    ) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn remove(
        _located: &Location,
        _expected: &crate::permission::Preimage,
    ) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
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
    fn an_edit_asks_with_the_exact_bounded_file_before_and_after_it_would_write() {
        // The whole risk of an edit is which bytes leave and which arrive, and
        // the 160-byte summary can only quote the beginning of each. The review
        // gets the exact pair, bounded once at the permission boundary rather
        // than trusted to whatever renders it -- and the pair is the **file**,
        // not the two strings the model sent, because a review that numbers its
        // lines and shows the ones around the change (upstream's) needs the text
        // those strings sit in. (Claim changed: this pinned `old_string` and
        // `new_string` until the review learned to number lines.)
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
        assert_eq!(diff.before, "alpha and more\n");
        // The break is **kept**, because the surface this side is built for
        // places rows and a break is where a row ends
        // (`crate::permission::bounded_diff_side`). The band's own one-line
        // summary still names it.
        assert_eq!(diff.after, "beta\ngamma and more\n");
    }

    #[test]
    fn an_edit_past_the_first_64_kib_of_a_file_still_reaches_the_review_with_its_line_number() {
        // A side is bounded at 64 KiB and a source file is routinely bigger: a
        // review of the *file* cut at that bound would hold no change at all
        // for an edit past it, and its two cut ends would read as a change that
        // is not being made. What the boundary sends is the window around the
        // change, and where it sits.
        let contents: String = (1..=4_000)
            .map(|line| format!("line number {line:05}\n"))
            .collect();
        assert!(
            contents.len() > 64 * 1024,
            "the fixture is not past the bound"
        );
        let (_dir, context, recorder) = asking(&contents);
        let input = decode_edit_file(&json!({
            "path": "notes.txt",
            "old_string": "line number 03900\n",
            "new_string": "LINE NUMBER 3900\n",
        }))
        .expect("decodes");

        let result = execute_edit_file(&input, &context);
        assert!(!result.ok, "the fixture prompter denies: {}", result.output);

        let diff = recorder.last().diff.expect("an edit carries its diff");
        assert!(
            diff.before.contains("line number 03900") && diff.after.contains("LINE NUMBER 3900"),
            "the change is not in what the review is sent: {} / {} bytes",
            diff.before.len(),
            diff.after.len()
        );
        assert_eq!(
            diff.first_line, 3_895,
            "five unchanged lines above the change"
        );
        assert_eq!(
            diff.lines_below,
            4_000 - 3_905,
            "and the rest counted below"
        );
        assert!(
            diff.before.len() < 1024,
            "the window carried more than the change"
        );
    }

    #[test]
    fn an_edit_larger_than_the_bound_is_cut_before_it_leaves_the_permission_boundary() {
        // One bound, at the boundary that builds the question. A tool that
        // handed the whole file on and left the cut to the renderer would put
        // four megabytes -- `ToolLimits::max_mutation_bytes` -- on a channel
        // sized for two sides of 64 KiB.
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
    fn a_write_asks_with_the_file_it_is_replacing_and_the_text_replacing_it() {
        // **A whole-file write is the largest change this product makes**, and
        // for a long time it was the one asked about in a sentence: a digest,
        // a size, and 160 bytes of preview. The pair is right there -- the
        // preimage this executor has already read (and holds a complete read
        // proof for) and the exact text that would replace it -- so the review
        // surface gets it, bounded and escaped at this boundary like an edit's.
        let (_dir, context, recorder) = asking("alpha\nand more\n");
        let write = decode_write_file(&json!({ "path": "notes.txt", "content": "beta\ngamma" }))
            .expect("decodes");

        let written = execute_write_file(&write, &context);
        assert!(
            !written.ok,
            "the fixture prompter denies: {}",
            written.output
        );

        let diff = recorder.last().diff.expect("a write carries its diff");
        // Line for line, with the breaks kept: the review screen turns each one
        // into a row, so the file reads as the file.
        assert_eq!(diff.before, "alpha\nand more\n");
        assert_eq!(diff.after, "beta\ngamma");
        // Which surface a question takes is no longer this layer's to say: it
        // is how many rows the TUI's review of the change needs
        // (`crate::tui::approval::ApprovalSurface`), and it is pinned there.
    }

    #[test]
    fn a_write_to_a_path_with_nothing_at_it_says_so_rather_than_inventing_a_before() {
        // A create is a change with one side, and the surface has a way of
        // saying that (`super::super::tui::approval_screen`'s `(nothing)`).
        // An empty string is the honest before; a fabricated one would be a
        // diff against a file that does not exist.
        let (dir, context, recorder) = asking("alpha\n");
        let _ = dir;
        let write = decode_write_file(&json!({ "path": "fresh.txt", "content": "beta\n" }))
            .expect("decodes");

        let written = execute_write_file(&write, &context);
        assert!(
            !written.ok,
            "the fixture prompter denies: {}",
            written.output
        );

        let diff = recorder.last().diff.expect("a write carries its diff");
        assert_eq!(diff.before, "");
        assert_eq!(diff.after, "beta\n");
    }

    #[test]
    fn a_write_over_bytes_that_are_not_text_names_them_instead_of_showing_them() {
        // The one case a before/after cannot be rendered as text, and the
        // failure to avoid is a **lossy** one: bytes that are not UTF-8 shown
        // as replacement characters would read as a file whose contents are
        // question marks, and a reviewer would be approving a diff against
        // something that is not what is there. So the side says what it is --
        // the same sentence `MutationPlan::preview` uses for the same fact.
        let dir = tempfile::tempdir().expect("temporary workspace");
        let bytes = [0xffu8, 0xfe, 0x00, 0x41];
        std::fs::write(dir.path().join("notes.txt"), bytes).expect("the fixture file");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("scope");
        let path = scope.primary().join("notes.txt");
        let recorder = Recorder::default();
        let context = ToolContext::new(scope).with_permissions(
            crate::permission::PermissionSession::new(crate::permission::PermissionMode::Ask)
                .with_prompter(Box::new(recorder.clone())),
        );
        let metadata = std::fs::metadata(&path).expect("the fixture's metadata");
        record_read(&context, &path, &metadata, &bytes, true);
        let write = decode_write_file(&json!({ "path": "notes.txt", "content": "beta\n" }))
            .expect("decodes");

        let written = execute_write_file(&write, &context);
        assert!(
            !written.ok,
            "the fixture prompter denies: {}",
            written.output
        );

        let diff = recorder.last().diff.expect("a write carries its diff");
        // The literal, not the function that produces it: a test that asked
        // `non_text_summary` what it says would agree with any answer it gave,
        // including an empty one.
        assert_eq!(
            diff.before, "<4 bytes of non-UTF-8 data>",
            "a file that is not text was rendered as lossy text"
        );
        // And it is the **shared** sentence, which is the half a literal alone
        // would not pin: the band's preview of a write's bytes and this screen's
        // "before" say the same thing about the same fact.
        assert_eq!(
            diff.before,
            crate::permission::non_text_summary(bytes.len())
        );
        assert!(
            !diff.before.contains('\u{fffd}'),
            "the bytes were shown as replacement characters: {:?}",
            diff.before
        );
        assert_eq!(diff.after, "beta\n");
    }

    #[test]
    fn a_write_larger_than_the_bound_is_cut_before_it_leaves_the_permission_boundary() {
        // One bound, at the boundary that builds the question, exactly as an
        // edit's is: the preimage may be a quarter of a megabyte and the
        // content four, and nothing downstream may be handed either whole.
        let contents = "a".repeat(100_000);
        let (_dir, context, recorder) = asking(&contents);
        let write =
            decode_write_file(&json!({ "path": "notes.txt", "content": "b".repeat(100_000) }))
                .expect("decodes");

        let written = execute_write_file(&write, &context);
        assert!(
            !written.ok,
            "the fixture prompter denies: {}",
            written.output
        );

        let diff = recorder.last().diff.expect("a write carries its diff");
        assert_eq!(diff.before.len(), 65_536);
        assert_eq!(diff.after.len(), 65_536);
        assert!(diff.before.ends_with('\u{2026}'));
        assert!(diff.after.ends_with('\u{2026}'));
    }

    #[test]
    fn a_create_folder_has_no_before_and_after_to_review_so_its_question_stays_in_the_band() {
        // It changes no content at all: there is no pair a diff screen could
        // show, and inventing one would be a screen about nothing.
        let (_dir, context, recorder) = asking("alpha\n");
        let create = decode_create_folder(&json!({ "path": "made" })).expect("decodes");
        let created = execute_create_folder(&create, &context);
        assert!(
            !created.ok,
            "the fixture prompter denies: {}",
            created.output
        );
        assert!(recorder.last().diff.is_none(), "a create_folder had a diff");
    }

    #[test]
    fn a_change_that_was_made_carries_its_diff_whether_or_not_anyone_reviewed_it() {
        // The full transcript shows the diff of every change, as upstream's
        // does (`full_transcript_screen.zig:5731-5760`), and most changes are
        // never reviewed: `yolo` asks nothing. The pair is the one the plan
        // was judged on -- built from the preimage the tool read -- and only
        // the change that was made carries it.
        let dir = tempfile::tempdir().expect("temporary workspace");
        std::fs::write(dir.path().join("notes.txt"), "alpha\n").expect("the fixture file");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("scope");
        let path = scope.primary().join("notes.txt");
        let context = ToolContext::new(scope).with_permissions(
            crate::permission::PermissionSession::new(crate::permission::PermissionMode::Yolo),
        );
        let metadata = std::fs::metadata(&path).expect("the fixture's metadata");
        record_read(&context, &path, &metadata, b"alpha\n", true);

        let edit = decode_edit_file(&json!({
            "path": "notes.txt",
            "old_string": "alpha",
            "new_string": "beta",
        }))
        .expect("decodes");
        let edited = execute_edit_file(&edit, &context);
        assert!(edited.ok, "{}", edited.output);
        let change = edited.change.expect("a made edit carries its change");
        assert_eq!(change.before, "alpha\n");
        assert_eq!(change.after, "beta\n");

        let write = decode_write_file(&json!({ "path": "fresh.txt", "content": "new\n" }))
            .expect("decodes");
        let written = execute_write_file(&write, &context);
        assert!(written.ok, "{}", written.output);
        let change = written.change.expect("a made write carries its change");
        assert_eq!(
            (change.before.as_str(), change.after.as_str()),
            ("", "new\n")
        );

        let create = decode_create_folder(&json!({ "path": "made" })).expect("decodes");
        let created = execute_create_folder(&create, &context);
        assert!(created.ok, "{}", created.output);
        assert!(created.change.is_none(), "a folder has no content to diff");

        // Asked and refused: nothing was changed, so there is no change.
        let (_dir, context, _recorder) = asking("alpha\n");
        let refused = execute_edit_file(&edit, &context);
        assert!(!refused.ok);
        assert!(refused.change.is_none(), "a refused edit carried a change");
    }

    // -----------------------------------------------------------------------
    // /undo
    // -----------------------------------------------------------------------

    use crate::tools::undo::{TrackedChange, Undone};

    /// A workspace whose tools may change anything, as `yolo` lets them, so
    /// each case is about what `/undo` makes of what was committed.
    fn committing() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("scope");
        let context = ToolContext::new(scope).with_permissions(
            crate::permission::PermissionSession::new(crate::permission::PermissionMode::Yolo),
        );
        (dir, context)
    }

    /// `name` as the tools locate it: under the scope's canonical root.
    fn located(context: &ToolContext, name: &str) -> std::path::PathBuf {
        context.scope().primary().join(name)
    }

    fn write(context: &ToolContext, name: &str, content: &str) -> ToolResult {
        let input =
            decode_write_file(&json!({ "path": name, "content": content })).expect("decodes");
        execute_write_file(&input, context)
    }

    fn edit(context: &ToolContext, name: &str, old: &str, new: &str) -> ToolResult {
        let input = decode_edit_file(&json!({
            "path": name,
            "old_string": old,
            "new_string": new,
        }))
        .expect("decodes");
        execute_edit_file(&input, context)
    }

    /// The proof a complete `read_file` of `name` leaves, from the file as it
    /// is now.
    fn read(context: &ToolContext, name: &str) {
        let path = located(context, name);
        let bytes = std::fs::read(&path).expect("read the file");
        let metadata = std::fs::metadata(&path).expect("its metadata");
        record_read(context, &path, &metadata, &bytes, true);
    }

    fn contents(context: &ToolContext, name: &str) -> Option<String> {
        std::fs::read_to_string(located(context, name)).ok()
    }

    #[test]
    fn undo_takes_back_an_overwrite_then_a_creation_then_has_nothing_left() {
        // fx's own receipt (`fx-undo-40x120`): `a.txt` created as `one`, then
        // overwritten with `two`; `/undo` three times says `Restored`, then
        // `Deleted … (was newly created)`, then `Nothing to undo.`, and the
        // file is `one`, then gone.
        let (_dir, context) = committing();
        let path = located(&context, "a.txt");
        assert!(write(&context, "a.txt", "one\n").ok);
        read(&context, "a.txt");
        assert!(write(&context, "a.txt", "two\n").ok);
        assert_eq!(context.changes().len(), 2);

        assert_eq!(undo_last(&context), Undone::Restored(path.clone()));
        assert_eq!(contents(&context, "a.txt").as_deref(), Some("one\n"));
        assert_eq!(undo_last(&context), Undone::Deleted(path.clone()));
        assert!(!path.exists(), "the created file is still there");
        assert_eq!(undo_last(&context), Undone::Nothing);
        assert_eq!(
            Undone::Restored(path.clone()).sentence(),
            format!("Restored {}", path.display())
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_edit_is_taken_back_with_the_files_permission_bits_kept() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, context) = committing();
        let path = located(&context, "run.sh");
        std::fs::write(&path, "echo one\n").expect("the fixture");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make it executable");
        read(&context, "run.sh");
        assert!(edit(&context, "run.sh", "one", "two").ok);
        assert_eq!(contents(&context, "run.sh").as_deref(), Some("echo two\n"));

        assert_eq!(undo_last(&context), Undone::Restored(path.clone()));
        assert_eq!(contents(&context, "run.sh").as_deref(), Some("echo one\n"));
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755, "the restored script stopped being executable");
        // Replaced by a staged file renamed into place: nothing staged is
        // left behind beside it.
        let names: Vec<_> = std::fs::read_dir(context.scope().primary())
            .expect("list")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("run.sh")]);
    }

    #[test]
    fn only_a_file_change_that_landed_is_tracked() {
        let (_dir, context) = committing();
        // A folder: upstream tracks `write_file` and `edit_file` alone.
        let create = decode_create_folder(&json!({ "path": "made" })).expect("decodes");
        assert!(execute_create_folder(&create, &context).ok);
        // A write that changes nothing.
        std::fs::write(located(&context, "same.txt"), "same\n").expect("fixture");
        read(&context, "same.txt");
        let unchanged = write(&context, "same.txt", "same\n");
        assert_eq!(unchanged.outcome, Outcome::Unchanged);
        // A write refused for want of a read proof.
        std::fs::write(located(&context, "unread.txt"), "unread\n").expect("fixture");
        assert!(!write(&context, "unread.txt", "replaced\n").ok);
        // An edit whose text is not there.
        assert!(!edit(&context, "same.txt", "absent", "x").ok);
        assert!(
            context.changes().is_empty(),
            "something not changed was tracked"
        );
        assert_eq!(undo_last(&context), Undone::Nothing);
        assert!(
            located(&context, "made").is_dir(),
            "the folder was taken back"
        );

        // And a change a person refused at the question.
        let (_dir, asked, _recorder) = asking("alpha\n");
        assert!(!edit(&asked, "notes.txt", "alpha", "beta").ok);
        assert!(asked.changes().is_empty(), "a refused change was tracked");
    }

    /// A prompter that says yes once to everything, and counts the questions.
    #[derive(Clone, Default)]
    struct Allowing(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl crate::permission::ApprovalPrompter for Allowing {
        fn request(
            &mut self,
            _request: &crate::permission::ApprovalRequest,
        ) -> std::io::Result<crate::permission::ApprovalAnswer> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::permission::ApprovalAnswer::Once)
        }
    }

    #[test]
    fn undo_asks_no_question_even_in_ask_mode() {
        // Upstream's `/undo` runs `undoLast` straight from its handler
        // (`app_commands.zig:1242-1290`): the person typed it, and there is no
        // proposal of the model's to judge. The model's change was asked about;
        // taking it back is not.
        let dir = tempfile::tempdir().expect("temporary workspace");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("scope");
        let asked = Allowing::default();
        let context = ToolContext::new(scope).with_permissions(
            crate::permission::PermissionSession::new(crate::permission::PermissionMode::Ask)
                .with_prompter(Box::new(asked.clone())),
        );
        assert!(write(&context, "a.txt", "one\n").ok);
        assert_eq!(asked.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        let (issued, consumed) = context.permissions().ledger_counts();

        assert_eq!(
            undo_last(&context),
            Undone::Deleted(located(&context, "a.txt"))
        );
        assert_eq!(
            asked.0.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the undo asked a question"
        );
        assert_eq!(
            context.permissions().ledger_counts(),
            (issued, consumed),
            "the undo minted an authority as though the model had proposed it"
        );
    }

    #[test]
    fn a_file_changed_since_is_left_as_it_is_and_the_change_is_spent() {
        // The rule every write here is held to: nothing nobody in this session
        // has seen is replaced. Upstream restores over whatever is there
        // (`change_tracker.zig:53-81`); this refuses, and says `Could not undo`
        // as upstream says it for any failure -- with the change taken off the
        // journal either way.
        let (_dir, context) = committing();
        std::fs::write(located(&context, "kept.txt"), "one\n").expect("fixture");
        read(&context, "kept.txt");
        assert!(write(&context, "kept.txt", "two\n").ok);
        assert!(write(&context, "new.txt", "made\n").ok);
        std::fs::write(located(&context, "kept.txt"), "the person's edit\n").expect("edit");
        std::fs::write(located(&context, "new.txt"), "the person's too\n").expect("edit");

        let undone = undo_last(&context);
        assert!(
            matches!(&undone, Undone::Unavailable { path, reason }
                if *path == located(&context, "new.txt") && reason.contains("no longer what")),
            "{undone:?}"
        );
        assert_eq!(
            undone.sentence(),
            format!("Could not undo {}", located(&context, "new.txt").display())
        );
        assert_eq!(
            contents(&context, "new.txt").as_deref(),
            Some("the person's too\n")
        );
        assert!(matches!(undo_last(&context), Undone::Unavailable { .. }));
        assert_eq!(
            contents(&context, "kept.txt").as_deref(),
            Some("the person's edit\n")
        );
        assert_eq!(
            undo_last(&context),
            Undone::Nothing,
            "a spent change came back"
        );
    }

    #[test]
    fn a_file_this_session_has_read_since_it_changed_may_be_put_back() {
        // The other half of the rule: content the session has read is content
        // it has seen, exactly as a write may replace it.
        let (_dir, context) = committing();
        std::fs::write(located(&context, "a.txt"), "one\n").expect("fixture");
        read(&context, "a.txt");
        assert!(write(&context, "a.txt", "two\n").ok);
        std::fs::write(located(&context, "a.txt"), "three\n").expect("edit");
        read(&context, "a.txt");

        assert_eq!(
            undo_last(&context),
            Undone::Restored(located(&context, "a.txt"))
        );
        assert_eq!(contents(&context, "a.txt").as_deref(), Some("one\n"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_standing_where_the_file_was_is_neither_followed_nor_replaced() {
        // `undo restore cannot be redirected by a symlink introduced after
        // capture` (`change_tracker.zig:501-537`): upstream replaces the link
        // with the preimage; a write here refuses a link at its target, and so
        // does this. Either way the link's target is never written.
        let (_dir, context) = committing();
        let outside = tempfile::tempdir().expect("another directory");
        let target = outside.path().join("target.txt");
        std::fs::write(&target, "must stay untouched\n").expect("fixture");
        std::fs::write(located(&context, "a.txt"), "one\n").expect("fixture");
        read(&context, "a.txt");
        assert!(write(&context, "a.txt", "two\n").ok);
        assert!(write(&context, "b.txt", "made\n").ok);
        for name in ["a.txt", "b.txt"] {
            std::fs::remove_file(located(&context, name)).expect("remove");
            std::os::unix::fs::symlink(&target, located(&context, name)).expect("link");
        }

        assert!(matches!(undo_last(&context), Undone::Unavailable { .. }));
        assert!(matches!(undo_last(&context), Undone::Unavailable { .. }));
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "must stay untouched\n"
        );
        for name in ["a.txt", "b.txt"] {
            let kind = std::fs::symlink_metadata(located(&context, name)).expect("lstat");
            assert!(
                kind.file_type().is_symlink(),
                "{name} is no longer the link"
            );
        }
    }

    #[test]
    fn a_directory_that_has_gone_is_not_made_again() {
        // `undoLast pops before filesystem restore failures, reports them, and
        // does not create parents` (`change_tracker.zig:399-423`).
        let (_dir, context) = committing();
        std::fs::create_dir(located(&context, "sub")).expect("fixture dir");
        std::fs::write(located(&context, "sub/a.txt"), "one\n").expect("fixture");
        read(&context, "sub/a.txt");
        assert!(write(&context, "sub/a.txt", "two\n").ok);
        std::fs::remove_dir_all(located(&context, "sub")).expect("remove the directory");

        assert!(matches!(undo_last(&context), Undone::Unavailable { .. }));
        assert!(
            !located(&context, "sub").exists(),
            "the directory was made again"
        );
        assert_eq!(undo_last(&context), Undone::Nothing);
    }

    #[test]
    fn a_created_file_already_gone_is_deleted_all_the_same() {
        // `undoLast reports deleted for a new write when the file is already
        // absent` (`change_tracker.zig:330-357`).
        let (_dir, context) = committing();
        assert!(write(&context, "a.txt", "made\n").ok);
        std::fs::remove_file(located(&context, "a.txt")).expect("remove");
        assert_eq!(
            undo_last(&context),
            Undone::Deleted(located(&context, "a.txt"))
        );
    }

    #[test]
    fn a_restored_file_is_refused_to_an_edit_until_it_is_read_as_it_now_is() {
        // The model is not told of an undo (upstream's isn't either), so what
        // it last read can stop being what the file holds. Its next edit then
        // rests on a proof that no longer matches and is refused until it reads
        // again -- the read-proof rule doing its job, not a gap in it.
        let (_dir, context) = committing();
        std::fs::write(located(&context, "a.txt"), "one\n").expect("fixture");
        read(&context, "a.txt");
        assert!(edit(&context, "a.txt", "one", "two").ok);
        read(&context, "a.txt");
        assert!(edit(&context, "a.txt", "two", "three").ok);

        assert!(matches!(undo_last(&context), Undone::Restored(_)));
        assert!(matches!(undo_last(&context), Undone::Restored(_)));
        assert_eq!(contents(&context, "a.txt").as_deref(), Some("one\n"));

        let refused = edit(&context, "a.txt", "one", "four");
        assert!(!refused.ok);
        assert!(
            refused.output.contains("read it again first"),
            "{}",
            refused.output
        );
        read(&context, "a.txt");
        assert!(edit(&context, "a.txt", "one", "four").ok);
    }

    #[test]
    fn an_undo_pinned_before_a_later_change_takes_back_the_one_before_it() {
        // `/undo` typed mid-turn, done once the turn is over: the change that
        // was newest at the keystroke is taken back, and the turn's later
        // change -- another file -- stays for the next `/undo`.
        let (_dir, context) = committing();
        assert!(write(&context, "a.txt", "one\n").ok);
        let typed = Mark::now();
        assert!(write(&context, "b.txt", "two\n").ok);

        assert_eq!(
            undo_through(&context, typed),
            Undone::Deleted(located(&context, "a.txt"))
        );
        assert_eq!(contents(&context, "b.txt").as_deref(), Some("two\n"));
        assert_eq!(undo_through(&context, typed), Undone::Nothing);
        assert_eq!(
            undo_last(&context),
            Undone::Deleted(located(&context, "b.txt"))
        );
    }

    #[test]
    fn an_undo_pinned_before_a_later_change_to_the_same_file_takes_nothing_back() {
        // The later change is the file as it now stands; putting the older
        // preimage back would discard it. `Could not undo`, the file as the
        // later change left it, the older change spent -- and the later one
        // still there to be taken back on its own.
        let (_dir, context) = committing();
        std::fs::write(located(&context, "a.txt"), "one\n").expect("fixture");
        read(&context, "a.txt");
        assert!(write(&context, "a.txt", "two\n").ok);
        let typed = Mark::now();
        read(&context, "a.txt");
        assert!(write(&context, "a.txt", "three\n").ok);
        // A complete read of the file as it now is, which on its own would
        // let a restore through: the rule here is about the journal.
        read(&context, "a.txt");

        let undone = undo_through(&context, typed);
        assert!(
            matches!(&undone, Undone::Unavailable { path, reason }
                if *path == located(&context, "a.txt") && reason.contains("a later change")),
            "{undone:?}"
        );
        assert_eq!(
            undone.sentence(),
            format!("Could not undo {}", located(&context, "a.txt").display())
        );
        assert_eq!(contents(&context, "a.txt").as_deref(), Some("three\n"));
        assert_eq!(context.changes().len(), 1);
        assert_eq!(
            undo_last(&context),
            Undone::Restored(located(&context, "a.txt"))
        );
        assert_eq!(contents(&context, "a.txt").as_deref(), Some("two\n"));
    }

    #[test]
    fn a_hostile_name_is_taken_back_and_said_spelled_out() {
        // `hostile path stays encoded through approval transcript changes and
        // undo` (`tests/e2e/tui-permissions.test.ts:1941-1982`): a file whose
        // name carries a window-title sequence and a line feed is created,
        // taken back, and named in the notice as text.
        let (_dir, context) = committing();
        let name = "name\u{1b}]2;FX_PWN\u{7}\nfile.txt";
        assert!(write(&context, name, "hostile\n").ok);
        let path = located(&context, name);
        assert!(path.exists());

        let undone = undo_last(&context);
        assert_eq!(undone, Undone::Deleted(path.clone()));
        assert!(!path.exists(), "the hostile file is still there");
        let said = undone.sentence();
        assert!(
            said.ends_with("/name\\x1b]2;FX_PWN\\x07\\x0afile.txt (was newly created)"),
            "{said:?}"
        );
        assert!(
            !said.contains('\u{1b}') && !said.contains('\u{7}') && !said.contains('\n'),
            "{said:?}"
        );
    }

    #[test]
    fn undo_reaches_only_what_a_write_could_reach() {
        // The journal is filled by `commit` alone, so a change outside the
        // workspace or under `.git` cannot be on it -- and if one were, the
        // undo locates it again from the conversation's scope, as a write
        // would, and is refused before it touches anything.
        let (_dir, context) = committing();
        let outside = tempfile::tempdir().expect("another directory");
        let stray = outside.path().join("stray.txt");
        std::fs::write(&stray, "outside\n").expect("fixture");
        std::fs::create_dir(located(&context, ".git")).expect("fixture dir");
        let config = located(&context, ".git/config");
        std::fs::write(&config, "[core]\n").expect("fixture");
        for path in [stray.clone(), config.clone()] {
            let now = std::fs::read(&path).expect("read");
            context.changes().push(TrackedChange {
                path: path.clone(),
                before: Some(b"planted\n".to_vec()),
                after: ContentHash::of(&now),
            });
        }

        for _ in 0..2 {
            assert!(matches!(undo_last(&context), Undone::Unavailable { .. }));
        }
        assert_eq!(std::fs::read_to_string(&stray).expect("read"), "outside\n");
        assert_eq!(std::fs::read_to_string(&config).expect("read"), "[core]\n");
    }
}
