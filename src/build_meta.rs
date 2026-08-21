// The pure rules behind the two build facts `status` reports: which channel a
// binary claims, and which source revision it was compiled from.
//
// `build.rs` `include!`s this file rather than importing it, because a build
// script cannot depend on the crate it is building. The library includes it
// again under `#[cfg(test)]`, because nothing ever runs a build script's own
// `#[test]`s -- so without this the rules that decide what a receipt claims
// would be the one part of the product with no test at all.
//
// The header is `//` rather than `//!` for the same reason: an inner doc
// comment is only legal at the top of a file, and this text arrives in the
// middle of the build script.

/// The width upstream uses for an abbreviated revision
/// (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:729`).
pub const REVISION_WIDTH: usize = 12;

/// Every channel a build may claim.
///
/// `debug` and `release` are the compile profiles Cargo reports. `preview` is
/// the one that is not a profile: it is a provenance claim made by the workflow
/// that publishes a preview build, which compiles with the release profile but
/// must not be read as a tagged release.
pub const CHANNELS: [&str; 3] = ["debug", "release", "preview"];

/// The channel to stamp, given an explicit `XFX_BUILD_CHANNEL` and Cargo's
/// `PROFILE`.
///
/// An absent or blank override leaves the compile profile, which is what this
/// metadata meant before there was anything to override. An override that is
/// present and is not one of [`CHANNELS`] is an error rather than a fallback:
/// the point of the variable is that a published binary can be tied to the run
/// that built it, and a misspelled `previw` that quietly stamped `release`
/// would be a build lying about where it came from. Failing the build costs a
/// re-run; a wrong channel costs the meaning of the field.
pub fn resolve_channel(explicit: Option<&str>, profile: Option<&str>) -> Result<String, String> {
    if let Some(requested) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        let normalized = requested.to_ascii_lowercase();
        if !CHANNELS.contains(&normalized.as_str()) {
            return Err(format!(
                "XFX_BUILD_CHANNEL is {requested:?}, which is not one of {}",
                CHANNELS.join(", ")
            ));
        }
        return Ok(normalized);
    }
    Ok(profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("debug")
        .to_string())
}

/// Accepts only a hex revision and abbreviates it to the fixed width.
///
/// Anything else is discarded, because a malformed revision in a receipt is
/// worse than an absent one: an absent revision says "unknown", and a wrong one
/// says "this commit", which is checkable and false.
pub fn normalize_revision(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() < REVISION_WIDTH || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(trimmed[..REVISION_WIDTH].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_channel_leaves_the_compile_profile() {
        assert_eq!(resolve_channel(None, Some("release")).unwrap(), "release");
        assert_eq!(resolve_channel(None, Some("debug")).unwrap(), "debug");
    }

    #[test]
    fn a_blank_channel_is_treated_as_absent() {
        // A workflow that sets the variable from an empty expression has said
        // nothing, not something wrong.
        assert_eq!(
            resolve_channel(Some(""), Some("release")).unwrap(),
            "release"
        );
        assert_eq!(
            resolve_channel(Some("   "), Some("release")).unwrap(),
            "release"
        );
    }

    #[test]
    fn the_profile_falls_back_to_debug_only_when_cargo_says_nothing() {
        assert_eq!(resolve_channel(None, None).unwrap(), "debug");
        assert_eq!(resolve_channel(None, Some("  ")).unwrap(), "debug");
    }

    #[test]
    fn an_explicit_preview_is_accepted() {
        assert_eq!(
            resolve_channel(Some("preview"), Some("release")).unwrap(),
            "preview"
        );
    }

    #[test]
    fn every_declared_channel_is_accepted_in_any_case_and_with_padding() {
        for channel in CHANNELS {
            assert_eq!(resolve_channel(Some(channel), None).unwrap(), channel);
            assert_eq!(
                resolve_channel(Some(&channel.to_ascii_uppercase()), None).unwrap(),
                channel
            );
            assert_eq!(
                resolve_channel(Some(&format!(" {channel}\n")), None).unwrap(),
                channel
            );
        }
    }

    #[test]
    fn an_unknown_channel_fails_the_build_rather_than_falling_back() {
        for requested in ["previw", "nightly", "stable", "prod", "release-candidate"] {
            let error = resolve_channel(Some(requested), Some("release"))
                .expect_err("an unknown channel must not resolve");
            assert!(
                error.contains(requested) && error.contains("preview"),
                "the error must name the rejected value and the accepted ones, got {error}"
            );
        }
    }

    #[test]
    fn a_full_revision_is_abbreviated_to_the_fixed_width() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(full.len(), 40);
        assert_eq!(normalize_revision(full).unwrap(), "0123456789ab");
        assert_eq!(
            normalize_revision(&format!("{}\n", full.to_ascii_uppercase())).unwrap(),
            "0123456789ab"
        );
    }

    #[test]
    fn a_revision_that_is_not_a_full_width_hex_string_is_discarded() {
        for raw in ["", "0123456789a", "0123456789ag", "not-a-revision", "  "] {
            assert!(
                normalize_revision(raw).is_none(),
                "{raw:?} must not become a revision"
            );
        }
    }
}
