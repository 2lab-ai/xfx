//! Permission modes, policies, and one-use execution authorities.
//!
//! Reading is free. Changing a file and running a command are not, and this
//! module is the only place that can say yes to either. It owns three things:
//!
//! - [`command`]: what a command would *do*, decided from its text.
//! - [`authority`]: the proofs a mutation rests on -- read records, content
//!   digests, file identities -- and the [`ExecutionAuthority`] that carries
//!   permission for exactly one prepared action.
//! - [`policy`]: the modes, the configured rules, the session grants, the
//!   approval channel, and the decision itself.
//!
//! # Why this shape
//!
//! The dangerous version of a permission system is a boolean returned by a
//! function that has already opened the file. Then "allowed" is a fact about a
//! moment that has passed, and the thing that runs is not necessarily the thing
//! that was judged. So the pipeline is staged, and each stage narrows:
//!
//! | Stage | Produces | Can it change the world? |
//! |---|---|---|
//! | decode / validate | a typed input | no |
//! | prepare | a [`MutationPlan`] or [`CommandPlan`]: exact target, preimage, exact bytes | no |
//! | policy | a [`PolicyDecision`] | only by asking the user |
//! | mint | an [`ExecutionAuthority`] with a fresh [`Nonce`] | no |
//! | execute | the change | yes, once, after revalidating |
//!
//! Two consequences are the point. A decision cannot mutate its own target,
//! because by then the target is a value in a plan. And an authority is spent
//! before it is checked, so a failure -- a stale preimage, a swapped symlink, a
//! full disk -- burns it, and a retry has to be authorized again.
//!
//! # What this is not
//!
//! It is not confinement. xfx runs commands as the user, with no OS sandbox, and
//! `status` reports `sandbox=none` for that reason. `auto` bounds *what xfx
//! agrees to start*, and nothing bounds what a started process then does
//! (design, "Risks and controls").

pub mod authority;
pub mod command;
pub mod policy;

/// The permission mode is a configuration setting, and configuration owns it.
///
/// Re-exported here so that a caller reasoning about permissions has one import,
/// but there is deliberately only one definition: two enums with the same three
/// names would eventually disagree.
pub use crate::config::PermissionMode;

pub use authority::{
    bounded_excerpt, AuthorityError, AuthorityLedger, CommandPlan, CommandRoute, ContentHash,
    ExecutionAuthority, FileIdentity, MutationExcerpt, MutationKind, MutationPlan, Nonce, Preimage,
    PreparedCommand, PreparedMutation, ReadRecord, ReadTracker, TargetScope, MAX_EXCERPT_BYTES,
};
pub use command::{classify, CommandEffect, DeniedEffect};
pub use policy::{
    AllowSource, ApprovalAnswer, ApprovalPrompter, ApprovalRequest, DenyCause, Grant,
    PermissionRules, PermissionSession, PolicyDecision, ProposedAction, Rule, TtyPrompter,
    YOLO_WARNING,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Takes the *configuration* type by name, on purpose.
    fn configured(mode: crate::config::PermissionMode) -> &'static str {
        mode.label()
    }

    #[test]
    fn the_permission_mode_here_is_the_configured_one() {
        // Not a conversion: this passes `permission::PermissionMode` values to a
        // function that demands `config::PermissionMode`. Two enums with the
        // same three names would not compile, which is the point -- the most
        // security-relevant setting xfx has must not have two definitions that
        // can drift.
        assert_eq!(configured(PermissionMode::Ask), "ask");
        assert_eq!(configured(PermissionMode::Auto), "auto");
        assert_eq!(configured(PermissionMode::Yolo), "yolo");
    }
}
