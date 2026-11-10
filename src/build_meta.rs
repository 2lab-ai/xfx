// The pure rules behind the two build facts every version line reports: which
// channel a binary claims, and which build it is -- its id, the UTC minute it
// was named and the source revision it was compiled from.
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

/// The length of `YYYY-MM-DD-HHMM`, the part of a build id before its revision.
const STAMP_WIDTH: usize = 15;

/// The id of a build that was not given one: the UTC minute its revision was
/// committed, then the revision, `YYYY-MM-DD-HHMM-<12 hex>`.
///
/// The shape a preview is published under (llmux's `<channel> <id>`, the
/// user's choice of 2026-11-10), so a local build and a published one read
/// alike. The committer time rather than the clock, because it is the same on
/// every machine that builds the commit: two builds of one commit are one id.
pub fn build_id_of(committed: i64, revision: &str) -> Option<String> {
    Some(format!(
        "{}-{}",
        utc_minute(committed)?,
        normalize_revision(revision)?
    ))
}

/// The UTC minute of a Unix time in seconds, as `YYYY-MM-DD-HHMM`.
///
/// `None` for a year that is not four digits, so whatever this returns has the
/// one shape [`is_build_id`] reads.
pub fn utc_minute(seconds: i64) -> Option<String> {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    if !(0..=9999).contains(&year) {
        return None;
    }
    let minute = seconds.rem_euclid(86_400) / 60;
    Some(format!(
        "{year:04}-{month:02}-{day:02}-{:02}{:02}",
        minute / 60,
        minute % 60
    ))
}

/// The proleptic Gregorian year, month and day of a count of days since
/// 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`
/// (<https://howardhinnant.github.io/date_algorithms.html#civil_from_days>):
/// move the epoch to 0000-03-01, so that a leap day is the last day of its
/// year, then count 400-year eras, years within the era and days within the
/// year, whose month falls out of a linear fit over the months from March.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// The id to stamp, given an explicit `XFX_BUILD_ID`.
///
/// `Ok(None)` for an absent or blank one: the build then names itself from
/// git. One that is present and is not an id is an error rather than a
/// fallback, as an unknown channel is: the variable exists so that a published
/// binary says which run named it, and a mangled one quietly replaced by
/// another would be a build lying about where it came from.
pub fn resolve_build_id(explicit: Option<&str>) -> Result<Option<String>, String> {
    let Some(given) = explicit.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if is_build_id(given) {
        Ok(Some(given.to_string()))
    } else {
        Err(format!(
            "XFX_BUILD_ID is {given:?}, which is not YYYY-MM-DD-HHMM-<{REVISION_WIDTH} lowercase hex> naming a real UTC minute"
        ))
    }
}

