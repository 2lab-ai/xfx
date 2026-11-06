//! Readings in a zone a test gives (upstream's own tests,
//! `local_time.zig:161-216@481104fd`), and readings through the C library in
//! a copy of this test binary started with its own `TZ`: the parent's
//! environment is never set, so no test races another's read of it.

use std::ffi::OsStr;
use std::time::Duration;

use super::*;

/// 2024-01-02 03:04:05.006 UTC, upstream's instant.
const AT: i64 = 1_704_164_645_006;
const IST: Zone<'static> = Zone {
    offset_s: 5 * 3600 + 30 * 60,
    name: b"IST",
};

fn shown(reading: Option<Reading>) -> String {
    reading.expect("a reading").to_string()
}

#[test]
fn a_reading_is_the_moment_in_the_given_zone() {
    assert_eq!(shown(in_zone(AT, Zone::UTC)), "2024-01-02 03:04:05.006 UTC");
    // East of UTC rolls the clock forward.
    assert_eq!(shown(in_zone(AT, IST)), "2024-01-02 08:34:05.006 IST");
    // West of UTC crosses back into the previous day and year.
    assert_eq!(
        shown(in_zone(
            1_704_078_245_006,
            Zone {
                offset_s: -5 * 3600,
                name: b"EST",
            }
        )),
        "2023-12-31 22:04:05.006 EST"
    );
}

#[test]
fn a_zone_that_cannot_apply_reads_in_utc() {
    // Shifting back from the epoch would leave the calendar.
    let pdt = Zone {
        offset_s: -7 * 3600,
        name: b"PDT",
    };
    assert_eq!(shown(in_zone(2, pdt)), "1970-01-01 00:00:00.002 UTC");
    // Shifting forward past 9999 would too.
    let ahead = Zone {
        offset_s: 14 * 3600,
        name: b"LINT",
    };
    assert_eq!(
        shown(in_zone(MAX_TIMESTAMP_MS, ahead)),
        "9999-12-31 23:59:59.999 UTC"
    );
    // An offset of a day or more, either way, and the extremes.
    for offset_s in [86_400, -86_400, 90_000, i32::MAX, i32::MIN] {
        let zone = Zone {
            offset_s,
            name: b"BAD",
        };
        assert_eq!(
            shown(in_zone(AT, zone)),
            "2024-01-02 03:04:05.006 UTC",
            "{offset_s}"
        );
    }
    // A day less a second still applies.
    let almost = Zone {
        offset_s: 86_399,
        name: b"",
    };
    assert_eq!(shown(in_zone(AT, almost)), "2024-01-03 03:04:04.006 +2359");
    // No reading at all outside 1970 through 9999.
    assert_eq!(in_zone(-1, Zone::UTC), None);
    assert_eq!(in_zone(MAX_TIMESTAMP_MS + 1, Zone::UTC), None);
    assert_eq!(local(-1), None);
    assert_eq!(local(i64::MIN), None);
    assert_eq!(local(MAX_TIMESTAMP_MS + 1), None);
    assert_eq!(local(i64::MAX), None);
}

#[test]
fn a_zone_without_a_plain_name_shows_its_offset() {
    let at = |offset_s, name: &'static [u8]| shown(in_zone(AT, Zone { offset_s, name }));
    // Nepal's: tzdata names it by its offset, itself a plain token.
    assert_eq!(
        at(5 * 3600 + 45 * 60, b"+0545"),
        "2024-01-02 08:49:05.006 +0545"
    );
    assert_eq!(at(IST.offset_s, b""), "2024-01-02 08:34:05.006 +0530");
    assert_eq!(
        at(-(9 * 3600 + 30 * 60), b"\x1b[2J"),
        "2024-01-01 17:34:05.006 -0930"
    );
    assert_eq!(at(0, b"ABCDEFGHIJK"), "2024-01-02 03:04:05.006 +0000");
    // Ten is the most shown.
    assert_eq!(at(0, b"ABCDEFGHIJ"), "2024-01-02 03:04:05.006 ABCDEFGHIJ");
}

