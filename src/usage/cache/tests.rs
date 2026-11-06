//! The usage cache's tests: fx's own (`src/usage/cache.zig@481104fd`, with
//! the cache side of `usage.zig`'s and `policy.zig`'s), then the five security
//! properties of a write under the profile.
//!
//! Every file is written under a `tempfile` directory -- the run's `TMPDIR` --
//! passed as the home; nothing here reads `HOME`, and a copy of this binary
//! started as a second process is given that same directory as `HOME`.

use super::*;
use crate::usage::report::{Budget as ReportBudget, RefreshPeriod};
use std::io::Read as _;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const HOUR_MS: i64 = 60 * 60 * 1000;
const MIN_MS: i64 = 60 * 1000;

/// A wall clock on the day after the fixtures' day 5 (`cache.zig:303-304`).
const TEST_NOW: i64 = 6 * DAY_MS;

/// The fake key. Its shape matches no credential pattern
/// (`scripts/check-no-secrets.sh`).
const KEY: &str = "xfx-usage-cache-test-key-0123456789-must-not-reach-disk";

fn one_row(cost: f64) -> Vec<ModelRow> {
    vec![ModelRow {
        model: "anthropic/claude-opus-5.5".to_string(),
        totals: Totals {
            request_count: 2,
            total_cost: cost,
            ..Totals::default()
        },
    }]
}

fn today(day: i64, at_ms: i64, cost: f64) -> Option<Today> {
    Some(Today {
        day,
        at_ms,
        models: one_row(cost),
    })
}

fn finished(day: i64, at_ms: i64, days_7: f64, month: f64) -> Option<Finished> {
    Some(Finished {
        day,
        at_ms,
        days_7: one_row(days_7),
        month: one_row(month),
    })
}

fn budget(at_ms: i64, limit: f64, spent: f64) -> Option<Budget> {
    Some(Budget {
        at_ms,
        value: ReportBudget {
            limit,
            spent,
            period: RefreshPeriod::Monthly,
        },
    })
}

fn small_fetch() -> Entry {
    Entry {
        today: today(5, 1, 1.0),
        ..Entry::default()
    }
}

/// A fresh home under the run's `TMPDIR`, with `~/.xfx` already there, as a
/// profile normally is.
fn home() -> (tempfile::TempDir, Home) {
    let dir = tempfile::tempdir().expect("a home");
    std::fs::create_dir(dir.path().join(".xfx")).expect("a profile");
    let home = Home::new(Some(dir.path())).expect("an absolute home");
    (dir, home)
}

fn cache_dir(root: &Path) -> PathBuf {
    root.join(".xfx").join(DIR_NAME)
}

fn file(root: &Path, id: &Identity, suffix: &str) -> PathBuf {
    cache_dir(root).join(format!("{}{suffix}", id.as_str()))
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("a directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 name")
        })
        .collect();
    names.sort();
    names
}

fn mode_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .expect("an entry")
        .permissions()
        .mode()
        & 0o777
}

fn api_key_id() -> Identity {
    Identity::of(API_KEY_SCOPE, "k", "")
}

// -- fx's tests ---------------------------------------------------------------

#[test]
fn merge_keeps_the_newer_part_of_each_half() {
    // `cache.zig:310-325@481104fd`.
    let old = Entry {
        today: today(10, 200, 2.0),
        finished: finished(10, 100, 7.0, 30.0),
        budget: budget(200, 100.0, 5.0),
    };
    // An older fetch arrives late: nothing of it may replace newer data.
    let late = merge(
        old.clone(),
        Entry {
            today: today(10, 150, 1.0),
            ..Entry::default()
        },
        11 * DAY_MS,
    );
    assert_eq!(late.today.as_ref().unwrap().at_ms, 200);
    assert_eq!(
        late.finished.as_ref().unwrap().days_7[0].totals.total_cost,
        7.0
    );
    // A newer fetch with only today keeps the stored finished days and budget.
    let fresh = merge(
        old,
        Entry {
            today: today(10, 300, 3.0),
            ..Entry::default()
        },
        11 * DAY_MS,
    );
    assert_eq!(
        fresh.today.as_ref().unwrap().models[0].totals.total_cost,
        3.0
    );
    assert_eq!(fresh.finished.as_ref().unwrap().day, 10);
    assert_eq!(fresh.budget.as_ref().unwrap().value.spent, 5.0);
}

