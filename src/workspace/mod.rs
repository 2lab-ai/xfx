//! The workspace: which real directories a turn may look at, and how a
//! requested path becomes a proven one.
//!
//! This module owns the security boundary for every read fxr performs. It has
//! no opinion about *what* is read -- that belongs to [`crate::tools`] -- only
//! about where.

pub mod path;

pub use path::{
    is_ignored_directory, AccessScope, PathError, ResolvedPath, IGNORED_DIRECTORY_NAMES,
};