/// The abbreviations a zone file or a `TZ` string could carry to the terminal
/// had they been printed as given: each reads as its offset.
fn hostile_names() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("ESC, a CSI erase", b"\x1b[2J".to_vec()),
        ("ESC, a CSI colour", b"\x1b[31mX".to_vec()),
        ("an OSC title", b"\x1b]0;pwn\x07".to_vec()),
        (
            "an OSC 8 link",
            b"\x1b]8;;http://x\x1b\\L\x1b]8;;\x1b\\".to_vec(),
        ),
        ("a C1 CSI, in UTF-8", b"\xc2\x9b2J".to_vec()),
        ("a bare C1 CSI", b"\x9b2J".to_vec()),
        ("a bare C1 OSC", b"\x9d0;x\x9c".to_vec()),
        ("a bell", b"IST\x07".to_vec()),
        ("a DEL", b"IST\x7f".to_vec()),
        ("a NUL inside", b"IS\0T".to_vec()),
        ("non-ASCII", "KST\u{d55c}".as_bytes().to_vec()),
        ("Latin-1", b"K\xe9T".to_vec()),
        ("a space", b"A B".to_vec()),
        ("an underscore (macOS's scrubbed ESC)", b"__2J".to_vec()),
        ("eleven bytes", b"ABCDEFGHIJK".to_vec()),
        ("300 bytes", vec![b'A'; 300]),
    ]
}

/// The cases of `cases` whose reading is not `expected`, each with what it
/// read instead: all of them, so a guard taken away shows every case it held.
fn leaks(cases: Vec<(&'static str, Result<String, String>)>, expected: &str) -> Vec<String> {
    cases
        .into_iter()
        .filter(|(_, read)| read.as_deref() != Ok(expected))
        .map(|(what, read)| format!("{what}: {read:?}"))
        .collect()
}

#[test]
fn a_hostile_abbreviation_never_reaches_the_screen() {
    let read = hostile_names()
        .into_iter()
        .map(|(what, name)| {
            let zone = Zone {
                offset_s: IST.offset_s,
                name: &name,
            };
            let read = std::panic::catch_unwind(|| in_zone(AT, zone).map(|r| r.to_string()));
            let read = match read {
                Ok(Some(text)) => Ok(text),
                Ok(None) => Err("no reading".to_string()),
                Err(_) => Err("panicked".to_string()),
            };
            (what, read)
        })
        .collect();
    let leaked = leaks(read, "2024-01-02 08:34:05.006 +0530");
    assert!(leaked.is_empty(), "{leaked:#?}");
}

#[test]
fn no_offset_or_moment_makes_a_reading_fail() {
    let long = [b'A'; 300];
    let names: [&[u8]; 5] = [b"", b"UTC", b"\x1b[2J", &long, b"ABCDEFGHIJ"];
    let offsets = [
        i32::MIN,
        i32::MIN + 1,
        -86_400,
        -86_399,
        -1,
        0,
        1,
        59,
        86_399,
        86_400,
        i32::MAX,
    ];
    let moments = [
        0,
        1,
        999,
        1000,
        86_399_999,
        AT,
        MAX_TIMESTAMP_MS - 86_400_000,
        MAX_TIMESTAMP_MS - 1,
        MAX_TIMESTAMP_MS,
    ];
    for name in names {
        for offset_s in offsets {
            for at in moments {
                let reading = in_zone(at, Zone { offset_s, name }).expect("a reading");
                let text = reading.to_string();
                let zone = reading.zone_name();
                assert!(is_plain_zone_name(zone.as_bytes()), "{text:?}");
                assert_eq!(text.len(), 24 + zone.len(), "{text:?}");
                assert!((1970..=9999).contains(&reading.year), "{text:?}");
                assert!((1..=12).contains(&reading.month), "{text:?}");
                assert!((1..=31).contains(&reading.day), "{text:?}");
            }
        }
    }
}

#[test]
fn a_reading_follows_the_test_zone() {
    assert_eq!(shown(local(AT)), "2024-01-02 03:04:05.006 UTC");
    with_test_zone(Some(IST), || {
        assert_eq!(shown(local(AT)), "2024-01-02 08:34:05.006 IST");
    });
    assert_eq!(shown(local(AT)), "2024-01-02 03:04:05.006 UTC");
}

// ---------------------------------------------------------------------------
// Through the C library, in a copy of this binary with its own `TZ`
// ---------------------------------------------------------------------------

/// The moments a copy of this test binary reads, comma-separated.
const MOMENTS: &str = "XFX_TEST_LOCAL_TIME_MOMENTS";
const READER: &str = "local_time::tests::a_copy_of_this_binary_reads_its_zone";

/// Does nothing in a normal run. In a copy started by [`read_in`], reads each
/// moment `MOMENTS` names through the C library, in the zone its `TZ` says,
/// and prints what a screen would show, escaped.
#[test]
fn a_copy_of_this_binary_reads_its_zone() {
    let Some(moments) = std::env::var_os(MOMENTS) else {
        return;
    };
    let moments = moments.into_string().expect("moments");
    with_test_zone(None, || {
        for at in moments.split(',') {
            let at: i64 = at.parse().expect("a moment");
            let text = local(at).map_or_else(|| "none".to_string(), |r| r.to_string());
            println!("\nreading={}=", text.escape_default());
        }
    });
}

/// What each of `moments` reads as under `TZ=tz`, through the C library, in
/// a copy of this test binary given that `TZ` and nothing else new.
fn read_in(tz: impl AsRef<OsStr>, moments: &[i64]) -> Vec<String> {
    try_read_in(tz, moments).unwrap_or_else(|report| panic!("{report}"))
}

/// [`read_in`], or what the copy printed when it failed.
fn try_read_in(tz: impl AsRef<OsStr>, moments: &[i64]) -> Result<Vec<String>, String> {
    let mut copy = crate::process::command(std::env::current_exe().expect("this test binary"));
    let joined = moments
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    copy.env("TZ", tz.as_ref()).env(MOMENTS, joined).args([
        "--exact",
        "--test-threads=1",
        "--nocapture",
        READER,
    ]);
    let output = crate::process::output_within(&mut copy, Duration::from_secs(60))
        .expect("a copy of this test binary");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        let panic = report
            .lines()
            .skip_while(|line| !line.contains("panicked at"))
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        return Err(format!("the copy failed: {panic}"));
    }
    let readings: Vec<String> = report
        .lines()
        .filter_map(|line| line.strip_prefix("reading="))
        .filter_map(|rest| rest.strip_suffix('='))
        .map(str::to_string)
        .collect();
    assert_eq!(readings.len(), moments.len(), "{report}");
    Ok(readings)
}