#[test]
fn the_cache_file_round_trips_and_rejects_other_identities() {
    // `cache.zig:327-345@481104fd`.
    let a = Identity::of("member", "acct_1", "team_1");
    let b = Identity::of("api_key", "acct_1", "team_1");
    assert_ne!(a, b);
    let entry = Entry {
        today: today(20737, 1, 1.5),
        budget: budget(1, 31600.0, 6596.55),
        ..Entry::default()
    };
    let bytes = encode(&a, &entry).expect("bytes");
    assert!(!String::from_utf8_lossy(&bytes).contains("acct_1"));
    let decoded = decode(&a, &bytes).expect("an entry");
    assert_eq!(
        decoded.today.as_ref().unwrap().models[0].model,
        "anthropic/claude-opus-5.5"
    );
    assert_eq!(
        decoded.budget.as_ref().unwrap().value.period,
        RefreshPeriod::Monthly
    );
    assert_eq!(decoded, entry);
    assert_eq!(decode(&b, &bytes), None);
    assert_eq!(decode(&a, br#"{"schema_version":2}"#), None);
    assert_eq!(decode(&a, b"garbage"), None);
}

#[test]
fn the_file_is_written_in_fxs_schema() {
    // `Wire`, `cache.zig:172-195@481104fd`: fx's field order, and every
    // absent part, its `codex` and `grok` among them, written `null`.
    let id = api_key_id();
    let bytes = encode(&id, &small_fetch()).expect("bytes");
    let text = String::from_utf8(bytes).expect("UTF-8");
    let expected = format!(
        concat!(
            r#"{{"schema_version":2,"identity":"{}","today":{{"day":5,"at_ms":1,"models":[{{"#,
            r#""model":"anthropic/claude-opus-5.5","totals":{{"request_count":2,"input_tokens":0,"#,
            r#""output_tokens":0,"cache_read_tokens":0,"cache_write_tokens":0,"reasoning_tokens":0,"#,
            r#""total_cost":1.0,"cache_hit_requests":0.0,"failed_requests":0.0,"ttft_weighted":0.0,"#,
            r#""ttft_requests":0,"tps_weighted":0.0,"tps_requests":0}}}}]}},"finished":null,"#,
            r#""budget":null,"codex":null,"grok":null}}"#
        ),
        id.as_str()
    );
    assert_eq!(text, expected);
}

#[test]
fn a_decoded_entry_outlives_the_bytes_it_came_from() {
    // `cache.zig:347-355@481104fd`.
    let id = Identity::of(API_KEY_SCOPE, "k", "");
    let mut encoded = encode(
        &id,
        &Entry {
            today: today(1, 1, 1.0),
            ..Entry::default()
        },
    )
    .expect("bytes");
    let decoded = decode(&id, &encoded).expect("an entry");
    encoded.fill(b'x');
    drop(encoded);
    assert_eq!(
        decoded.today.unwrap().models[0].model,
        "anthropic/claude-opus-5.5"
    );
}

#[test]
fn store_merges_under_the_lock_and_never_regresses_today() {
    // `cache.zig:357-376@481104fd`.
    let (_root, home) = home();
    let id = Identity::of("member", "acct_1", "team_1");
    store(
        &home,
        &id,
        Entry {
            today: today(5, 500, 5.0),
            finished: finished(5, 500, 7.0, 30.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the first store");
    let second = store(
        &home,
        &id,
        Entry {
            today: today(5, 400, 4.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the second store");
    assert_eq!(second.today.as_ref().unwrap().at_ms, 500);
    assert_eq!(second.finished.as_ref().unwrap().day, 5);
    let again = read(&home, &id).expect("the stored entry");
    assert_eq!(again.today.unwrap().models[0].totals.total_cost, 5.0);
}

#[test]
fn a_part_stored_while_the_clock_ran_ahead_yields_to_the_next_fetch() {
    // `cache.zig:378-416@481104fd`.
    let day: i64 = 20737;
    let at = day * DAY_MS + 3 * HOUR_MS;
    let tomorrow = at + DAY_MS;
    let ahead = Entry {
        today: today(day + 1, tomorrow, 6.0),
        finished: finished(day + 1, tomorrow, 7.0, 30.0),
        budget: budget(tomorrow, 100.0, 60.0),
    };
    let fetched = Entry {
        today: today(day, at, 3.0),
        finished: finished(day, at, 70.0, 300.0),
        budget: None,
    };
    // Corrected back a day: what was stored for tomorrow yields.
    let corrected = merge(ahead.clone(), fetched.clone(), at + MIN_MS);
    assert_eq!(corrected.today.as_ref().unwrap().at_ms, at);
    assert_eq!(corrected.finished.as_ref().unwrap().day, day);
    assert!(corrected.budget.is_none());
    // Once the clock passes it, the stored parts are simply newer.
    let later = merge(ahead.clone(), fetched.clone(), tomorrow + MIN_MS);
    assert_eq!(later.today.as_ref().unwrap().at_ms, tomorrow);
    assert_eq!(later.finished.as_ref().unwrap().day, day + 1);
    assert_eq!(later.budget.as_ref().unwrap().value.spent, 60.0);
    // A small correction within a day drops today but keeps the finished
    // days, which do not change during the day.
    let same_day = Entry {
        today: today(day, at + 10 * MIN_MS, 4.0),
        finished: finished(day, at + 10 * MIN_MS, 7.0, 30.0),
        budget: None,
    };
    let small = merge(
        same_day,
        Entry {
            today: today(day, at, 3.0),
            ..Entry::default()
        },
        at + MIN_MS,
    );
    assert_eq!(small.today.as_ref().unwrap().at_ms, at);
    assert_eq!(
        small.finished.as_ref().expect("kept").days_7[0]
            .totals
            .total_cost,
        7.0
    );
    // A fetch that started tomorrow by the old clock keeps its finished days
    // out; its today is stored and yields to the next fetch.
    let straddling = merge(fetched, ahead, at + MIN_MS);
    assert_eq!(straddling.finished.as_ref().unwrap().day, day);
    assert_eq!(straddling.today.as_ref().unwrap().at_ms, tomorrow);
}

#[test]
fn a_finished_day_is_never_replaced_by_an_earlier_one() {
    // `newer` by day (`cache.zig:108,158-170@481104fd`): a late fetch of
    // yesterday's finished days does not replace today's.
    let stored = Entry {
        finished: finished(5, 500, 7.0, 30.0),
        ..Entry::default()
    };
    let late = merge(
        stored,
        Entry {
            finished: finished(4, 900, 1.0, 1.0),
            ..Entry::default()
        },
        TEST_NOW,
    );
    assert_eq!(late.finished.as_ref().unwrap().day, 5);
    assert_eq!(late.finished.unwrap().days_7[0].totals.total_cost, 7.0);
}

#[test]
fn a_store_never_keeps_less_than_the_caller_holds() {
    // `cache.zig:418-459@481104fd`.
    let (root, home) = home();
    let id = api_key_id();
    let first = store(
        &home,
        &id,
        Entry {
            finished: finished(5, 500, 7.0, 30.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the first store");
    let path = file(root.path(), &id, ".json");
    std::fs::write(&path, "garbage").expect("an unreadable file");
    // The fetch skipped the finished days because the caller already had them.
    let second = store(
        &home,
        &id,
        Entry {
            today: today(5, 600, 4.0),
            ..Entry::default()
        },
        TEST_NOW,
        Some(&first),
    )
    .expect("the second store");
    assert_eq!(second.finished.as_ref().expect("held").day, 5);
    let kept = read(&home, &id).expect("the stored entry");
    assert_eq!(kept.finished.as_ref().unwrap().day, 5);
    assert_eq!(kept.today.as_ref().unwrap().at_ms, 600);

    // A readable file with less than the caller holds (a store that started
    // over from an unreadable file, with yesterday's finished days) does not
    // take the caller's finished days either.
    let older = encode(
        &id,
        &Entry {
            finished: finished(4, 400, 1.0, 1.0),
            ..Entry::default()
        },
    )
    .expect("bytes");
    std::fs::write(&path, older).expect("an older file");
    let third = store(
        &home,
        &id,
        Entry {
            today: today(5, 700, 5.0),
            ..Entry::default()
        },
        TEST_NOW,
        Some(&second),
    )
    .expect("the third store");
    assert_eq!(third.finished.as_ref().expect("held").day, 5);
}

#[test]
fn store_never_writes_an_entry_its_reader_would_reject() {
    // `cache.zig:461-481@481104fd`; fx's `own` is `Entry::valid` here.
    let (root, home) = home();
    let id = api_key_id();
    store(
        &home,
        &id,
        Entry {
            finished: finished(5, 500, 7.0, 30.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the first store");
    let before = std::fs::read(file(root.path(), &id, ".json")).expect("the file");
    let huge = Entry {
        today: Some(Today {
            day: 5,
            at_ms: 600,
            models: vec![ModelRow {
                model: "m".to_string(),
                totals: Totals {
                    request_count: 1,
                    total_cost: f64::INFINITY,
                    ..Totals::default()
                },
            }],
        }),
        ..Entry::default()
    };
    assert!(matches!(
        store(&home, &id, huge.clone(), TEST_NOW, None),
        Err(StoreError::Invalid)
    ));
    assert!(!huge.valid());
    let kept = read(&home, &id).expect("the stored entry");
    assert_eq!(kept.finished.as_ref().unwrap().day, 5);
    assert!(kept.today.is_none());
    assert_eq!(
        std::fs::read(file(root.path(), &id, ".json")).expect("the file"),
        before
    );
}

#[test]
fn finished_days_are_read_once_per_utc_day() {
    // The cache side of `policy.zig:133-143@481104fd`: `plan` reads the
    // finished days only when the cache lacks today's copy.
    let now: i64 = 1_791_676_800_000 + 3 * HOUR_MS; // 2026-10-11 03:00 UTC
    let day = utc_day(now);
    assert!(finished_due(None, now));
    assert!(finished_due(Some(&Entry::default()), now));
    let yesterdays = Entry {
        finished: finished(day - 1, now - DAY_MS, 1.0, 1.0),
        ..Entry::default()
    };
    assert!(finished_due(Some(&yesterdays), now));
    let todays = Entry {
        finished: finished(day, now, 1.0, 1.0),
        ..Entry::default()
    };
    assert!(!finished_due(Some(&todays), now));
    // A day later they are due again.
    assert!(finished_due(Some(&todays), now + DAY_MS));
}

#[test]
fn a_first_store_creates_a_private_profile() {
    // The cache side of `usage.zig:1165-1182@481104fd`: a missing `~/.xfx` is
    // created `0700` for the cache.
    let root = tempfile::tempdir().expect("a home");
    let home = Home::new(Some(root.path())).expect("an absolute home");
    store(&home, &api_key_id(), small_fetch(), TEST_NOW, None).expect("a store");
    let profile = std::fs::metadata(root.path().join(".xfx")).expect("a profile");
    assert!(profile.is_dir());
    assert_eq!(profile.permissions().mode() & 0o777, 0o700);
}

#[test]
fn a_cache_directory_that_is_a_file_refuses_the_store() {
    // The cache side of `usage.zig:1514-1532@481104fd`: the store fails, so
    // the dashboard (U3) shows the fetch from memory.
    let (root, home) = home();
    std::fs::write(cache_dir(root.path()), "not a directory").expect("a file");
    assert!(store(&home, &api_key_id(), small_fetch(), TEST_NOW, None).is_err());
    assert_eq!(read(&home, &api_key_id()), None);
    assert_eq!(
        std::fs::read(cache_dir(root.path())).expect("the file"),
        b"not a directory"
    );
}

#[test]
fn a_fixture_seeded_for_fx_seeds_this_cache() {
    // `p5-usage-mkcache.py` (research/tui-phase5) writes fx's identity for
    // the capture harness's key in Python's JSON: integers where fx writes
    // numbers, and no `budget`, `codex` or `grok` key. The same file under
    // `~/.xfx/usage-cache/` is read the same, so U4's pairs can seed both.
    let id = Identity::of(API_KEY_SCOPE, "fake-key-not-real", "");
    assert_eq!(
        id.as_str(),
        "c5482018bc3612f715c33c5a278e44485d3bc741118d70cfeff2c0aa3f94fc4e"
    );
    let seeded = format!(
        concat!(
            r#"{{"schema_version": 2, "identity": "{}", "today": {{"day": 20738, "at_ms": 1791763200000, "#,
            r#""models": [{{"model": "fake/model-a", "totals": {{"request_count": 3, "#,
            r#""input_tokens": 12000, "output_tokens": 800, "cache_read_tokens": 4000, "#,
            r#""cache_write_tokens": 0, "reasoning_tokens": 0, "total_cost": 0.0125, "#,
            r#""cache_hit_requests": 0.999, "failed_requests": 0.0, "ttft_weighted": 2760, "#,
            r#""ttft_requests": 3, "tps_weighted": 264, "tps_requests": 3}}}}]}}, "#,
            r#""finished": {{"day": 20738, "at_ms": 1791763200000, "days_7": [], "month": []}}}}"#
        ),
        id.as_str()
    );
    let entry = decode(&id, seeded.as_bytes()).expect("the seeded entry");
    let row = &entry.today.as_ref().unwrap().models[0];
    assert_eq!(row.totals.total_tokens(), 16800);
    assert_eq!(row.totals.p90_ttft_ms(), Some(920.0));
    assert!(entry.budget.is_none());
}

#[test]
fn a_question_names_its_cache_entry_by_fxs_api_key_identity() {
    // `usage_host.zig:105-111` with `usage.zig:422-427@481104fd`: scope
    // `api_key`, the key as subject, no team.
    let workspace = tempfile::tempdir().expect("a workspace");
    let config = crate::config::RuntimeConfig::load_with(
        &crate::config::Environment::new(
            Some(workspace.path().to_path_buf()),
            [("AI_GATEWAY_API_KEY".to_string(), KEY.to_string())]
                .into_iter()
                .collect(),
        ),
        workspace.path(),
    )
    .expect("a configuration");
    let endpoint = crate::gateway::Endpoint::resolve(None).expect("the default endpoint");
    let question = crate::usage::report::question(&config, &endpoint).expect("a key is asked");
    let id = question.identity();
    assert_eq!(id, Identity::of("api_key", KEY, ""));
    assert_ne!(id, Identity::of("api_key", "another key", ""));
    assert!(!format!("{id:?}").contains(KEY));
}

#[test]
fn no_absolute_home_means_no_cache() {
    // fx's `openHome` (`usage.zig:524-529@481104fd`, 69a01a40): an unset,
    // empty or relative home is none, so nothing can be read or written
    // relative to the directory xfx started in.
    for relative in ["", "relative", "./home", "../home", "~/home", "home/.xfx"] {
        assert_eq!(Home::new(Some(Path::new(relative))), None, "{relative:?}");
    }
    assert_eq!(Home::new(None), None);
    assert!(Home::new(Some(Path::new("/an/absolute/home"))).is_some());

    // And as the configuration hands it over: a relative `HOME` makes a
    // relative profile, and still no cache.
    let workspace = tempfile::tempdir().expect("a workspace");
    let config = crate::config::RuntimeConfig::load_with(
        &crate::config::Environment::new(Some(PathBuf::from("relative-home")), Default::default()),
        workspace.path(),
    )
    .expect("a configuration");
    assert_eq!(config.home(), Some(Path::new("relative-home")));
    assert_eq!(Home::new(config.home()), None);
    assert!(!Path::new("relative-home").exists());
}

// -- S1: the key never reaches the disk ---------------------------------------

/// Every way the key could be spelled in bytes: as is, in hex either case, and
/// in base64 (standard and URL-safe) at each of the three byte alignments, by
/// the run of characters that encodes only the key's own bytes.
fn key_spellings(key: &str) -> Vec<String> {
    let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
    let mut spellings = vec![key.to_string(), hex.to_uppercase(), hex];
    for prefix in ["", "x", "xx"] {
        let encoded = base64(format!("{prefix}{key}").as_bytes());
        let interior = &encoded[4..encoded.len() - 4];
        spellings.push(interior.to_string());
        spellings.push(interior.replace('+', "-").replace('/', "_"));
    }
    spellings
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize])
            } else {
                '='
            });
        }
    }
    out
}

/// The spelling of the key `bytes` holds, if any.
fn spelled_in<'a>(bytes: &[u8], spellings: &'a [String]) -> Option<&'a str> {
    let text = String::from_utf8_lossy(bytes);
    spellings
        .iter()
        .find(|spelling| text.contains(spelling.as_str()))
        .map(String::as_str)
}

/// Every entry under `root`, links not followed, with each file's bytes.
fn everything_under(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            let meta = std::fs::symlink_metadata(&path).expect("metadata");
            let bytes = if meta.is_file() {
                std::fs::read(&path).expect("a file")
            } else {
                Vec::new()
            };
            if meta.is_dir() {
                pending.push(path.clone());
            }
            found.push((path, bytes));
        }
    }
    found
}

#[test]
fn the_key_never_reaches_the_disk() {
    let spellings = key_spellings(KEY);
    // The control: the scan finds the key however it is embedded.
    for prefix in ["", "a", "ab", "abc"] {
        let embedded = format!("{prefix}{KEY}-");
        let hex: String = embedded.bytes().map(|b| format!("{b:02X}")).collect();
        for sample in [embedded.clone(), base64(embedded.as_bytes()), hex] {
            assert!(
                spelled_in(format!("<{sample}>").as_bytes(), &spellings).is_some(),
                "the scan missed {sample}"
            );
        }
    }

    let (root, home) = home();
    let id = Identity::of(API_KEY_SCOPE, KEY, "");
    let now = 20737 * DAY_MS + HOUR_MS;
    let first = store(
        &home,
        &id,
        Entry {
            today: today(20737, now, 1.5),
            finished: finished(20737, now, 7.0, 30.0),
            budget: budget(now, 100.0, 41.0),
        },
        now,
        None,
    )
    .expect("a store");
    store(
        &home,
        &id,
        Entry {
            today: today(20737, now + MIN_MS, 2.5),
            ..Entry::default()
        },
        now + MIN_MS,
        Some(&first),
    )
    .expect("a second store");
    assert!(read(&home, &id).is_some());

    let everything = everything_under(root.path());
    assert!(
        everything.len() >= 4,
        "the profile, the directory, the file and the lock: {everything:?}"
    );
    for (path, bytes) in &everything {
        let name = path.to_string_lossy();
        assert_eq!(
            spelled_in(name.as_bytes(), &spellings),
            None,
            "{name} names the key"
        );
        assert_eq!(spelled_in(bytes, &spellings), None, "{name} holds the key");
    }
}

// -- S2: a model name is text, never a path ------------------------------------

#[test]
fn model_names_are_written_escaped_read_back_unchanged_and_never_name_a_path() {
    let hostile = [
        "quote\"and\\backslash",
        "\u{1b}[31mred\u{1b}[0m",
        "line\nbreak\rreturn\ttab",
        "nul\u{0}byte",
        "bidi\u{202e}txt\u{2066}",
        "zero\u{200b}width",
        "../../../escaped",
        "/an/absolute/path",
        "dot/./slash",
        "unicode é 漢字 🙂",
        "\u{7f}\u{85}\u{2028}",
    ];
    let (root, home) = home();
    let id = api_key_id();
    let models: Vec<ModelRow> = hostile
        .iter()
        .enumerate()
        .map(|(i, name)| ModelRow {
            model: name.to_string(),
            totals: Totals {
                request_count: i as u64,
                total_cost: i as f64,
                ..Totals::default()
            },
        })
        .collect();
    let stored = store(
        &home,
        &id,
        Entry {
            today: Some(Today {
                day: 5,
                at_ms: 1,
                models: models.clone(),
            }),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("a store");
    assert_eq!(stored.today.as_ref().unwrap().models, models);
    assert_eq!(read(&home, &id).unwrap().today.unwrap().models, models);

    let bytes = std::fs::read(file(root.path(), &id, ".json")).expect("the file");
    // No control character is written raw: ESC, CR, LF, tab and NUL are all
    // JSON escapes.
    assert!(
        bytes.iter().all(|b| *b >= 0x20),
        "a raw control byte in {}",
        String::from_utf8_lossy(&bytes)
    );
    let text = String::from_utf8(bytes).expect("UTF-8");
    assert!(text.contains(r#""quote\"and\\backslash""#), "{text}");
    assert!(text.contains(r"\u001b[31m"), "{text}");

    // Nothing is named after a model: the profile holds the directory, and
    // the directory exactly the identity's file and its lock.
    assert_eq!(
        names_in(&cache_dir(root.path())),
        [
            format!("{}.json", id.as_str()),
            format!("{}.lock", id.as_str())
        ]
    );
    let everything = everything_under(root.path());
    assert_eq!(everything.len(), 4, "{everything:?}");
}

// -- S3: modes, links and other files ------------------------------------------

#[test]
fn modes_are_owner_only_even_under_a_permissive_umask() {
    // In a copy of this binary: the umask is the whole process's.
    let root = tempfile::tempdir().expect("a home");
    let (status, report) = run_role("umask", root.path(), "");
    assert!(
        status.success() && report.contains("test result: ok. 1 passed"),
        "{report}"
    );
    let id = Identity::of(API_KEY_SCOPE, "umask", "");
    assert_eq!(mode_of(&root.path().join(".xfx")), 0o700);
    assert_eq!(mode_of(&cache_dir(root.path())), 0o700);
    assert_eq!(mode_of(&file(root.path(), &id, ".json")), 0o600);
    assert_eq!(mode_of(&file(root.path(), &id, ".lock")), 0o600);

    // A directory and files left wider by something else are made private.
    let (root, home) = home();
    let id = api_key_id();
    let dir = cache_dir(root.path());
    std::fs::create_dir(&dir).expect("a directory");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("0755");
    for suffix in [".json", ".lock"] {
        let path = file(root.path(), &id, suffix);
        std::fs::write(&path, "").expect("a file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("0644");
    }
    store(&home, &id, small_fetch(), TEST_NOW, None).expect("a store");
    assert_eq!(mode_of(&dir), 0o700);
    assert_eq!(mode_of(&file(root.path(), &id, ".json")), 0o600);
    assert_eq!(mode_of(&file(root.path(), &id, ".lock")), 0o600);
}

/// A valid file for `id`, written outside the cache, for a link to point at:
/// a link that were followed would read it as an entry.
fn decoy(root: &Path, id: &Identity) -> (PathBuf, Vec<u8>) {
    let path = root.join("decoy.json");
    let bytes = encode(
        id,
        &Entry {
            today: today(5, 999, 9.0),
            ..Entry::default()
        },
    )
    .expect("bytes");
    assert!(decode(id, &bytes).is_some(), "the decoy is a real entry");
    std::fs::write(&path, &bytes).expect("the decoy");
    (path, bytes)
}

#[test]
fn a_link_where_the_cache_directory_belongs_is_refused_not_followed() {
    let (root, home) = home();
    let id = api_key_id();
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("a directory");
    let (decoy_path, decoy_bytes) = decoy(root.path(), &id);
    let copy = elsewhere.join(format!("{}.json", id.as_str()));
    std::fs::copy(&decoy_path, &copy).expect("a copy");
    std::os::unix::fs::symlink(&elsewhere, cache_dir(root.path())).expect("a link");

    assert_eq!(read(&home, &id), None, "the link was followed");
    assert!(matches!(
        store(&home, &id, small_fetch(), TEST_NOW, None),
        Err(StoreError::Refused(_))
    ));
    assert_eq!(
        names_in(&elsewhere),
        [format!("{}.json", id.as_str())],
        "nothing was added"
    );
    assert_eq!(std::fs::read(&copy).expect("the copy"), decoy_bytes);
    assert!(std::fs::symlink_metadata(cache_dir(root.path()))
        .expect("the link")
        .file_type()
        .is_symlink());

    // A hard link to a directory cannot be made at all (`link(2)` refuses a
    // directory on Linux and macOS), so there is no such case to refuse.
    assert!(std::fs::hard_link(&elsewhere, root.path().join("dir-link")).is_err());
}

#[test]
fn a_link_where_a_cache_file_belongs_is_refused_not_followed() {
    let id = api_key_id();

    // A symbolic link and a hard link where the entry's file belongs.
    for hard in [false, true] {
        let (root, home) = home();
        std::fs::create_dir(cache_dir(root.path())).expect("a directory");
        let (decoy_path, decoy_bytes) = decoy(root.path(), &id);
        let json = file(root.path(), &id, ".json");
        if hard {
            std::fs::hard_link(&decoy_path, &json).expect("a hard link");
        } else {
            std::os::unix::fs::symlink(&decoy_path, &json).expect("a link");
        }
        assert_eq!(read(&home, &id), None, "hard={hard}: the link was followed");
        let stored = store(&home, &id, small_fetch(), TEST_NOW, None);
        assert!(
            matches!(stored, Err(StoreError::Refused(_))),
            "hard={hard}: {stored:?}"
        );
        assert_eq!(std::fs::read(&decoy_path).expect("the decoy"), decoy_bytes);
        let meta = std::fs::symlink_metadata(&json).expect("still there");
        if hard {
            assert_eq!(meta.nlink(), 2, "the hard link was replaced");
        } else {
            assert!(meta.file_type().is_symlink(), "the link was replaced");
        }
    }

    // A link where the lock belongs, to a file and to nowhere.
    for dangling in [false, true] {
        let (root, home) = home();
        std::fs::create_dir(cache_dir(root.path())).expect("a directory");
        let target = root.path().join("lock-target");
        if !dangling {
            std::fs::write(&target, "theirs").expect("a target");
        }
        std::os::unix::fs::symlink(&target, file(root.path(), &id, ".lock")).expect("a link");
        let stored = store(&home, &id, small_fetch(), TEST_NOW, None);
        assert!(
            matches!(stored, Err(StoreError::Refused(_))),
            "dangling={dangling}: {stored:?}"
        );
        if dangling {
            assert!(!target.exists(), "the link was followed to make its target");
        } else {
            assert_eq!(std::fs::read(&target).expect("the target"), b"theirs");
        }
        assert!(!file(root.path(), &id, ".json").exists());
    }

    // A link where the stage goes is removed, never written through.
    let (root, home) = home();
    std::fs::create_dir(cache_dir(root.path())).expect("a directory");
    let target = root.path().join("stage-target");
    std::fs::write(&target, "theirs").expect("a target");
    std::os::unix::fs::symlink(&target, file(root.path(), &id, ".tmp")).expect("a link");
    store(&home, &id, small_fetch(), TEST_NOW, None).expect("a store");
    assert_eq!(std::fs::read(&target).expect("the target"), b"theirs");
    assert!(std::fs::symlink_metadata(file(root.path(), &id, ".tmp")).is_err());
    assert_eq!(read(&home, &id), Some(small_fetch()));
}

/// `f` on a thread of its own, failing after `limit`: a refusal that blocked
/// on a FIFO would otherwise hang the run.
fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(f());
    });
    result.recv_timeout(limit).expect("finished in time")
}

#[test]
fn something_that_is_not_a_regular_file_is_refused() {
    let id = api_key_id();
    for (suffix, kind) in [
        (".json", "dir"),
        (".json", "fifo"),
        (".lock", "dir"),
        (".lock", "fifo"),
        (".tmp", "dir"),
    ] {
        let (root, home) = home();
        std::fs::create_dir(cache_dir(root.path())).expect("a directory");
        let path = file(root.path(), &id, suffix);
        if kind == "dir" {
            std::fs::create_dir(&path).expect("a directory");
        } else {
            let c_path =
                std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).expect("a path");
            // SAFETY: a NUL-terminated path this test owns.
            assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0, "a FIFO");
        }
        if suffix == ".json" {
            let reader = home.clone();
            assert_eq!(
                within(Duration::from_secs(10), move || read(
                    &reader,
                    &api_key_id()
                )),
                None,
                "{suffix} {kind}"
            );
        }
        let writer = home.clone();
        let stored = within(Duration::from_secs(10), move || {
            store(&writer, &api_key_id(), small_fetch(), TEST_NOW, None)
        });
        assert!(
            matches!(stored, Err(StoreError::Refused(_))),
            "{suffix} {kind}: {stored:?}"
        );
        let meta = std::fs::symlink_metadata(&path).expect("left in place");
        assert_eq!(meta.is_dir(), kind == "dir", "{suffix} {kind}");
        if suffix != ".json" {
            assert!(!file(root.path(), &id, ".json").exists());
        }
    }
}

// -- S4: two processes, and a crash --------------------------------------------

/// The variables that tell a copy of this binary which part to play.
const ROLE: &str = "XFX_TEST_USAGE_CACHE_ROLE";
const ROLE_HOME: &str = "XFX_TEST_USAGE_CACHE_HOME";
const ROLE_ARG: &str = "XFX_TEST_USAGE_CACHE_ARG";
const ROLE_TEST: &str = "usage::cache::tests::a_copy_of_this_binary_plays_its_role";

/// Stores per writer, and writers, in the two-process test.
const STORES: i64 = 200;
const WRITERS: i64 = 2;
/// Where the writers' `at_ms` values start; their clock reads later than all.
const BASE_MS: i64 = 20737 * DAY_MS + HOUR_MS;
const WRITER_NOW: i64 = BASE_MS + 10 * HOUR_MS;

/// A writer's fetch stamped `at_ms`, whose one row counts `at_ms` requests, so
/// a file mixed from two writes shows.
fn writer_fetch(at_ms: i64) -> Entry {
    Entry {
        today: Some(Today {
            day: utc_day(BASE_MS),
            at_ms,
            models: vec![ModelRow {
                model: "m".to_string(),
                totals: Totals {
                    request_count: at_ms as u64,
                    total_cost: 1.0,
                    ..Totals::default()
                },
            }],
        }),
        ..Entry::default()
    }
}

fn writer_id() -> Identity {
    Identity::of(API_KEY_SCOPE, "two processes", "")
}

/// Does nothing in a normal run. In a copy of this binary started by
/// [`spawn_role`], plays the part `ROLE` names against the home `ROLE_HOME`.
#[test]
fn a_copy_of_this_binary_plays_its_role() {
    let Some(role) = std::env::var_os(ROLE) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROLE_HOME).expect("a home"));
    let home = Home::new(Some(&root)).expect("an absolute home");
    let arg = std::env::var(ROLE_ARG).unwrap_or_default();
    match role.to_str().expect("a role") {
        "writer" => {
            // Writer `i`'s `n`th store is stamped `BASE_MS + n * WRITERS + i`.
            let i: i64 = arg.parse().expect("a writer index");
            let mut last = i64::MIN;
            for n in 0..STORES {
                let at_ms = BASE_MS + n * WRITERS + i;
                let stored = store(&home, &writer_id(), writer_fetch(at_ms), WRITER_NOW, None)
                    .expect("every store succeeds");
                let now_at = stored.today.expect("today").at_ms;
                assert!(
                    now_at >= at_ms && now_at >= last,
                    "{now_at} < {at_ms}/{last}"
                );
                last = now_at;
            }
        }
        "umask" => {
            rustix::process::umask(rustix::fs::Mode::empty());
            let id = Identity::of(API_KEY_SCOPE, "umask", "");
            store(&home, &id, small_fetch(), TEST_NOW, None).expect("a store");
        }
        "crash" => {
            // `fs::KILL_BEFORE_RENAME` is set: this store does not return.
            let _ = store(
                &home,
                &api_key_id(),
                Entry {
                    today: today(5, 900, 9.0),
                    ..Entry::default()
                },
                TEST_NOW,
                None,
            );
            panic!("the store returned instead of dying before the rename");
        }
        other => panic!("no role {other}"),
    }
}

fn spawn_role(role: &str, home: &Path, arg: &str) -> Child {
    let mut copy = crate::process::command(std::env::current_exe().expect("this test binary"));
    copy.env(ROLE, role)
        .env(ROLE_HOME, home)
        .env(ROLE_ARG, arg)
        .env("HOME", home)
        .env_remove(fs::KILL_BEFORE_RENAME)
        .args(["--exact", "--test-threads=1", ROLE_TEST])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if role == "crash" {
        copy.env(fs::KILL_BEFORE_RENAME, "1");
    }
    copy.spawn().expect("a copy of this test binary")
}

/// Waits for `child` at most `limit`, killing it by its pid after that, and
/// returns how it ended with what it printed.
fn finish(mut child: Child, limit: Duration) -> (ExitStatus, String) {
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("a status") {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("a copy of this binary ran past {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut report = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut report);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut report);
    }
    (status, report)
}

fn run_role(role: &str, home: &Path, arg: &str) -> (ExitStatus, String) {
    finish(spawn_role(role, home, arg), Duration::from_secs(60))
}

#[test]
fn two_processes_storing_at_once_never_tear_or_regress_the_file() {
    let (root, home) = home();
    let id = writer_id();
    let mut writers: Vec<Option<Child>> = (0..WRITERS)
        .map(|i| Some(spawn_role("writer", root.path(), &i.to_string())))
        .collect();

    // This process reads while they write, without a lock, as `/usage` does.
    let started = Instant::now();
    let mut results = Vec::new();
    let mut last: Option<i64> = None;
    let mut reads = 0usize;
    while writers.iter().any(Option::is_some) {
        if started.elapsed() > Duration::from_secs(120) {
            for child in writers.iter_mut().flatten() {
                let _ = child.kill();
                let _ = child.wait();
            }
            panic!("the writers ran past 120 s");
        }
        match read(&home, &id) {
            Some(entry) => {
                let t = entry.today.expect("today");
                assert_eq!(
                    t.models[0].totals.request_count, t.at_ms as u64,
                    "a file mixed from two writes"
                );
                if let Some(last) = last {
                    assert!(t.at_ms >= last, "the file went back: {} < {last}", t.at_ms);
                }
                last = Some(t.at_ms);
                reads += 1;
            }
            None => assert!(last.is_none(), "a reader found the file unreadable"),
        }
        for slot in writers.iter_mut() {
            let done = slot
                .as_mut()
                .is_some_and(|child| child.try_wait().expect("a status").is_some());
            if done {
                results.push(finish(
                    slot.take().expect("a writer"),
                    Duration::from_secs(5),
                ));
            }
        }
    }
    for (status, report) in &results {
        assert!(
            status.success() && report.contains("test result: ok. 1 passed"),
            "{report}"
        );
    }
    assert!(reads > 0, "the reader never saw a file");
    let final_entry = read(&home, &id).expect("the file");
    assert_eq!(
        final_entry.today.expect("today").at_ms,
        BASE_MS + STORES * WRITERS - 1
    );
}

#[test]
fn a_crash_between_the_stage_and_the_rename_leaves_the_old_file_whole() {
    use std::os::unix::process::ExitStatusExt;
    let (root, home) = home();
    let id = api_key_id();
    store(
        &home,
        &id,
        Entry {
            today: today(5, 500, 5.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the first store");
    let json = file(root.path(), &id, ".json");
    let before = std::fs::read(&json).expect("the file");

    let (status, report) = run_role("crash", root.path(), "");
    assert_eq!(status.signal(), Some(libc::SIGKILL), "{report}");
    assert_eq!(std::fs::read(&json).expect("the file"), before);
    let stage = std::fs::read(file(root.path(), &id, ".tmp")).expect("the stage it left");
    assert_eq!(
        decode(&id, &stage).and_then(|e| e.today).map(|t| t.at_ms),
        Some(900),
        "the stage held the new entry"
    );
    assert_eq!(read(&home, &id).unwrap().today.unwrap().at_ms, 500);

    // The next store takes the lock the dead one let go of, and replaces the
    // stage it left.
    let next = store(
        &home,
        &id,
        Entry {
            today: today(5, 600, 6.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
    )
    .expect("the next store");
    assert_eq!(next.today.unwrap().at_ms, 600);
    assert!(!file(root.path(), &id, ".tmp").exists());
    assert_eq!(read(&home, &id).unwrap().today.unwrap().at_ms, 600);
}

#[test]
fn a_lock_held_past_the_deadline_is_a_failure_not_a_wait() {
    use rustix::fs::{flock, FlockOperation};
    let (root, home) = home();
    let id = api_key_id();
    store(&home, &id, small_fetch(), TEST_NOW, None).expect("a first store");
    let before = std::fs::read(file(root.path(), &id, ".json")).expect("the file");
    // Another open of the lock is another holder: `flock` is per open file.
    let held = std::fs::File::open(file(root.path(), &id, ".lock")).expect("the lock");
    // A copy of this binary another test is starting holds every open file
    // of this process between its fork and its exec, the store's own lock
    // among them, for a moment.
    let started = Instant::now();
    while flock(&held, FlockOperation::NonBlockingLockExclusive).is_err() {
        assert!(started.elapsed() < Duration::from_secs(5), "hold the lock");
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    let refused = fs::store_within(
        &home,
        &id,
        Entry {
            today: today(5, 2, 2.0),
            ..Entry::default()
        },
        TEST_NOW,
        None,
        Duration::from_millis(200),
    );
    assert!(
        matches!(&refused, Err(StoreError::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock),
        "{refused:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        std::fs::read(file(root.path(), &id, ".json")).expect("the file"),
        before
    );
    drop(held);
    store(&home, &id, small_fetch(), TEST_NOW, None).expect("a store once it is free");
}

// -- S5: hostile or oversized files -------------------------------------------

/// A file for [`api_key_id`] with `parts` after its identity.
fn wire(parts: &str) -> String {
    format!(
        r#"{{"schema_version":2,"identity":"{}"{parts}}}"#,
        api_key_id().as_str()
    )
}

fn rows_json(rows: &[(&str, &str)]) -> String {
    let rows: Vec<String> = rows
        .iter()
        .map(|(model, totals)| format!(r#"{{"model":"{model}","totals":{{{totals}}}}}"#))
        .collect();
    format!("[{}]", rows.join(","))
}

fn today_json(rows: &[(&str, &str)]) -> String {
    format!(
        r#","today":{{"day":5,"at_ms":800,"models":{}}}"#,
        rows_json(rows)
    )
}

#[test]
fn hostile_or_oversized_files_are_refused_whole() {
    let good = r#""request_count":1,"total_cost":1"#;
    let valid = wire(&today_json(&[("m", good)]));
    assert!(
        decode(&api_key_id(), valid.as_bytes()).is_some(),
        "the control"
    );

    let max = MAX_FILE_BYTES as usize;
    let padded = |len: usize| {
        let mut bytes = valid.clone().into_bytes();
        bytes.resize(len, b' ');
        bytes
    };
    // The bound itself is a file fx would read.
    assert!(decode(&api_key_id(), &padded(max)).is_some());

    let many: Vec<String> = (0..=MAX_MODELS).map(|i| format!("m{i}")).collect();
    let many: Vec<(&str, &str)> = many.iter().map(|m| (m.as_str(), good)).collect();
    let long_name = "n".repeat(MAX_MODEL_NAME_BYTES + 1);
    let nested = format!("{}1{}", "[".repeat(200), "]".repeat(200));
    let other = Identity::of(API_KEY_SCOPE, "another key", "");
    let hostile: Vec<(&str, Vec<u8>)> = vec![
        ("over 4 MiB", padded(max + 1)),
        (
            "another identity",
            valid
                .replace(api_key_id().as_str(), other.as_str())
                .into_bytes(),
        ),
        ("schema 1", valid.replace(":2,", ":1,").into_bytes()),
        ("schema 3", valid.replace(":2,", ":3,").into_bytes()),
        (
            "no identity",
            br#"{"schema_version":2,"today":null}"#.to_vec(),
        ),
        ("garbage", b"\x00\xff garbage".to_vec()),
        ("cut short", valid.as_bytes()[..valid.len() / 2].to_vec()),
        (
            "a key twice",
            wire(&format!(
                "{}{}",
                today_json(&[("m", good)]),
                today_json(&[("m", good)])
            ))
            .into_bytes(),
        ),
        ("an unknown field", wire(r#","extra":1"#).into_bytes()),
        (
            "an unknown totals field",
            wire(&today_json(&[("m", r#""request_count":1,"bonus":1"#)])).into_bytes(),
        ),
        (
            "a negative cost",
            wire(&today_json(&[("m", r#""total_cost":-1"#)])).into_bytes(),
        ),
        (
            "a cost past f64",
            wire(&today_json(&[("m", r#""total_cost":1e999"#)])).into_bytes(),
        ),
        (
            "a negative count",
            wire(&today_json(&[("m", r#""request_count":-1"#)])).into_bytes(),
        ),
        (
            "a fractional count",
            wire(&today_json(&[("m", r#""request_count":1.5"#)])).into_bytes(),
        ),
        (
            "a count as text",
            wire(&today_json(&[("m", r#""request_count":"1""#)])).into_bytes(),
        ),
        ("257 models", wire(&today_json(&many)).into_bytes()),
        (
            "an empty name",
            wire(&today_json(&[("", good)])).into_bytes(),
        ),
        (
            "a 257-byte name",
            wire(&today_json(&[(&long_name, good)])).into_bytes(),
        ),
        (
            "a sum past f64",
            wire(&today_json(&[
                ("a", r#""total_cost":1e308"#),
                ("b", r#""total_cost":1e308"#),
            ]))
            .into_bytes(),
        ),
        (
            "a week and today past f64",
            wire(&format!(
                r#"{},"finished":{{"day":5,"at_ms":800,"days_7":{},"month":[]}}"#,
                today_json(&[("a", r#""total_cost":1.7e308"#)]),
                rows_json(&[("a", r#""total_cost":1.7e308"#)])
            ))
            .into_bytes(),
        ),
        (
            "a negative budget",
            wire(r#","budget":{"at_ms":1,"value":{"limit":-1,"spent":0,"period":"monthly"}}"#)
                .into_bytes(),
        ),
        (
            "an unknown budget period",
            wire(r#","budget":{"at_ms":1,"value":{"limit":1,"spent":0,"period":"yearly"}}"#)
                .into_bytes(),
        ),
        ("a Codex plan", wire(r#","codex":{"at_ms":1}"#).into_bytes()),
        ("a Grok plan", wire(r#","grok":0"#).into_bytes()),
        (
            "deep nesting",
            wire(&format!(r#","today":{nested}"#)).into_bytes(),
        ),
        (
            "one bad row among good ones",
            wire(&today_json(&[
                ("good", r#""request_count":5,"total_cost":5"#),
                ("bad", r#""total_cost":-0.5"#),
            ]))
            .into_bytes(),
        ),
    ];

    for (what, bytes) in hostile {
        assert_eq!(decode(&api_key_id(), &bytes), None, "{what}");
        let (root, home) = home();
        std::fs::create_dir(cache_dir(root.path())).expect("a directory");
        std::fs::write(file(root.path(), &api_key_id(), ".json"), &bytes).expect("a file");
        let shown = read(&home, &api_key_id());
        assert_eq!(shown, None, "{what}");
        // Nothing of it reaches a total: the view is empty, not a partial sum.
        let view = crate::usage::view::build(
            shown.as_ref(),
            crate::usage::view::Period::Today,
            5 * DAY_MS,
        );
        assert!(view.empty() && view.totals == Totals::default(), "{what}");
        // A store over it starts from nothing, and carries nothing of it.
        let stored = store(&home, &api_key_id(), small_fetch(), TEST_NOW, None).expect(what);
        assert_eq!(stored, small_fetch(), "{what}");
        assert_eq!(read(&home, &api_key_id()), Some(small_fetch()), "{what}");
    }
}
