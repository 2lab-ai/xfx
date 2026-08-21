//! The workspace: which real directories a turn may look at, how a requested
//! path becomes a proven one, and what the project has said about itself.
//!
//! This module owns the security boundary for every read xfx performs. It has
//! no opinion about *what* is read -- that belongs to [`crate::tools`] -- only
//! about where.
//!
//! [`context`] sits on the same boundary from the other side: it decides which
//! `AGENTS.md` files a turn may deliver to the model, and it is deliberately
//! narrower than the read scope, because a directory the user opened for
//! *reading* must not also become a source of instructions.

pub mod context;
pub mod path;

pub use context::{
    ContextLimits, ContextOmission, ContextScopeKind, ContextSection, OmissionReason,
    ProjectContext, CONTEXT_FILE_NAME, CONTEXT_GUIDANCE,
};
pub use path::{
    is_ignored_directory, is_protected_write_directory, AccessScope, PathError, ResolvedPath,
    IGNORED_DIRECTORY_NAMES, PROTECTED_WRITE_DIRECTORY_NAMES,
};