/// A version-1 zone file (RFC 8536 3) of one type: `offset_s` and `name`, in
/// effect at every moment.
fn zone_file(name: &[u8], offset_s: i32) -> Vec<u8> {
    let mut file = b"TZif\0".to_vec();
    file.extend([0; 15]);
    // isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt.
    for count in [0, 0, 0, 0, 1, name.len() + 1] {
        file.extend(u32::try_from(count).expect("a count").to_be_bytes());
    }
    file.extend(offset_s.to_be_bytes());
    file.extend([0, 0]); // isdst, the name's index
    file.extend(name);
    file.push(0);
    file
}

#[test]
fn the_c_library_reads_daylight_saving_time_on_each_moments_own_date() {
    // America/New_York's rule as a POSIX string, so no zone files are needed:
    // the last millisecond before each change, and the change.
    assert_eq!(
        read_in(
            "EST5EDT,M3.2.0,M11.1.0",
            &[
                1_710_053_999_999,
                1_710_054_000_000,
                1_730_613_599_999,
                1_730_613_600_000,
                AT,
            ]
        ),
        [
            "2024-03-10 01:59:59.999 EST",
            "2024-03-10 03:00:00.000 EDT",
            "2024-11-03 01:59:59.999 EDT",
            "2024-11-03 01:00:00.000 EST",
            "2024-01-01 22:04:05.006 EST",
        ]
    );
    // Australia/Sydney's, the other way round the year.
    assert_eq!(
        read_in(
            "AEST-10AEDT,M10.1.0,M4.1.0/3",
            &[
                1_712_419_199_999,
                1_712_419_200_000,
                1_728_143_999_999,
                1_728_144_000_000,
            ]
        ),
        [
            "2024-04-07 02:59:59.999 AEDT",
            "2024-04-07 02:00:00.000 AEST",
            "2024-10-06 01:59:59.999 AEST",
            "2024-10-06 03:00:00.000 AEDT",
        ]
    );
}