/// Whether `text` is exactly `YYYY-MM-DD-HHMM-<12 lowercase hex>`, its date a
/// real day and its time a real minute.
pub fn is_build_id(text: &str) -> bool {
    let shaped = text.len() == STAMP_WIDTH + 1 + REVISION_WIDTH
        && text.bytes().enumerate().all(|(at, byte)| match at {
            4 | 7 | 10 | STAMP_WIDTH => byte == b'-',
            0..STAMP_WIDTH => byte.is_ascii_digit(),
            _ => matches!(byte, b'0'..=b'9' | b'a'..=b'f'),
        });
    if !shaped {
        return false;
    }
    let number = |from: usize, to: usize| text[from..to].parse::<i64>().unwrap_or(-1);
    let (year, month, day) = (number(0, 4), number(5, 7), number(8, 10));
    let last = match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return false,
    };
    (1..=last).contains(&day) && number(11, 13) < 24 && number(13, 15) < 60
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

    // The Unix times below were read off Python's
    // `datetime.fromisoformat(...).replace(tzinfo=timezone.utc).timestamp()`,
    // an implementation this file shares nothing with.

    #[test]
    fn the_epoch_is_midnight_on_the_first_of_january_1970() {
        assert_eq!(utc_minute(0).unwrap(), "1970-01-01-0000");
        assert_eq!(utc_minute(59).unwrap(), "1970-01-01-0000");
        assert_eq!(utc_minute(60).unwrap(), "1970-01-01-0001");
        assert_eq!(utc_minute(86_399).unwrap(), "1970-01-01-2359");
        assert_eq!(utc_minute(86_400).unwrap(), "1970-01-02-0000");
        // A second before the epoch is the minute before it: the division
        // rounds down, not toward zero.
        assert_eq!(utc_minute(-1).unwrap(), "1969-12-31-2359");
    }

    #[test]
    fn a_leap_day_is_its_own_day_from_0000_to_2359() {
        assert_eq!(utc_minute(951_782_400).unwrap(), "2000-02-29-0000");
        assert_eq!(utc_minute(951_868_799).unwrap(), "2000-02-29-2359");
        assert_eq!(utc_minute(951_868_800).unwrap(), "2000-03-01-0000");
        assert_eq!(utc_minute(1_709_208_000).unwrap(), "2024-02-29-1200");
        // 2100 is divisible by 4 and by 100 and not by 400: no leap day.
        assert_eq!(utc_minute(4_107_542_399).unwrap(), "2100-02-28-2359");
        assert_eq!(utc_minute(4_107_542_400).unwrap(), "2100-03-01-0000");
    }

    #[test]
    fn a_year_ends_at_2359_and_the_next_begins_at_0000() {
        assert_eq!(utc_minute(1_767_225_599).unwrap(), "2025-12-31-2359");
        assert_eq!(utc_minute(1_767_225_600).unwrap(), "2026-01-01-0000");
    }

    #[test]
    fn a_year_that_is_not_four_digits_is_no_minute() {
        assert_eq!(utc_minute(253_402_300_799).unwrap(), "9999-12-31-2359");
        assert!(utc_minute(253_402_300_800).is_none());
        assert!(utc_minute(i64::MIN / 2).is_none());
    }

    #[test]
    fn a_build_id_is_the_commit_minute_then_the_revision() {
        let full = "EC40C7AD2E50a1b2c3d4e5f60718293a4b5c6d7e";
        assert_eq!(
            build_id_of(1_767_225_600, full).unwrap(),
            "2026-01-01-0000-ec40c7ad2e50"
        );
        assert!(build_id_of(1_767_225_600, "ec40c7ad2e5").is_none());
        assert!(build_id_of(253_402_300_800, full).is_none());
    }

    #[test]
    fn every_id_a_commit_gives_is_one_an_override_may_name() {
        let mut seconds = -86_400 * 800;
        while seconds < 4_200_000_000 {
            let id = build_id_of(seconds, "0123456789abcdef").unwrap();
            assert!(is_build_id(&id), "{id}");
            assert_eq!(
                resolve_build_id(Some(&id)).unwrap().as_deref(),
                Some(id.as_str())
            );
            seconds += 86_400 * 7 + 3_661;
        }
    }

    #[test]
    fn an_absent_or_blank_id_leaves_the_build_to_name_itself() {
        for explicit in [None, Some(""), Some("  \n")] {
            assert_eq!(resolve_build_id(explicit).unwrap(), None, "{explicit:?}");
        }
    }

    #[test]
    fn a_given_id_is_taken_as_written_less_its_padding() {
        // The user's example, llmux's preview of 2026-11-10.
        for given in [
            "2026-11-10-0019-ec40c7ad2e50",
            " 2026-11-10-0019-ec40c7ad2e50\n",
        ] {
            assert_eq!(
                resolve_build_id(Some(given)).unwrap().as_deref(),
                Some("2026-11-10-0019-ec40c7ad2e50")
            );
        }
        assert!(is_build_id("2024-02-29-2359-0123456789ab"));
        assert!(is_build_id("2000-02-29-0000-0123456789ab"));
    }

    #[test]
    fn an_id_that_is_not_one_fails_the_build_rather_than_falling_back() {
        for given in [
            "dev",
            "preview",
            "preview-2026-11-10-0019-ec40c7ad2e50",
            "2026-11-10-0019-EC40C7AD2E50",
            "2026-11-10-0019-ec40c7ad2e5",
            "2026-11-10-0019-ec40c7ad2e50a",
            "2026-11-10-0019-ec40c7ad2e5g",
            "2026-11-10-019-ec40c7ad2e500",
            "2026-11-10T0019-ec40c7ad2e50",
            "2026-11-10-0019_ec40c7ad2e50",
            "2026-02-30-0019-ec40c7ad2e50",
            "2026-02-29-0019-ec40c7ad2e50",
            "2100-02-29-0019-ec40c7ad2e50",
            "2026-13-10-0019-ec40c7ad2e50",
            "2026-00-10-0019-ec40c7ad2e50",
            "2026-11-00-0019-ec40c7ad2e50",
            "2026-11-31-0019-ec40c7ad2e50",
            "2026-11-10-2400-ec40c7ad2e50",
            "2026-11-10-0060-ec40c7ad2e50",
            "+026-11-10-0019-ec40c7ad2e50",
        ] {
            let error = resolve_build_id(Some(given)).expect_err("a malformed id must not resolve");
            assert!(
                error.contains(given) && error.contains("YYYY-MM-DD-HHMM"),
                "the error must name the rejected value and the shape, got {error}"
            );
        }
    }
}