#[test]
fn the_c_library_reads_a_zone_without_a_usable_abbreviation_as_its_offset() {
    // Nepal's, as tzdata names it: by its offset.
    assert_eq!(
        read_in("<+0545>-5:45", &[AT]),
        ["2024-01-02 08:49:05.006 +0545"]
    );
    // A name past ten bytes.
    assert_eq!(
        read_in("<ABCDEFGHIJK>-5:30", &[AT]),
        ["2024-01-02 08:34:05.006 +0530"]
    );
    // A zone file whose name is empty.
    let dir = tempfile::tempdir().expect("a directory");
    let path = dir.path().join("empty");
    std::fs::write(&path, zone_file(b"", IST.offset_s)).expect("a zone file");
    assert_eq!(read_in(&path, &[AT]), ["2024-01-02 08:34:05.006 +0530"]);
}

#[test]
fn the_c_library_reads_a_zone_that_cannot_apply_in_utc() {
    // Shifted out of 1970 through 9999.
    assert_eq!(read_in("PST8", &[2]), ["1970-01-01 00:00:00.002 UTC"]);
    assert_eq!(
        read_in("<+14>-14", &[MAX_TIMESTAMP_MS]),
        ["9999-12-31 23:59:59.999 UTC"]
    );
    // An offset of a day or more, from a string and from zone files.
    assert_eq!(read_in("<XX>-25", &[AT]), ["2024-01-02 03:04:05.006 UTC"]);
    let dir = tempfile::tempdir().expect("a directory");
    for offset_s in [90_000, -90_000, i32::MAX] {
        let path = dir.path().join(format!("day{offset_s}"));
        std::fs::write(&path, zone_file(b"DAY", offset_s)).expect("a zone file");
        assert_eq!(
            read_in(&path, &[AT]),
            ["2024-01-02 03:04:05.006 UTC"],
            "{offset_s}"
        );
    }
    // And nothing at all outside the range.
    assert_eq!(
        read_in("IST-5:30", &[-1, MAX_TIMESTAMP_MS + 1]),
        ["none", "none"]
    );
}

#[test]
fn a_hostile_zone_file_never_reaches_the_screen() {
    let dir = tempfile::tempdir().expect("a directory");
    let mut long = vec![b'A'; 40];
    long.extend(b"\x1b[2J");
    let mut cases = hostile_names();
    // macOS cuts a name past 16 bytes at 16; glibc keeps it (the copy here
    // keeps 11).
    cases.push(("forty bytes and an ESC", long));
    let read = cases
        .into_iter()
        .enumerate()
        // A NUL ends a zone file's name, and macOS refuses a file whose names
        // pass 50 bytes: those two are read without the C library above, and
        // the long one below.
        .filter(|(_, (_, name))| !name.contains(&0) && name.len() <= 50)
        .map(|(at, (what, name))| {
            let path = dir.path().join(format!("hostile{at}"));
            std::fs::write(&path, zone_file(&name, IST.offset_s)).expect("a zone file");
            (what, try_read_in(&path, &[AT]).map(|read| read.concat()))
        })
        .collect();
    let leaked = leaks(read, "2024-01-02 08:34:05.006 +0530");
    assert!(leaked.is_empty(), "{leaked:#?}");
    // 300 bytes: macOS refuses the file and reads UTC; a library that takes
    // it gives eleven bytes of it here, too many.
    let path = dir.path().join("long300");
    std::fs::write(&path, zone_file(&[b'A'; 300], IST.offset_s)).expect("a zone file");
    let [reading] = <[String; 1]>::try_from(read_in(&path, &[AT])).expect("one reading");
    assert!(
        reading == "2024-01-02 08:34:05.006 +0530" || reading == "2024-01-02 03:04:05.006 UTC",
        "{reading}"
    );
}
