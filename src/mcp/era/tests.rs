//! fx's own tests of the detection core, ported (`vercel-labs/fx@481104fd
//! src/mcp/core/era.zig:555-930`): every answer sequence up to depth 7 and
//! 2,000 random long runs against the model's invariants
//! (`tests/tla/mcp/Era.tla`), then the cases one by one.

use std::time::Duration;

use serde_json::{json, Value};

use super::*;

const TEST: Config = Config {
    preferred: &["2026-07-28", "2027-01-01"],
    legacy: &["2025-11-25", "2025-06-18", "2025-03-26"],
    probe_timeout: Duration::from_millis(50),
};

fn step(d: &mut Detector, event: Event) -> Output {
    d.step(event).expect("a step")
}

/// What the server and the client have done this process, rebuilt from the
/// core's effects (`World`, `era.zig:562-579`).
#[derive(Debug, Clone, Default)]
struct World {
    processes: u32,
    pending: Option<Pending>,
    /// Armed probe timers that haven't fired, by probe number.
    timers: u64,
    /// Fall forwards this process.
    forwards: u8,
    probes: [u8; MAX_PREFERRED],
    /// The server answered like a modern one.
    modern_answer: bool,
    last_listed: Listed,
    ready: Option<Version>,
    ready_era: Option<State>,
    initialized: u8,
    /// Modern without discovery: the server hasn't listed its versions.
    unlisted: bool,
}

impl World {
    fn pending(&self) -> Pending {
        self.pending.unwrap_or(Pending::None)
    }
}

/// One move, offered only where the environment could make it, checked
/// against the invariants (`apply`, `era.zig:583-700`). False: not offered.
fn apply(d: &mut Detector, w: &mut World, event: Event) -> bool {
    match event {
        Event::ProcessStarted if w.processes >= 2 => return false,
        Event::DiscoverResult(_)
        | Event::UnsupportedVersion(_)
        | Event::ModernRejection
        | Event::LegacyAnswer
        | Event::DiscoverNotImplemented
            if w.pending() != Pending::Discover =>
        {
            return false
        }
        Event::ProbeTimerFired(n) if n >= 64 || w.timers & (1u64 << n) == 0 => return false,
        Event::InitializeResult(_) | Event::InitializeError(_) | Event::InitializeTimedOut
            if w.pending() != Pending::Initialize =>
        {
            return false
        }
        _ => {}
    }
    let before = d.clone();
    let out = match d.step(event) {
        Ok(out) => out,
        Err(NotReady) => {
            // LegacyOrder: operation requests wait for detection.
            assert!(!before.ready());
            assert_eq!(&before, d);
            return true;
        }
    };
    match event {
        Event::ProcessStarted => {
            *w = World {
                processes: w.processes + 1,
                timers: w.timers,
                ..World::default()
            }
        }
        Event::DiscoverResult(listed) | Event::UnsupportedVersion(listed) => {
            w.modern_answer = true;
            w.last_listed = listed;
            w.unlisted = false;
            w.pending = None;
        }
        Event::DiscoverNotImplemented => {
            w.modern_answer = true;
            w.unlisted = true;
            w.pending = None;
        }
        Event::ModernRejection => {
            w.modern_answer = true;
            w.pending = None;
        }
        Event::LegacyAnswer
        | Event::InitializeResult(_)
        | Event::InitializeError(_)
        | Event::InitializeTimedOut => w.pending = None,
        Event::ProbeTimerFired(n) => {
            w.timers &= !(1u64 << n);
            // Only the current probe's timer answers anything.
            if n == before.probe && before.pending == Pending::Discover {
                w.pending = None;
            }
        }
        Event::OperationVersionRejected(listed) if before.state == State::Modern => {
            w.last_listed = listed;
            w.unlisted = false;
        }
        _ => {}
    }
    if event == Event::InitializeError(true) {
        w.modern_answer = true;
    }
    for effect in &out.effects {
        match *effect {
            Effect::SendDiscover(i) => {
                if matches!(event, Event::InitializeError(_)) {
                    w.forwards += 1;
                    assert!(w.forwards <= 1, "one fall forward");
                }
                w.pending = Some(Pending::Discover);
                w.probes[usize::from(i)] += 1;
                // ProbesBounded.
                assert!(w.probes[usize::from(i)] <= 2, "ProbesBounded");
            }
            Effect::SendInitialize => {
                // NoFallbackAfterModern.
                if w.modern_answer {
                    assert!(w.last_listed.legacy, "NoFallbackAfterModern");
                }
                w.pending = Some(Pending::Initialize);
            }
            Effect::SendInitialized => {
                assert!(matches!(event, Event::InitializeResult(Version::Legacy(_))));
                w.initialized += 1;
            }
            Effect::ArmProbeTimer { probe, .. } => {
                assert!(probe < 64);
                w.timers |= 1u64 << probe;
            }
            Effect::Ready(v) => {
                let era = if matches!(v, Version::Legacy(_)) {
                    State::Legacy
                } else {
                    State::Modern
                };
                // EraSticky.
                if let Some(e) = w.ready_era {
                    assert_eq!(e, era, "EraSticky");
                }
                w.ready_era = Some(era);
                w.ready = Some(v);
                // ChosenSupported.
                match v {
                    Version::Preferred(i) => {
                        assert!(
                            w.last_listed.has(usize::from(i)) || w.unlisted,
                            "ChosenSupported"
                        )
                    }
                    Version::Legacy(_) => {
                        assert!(matches!(event, Event::InitializeResult(Version::Legacy(_))))
                    }
                    other => panic!("ready with {other:?}"),
                }
            }
            Effect::Failed(_) => {}
        }
    }
    if d.state == State::Legacy {
        assert_eq!(w.initialized, 1);
    }
    // LegacyOrder and VersionOnEveryRequest: only after detection, with the
    // version in use.
    if event == Event::OperationSent {
        assert!(w.ready.is_some());
        let carried = if d.state == State::Modern {
            w.ready.expect("ready")
        } else {
            Version::None
        };
        assert_eq!(out.meta, Some(carried));
    }
    // Where the model has no choice, the core must act.
    match event {
        Event::DiscoverResult(listed)
            if before.pending == Pending::Discover && listed.preferred != 0 =>
        {
            assert_eq!(d.state, State::Modern)
        }
        Event::LegacyAnswer if before.pending == Pending::Discover && !before.modern_seen => {
            assert_eq!(d.state, State::Initializing)
        }
        Event::DiscoverNotImplemented if before.pending == Pending::Discover => {
            assert_eq!(d.state, State::Modern)
        }
        Event::ProbeTimerFired(n) => {
            if n == before.probe && before.pending == Pending::Discover {
                assert!(d.state != State::Probing || d.probe != before.probe);
            } else {
                // A timer from an earlier probe is no step at all.
                assert!(!out.stepped);
                assert_eq!(&before, d);
            }
        }
        Event::InitializeResult(Version::Legacy(_)) => assert_eq!(d.state, State::Legacy),
        Event::InitializeTimedOut => assert_eq!(d.state, State::Failed),
        _ => {}
    }
    if out.stepped && out.ignored {
        assert_eq!(&before, d);
        assert!(out.effects.is_empty());
    }
    true
}

/// Every move the explorer tries (`candidateMoves`, `era.zig:702-723`).
fn candidate_moves() -> Vec<Event> {
    let mut moves = vec![
        Event::ProcessStarted,
        Event::DetectRequested,
        Event::ModernRejection,
        Event::LegacyAnswer,
        Event::ProbeTimerFired(1),
        Event::ProbeTimerFired(2),
        Event::ProbeTimerFired(3),
        Event::ProbeTimerFired(4),
        Event::ProbeTimerFired(5),
        Event::InitializeResult(Version::Legacy(0)),
        Event::InitializeResult(Version::Preferred(0)),
        Event::InitializeResult(Version::Other),
        Event::InitializeError(true),
        Event::InitializeError(false),
        Event::InitializeTimedOut,
        Event::OperationSent,
        Event::DiscoverNotImplemented,
    ];
    for mask in 0..8u8 {
        let listed = Listed {
            preferred: mask & 3,
            legacy: mask & 4 != 0,
        };
        moves.push(Event::DiscoverResult(listed));
        moves.push(Event::UnsupportedVersion(listed));
        moves.push(Event::OperationVersionRejected(listed));
    }
    moves
}

fn explore(d: &Detector, w: &World, depth: usize, moves: &[Event], steps: &mut usize) {
    if depth == 0 {
        return;
    }
    for &event in moves {
        let (mut dc, mut wc) = (d.clone(), w.clone());
        if !apply(&mut dc, &mut wc, event) {
            continue;
        }
        *steps += 1;
        explore(&dc, &wc, depth - 1, moves, steps);
    }
}

#[test]
fn every_answer_sequence_up_to_depth_7_keeps_the_models_invariants() {
    let moves = candidate_moves();
    assert_eq!(moves.len(), 41);
    let mut steps = 0;
    explore(
        &Detector::new(TEST),
        &World::default(),
        7,
        &moves,
        &mut steps,
    );
    assert!(steps > 100_000, "{steps} steps");
}

/// A small fixed-seed generator, so the random runs are the same every time.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) % n as u64) as usize
    }
}

#[test]
fn random_long_runs_keep_the_invariants_and_detection_always_ends() {
    let moves = candidate_moves();
    let mut rng = Rng(0xe2a);
    for _ in 0..2_000 {
        let mut d = Detector::new(TEST);
        let mut w = World::default();
        for _ in 0..rng.below(60) {
            apply(&mut d, &mut w, moves[rng.below(moves.len())]);
        }
        // Only the fair moves from here: probe timers fire, initialize times
        // out (ReachesEra).
        for _ in 0..16 {
            if d.state != State::Probing && d.state != State::Initializing {
                break;
            }
            if d.pending == Pending::Discover {
                let probe = d.probe;
                apply(&mut d, &mut w, Event::ProbeTimerFired(probe));
            } else {
                apply(&mut d, &mut w, Event::InitializeTimedOut);
            }
        }
        assert!(d.state != State::Probing && d.state != State::Initializing);
    }
}

#[test]
fn a_modern_server_is_found_with_server_discover_and_its_first_shared_version() {
    let mut d = Detector::new(TEST);
    step(&mut d, Event::ProcessStarted);
    let out = step(&mut d, Event::DetectRequested);
    assert_eq!(
        out.effects,
        [
            Effect::SendDiscover(0),
            Effect::ArmProbeTimer {
                probe: 1,
                after: Duration::from_millis(50)
            }
        ]
    );
    let out = step(
        &mut d,
        Event::DiscoverResult(Listed {
            preferred: 0b10,
            legacy: false,
        }),
    );
    assert_eq!(out.effects, [Effect::Ready(Version::Preferred(1))]);
    assert_eq!(
        step(&mut d, Event::OperationSent).meta,
        Some(Version::Preferred(1))
    );
}

#[test]
fn minus_32022_retries_with_a_listed_version_and_names_only_the_legacy_one_to_fall_back() {
    let mut d = Detector::new(TEST);
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    let out = step(
        &mut d,
        Event::UnsupportedVersion(Listed {
            preferred: 0b10,
            legacy: false,
        }),
    );
    assert_eq!(out.effects[0], Effect::SendDiscover(1));
    let out = step(
        &mut d,
        Event::UnsupportedVersion(Listed {
            preferred: 0,
            legacy: true,
        }),
    );
    assert_eq!(out.effects, [Effect::SendInitialize]);
    let out = step(&mut d, Event::InitializeResult(Version::Legacy(0)));
    assert_eq!(
        out.effects,
        [Effect::SendInitialized, Effect::Ready(Version::Legacy(0))]
    );
}

fn body(raw: &str) -> wire::Body<'_> {
    match wire::decode(raw).expect("a message") {
        wire::Message::Response(r) => r.body,
        other => panic!("not a response: {other:?}"),
    }
}

#[test]
fn initialize_may_agree_on_an_older_2025_version_which_the_detector_keeps() {
    let mut d = Detector::new(Config::FX);
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    assert_eq!(
        step(&mut d, Event::LegacyAnswer).effects,
        [Effect::SendInitialize]
    );
    let frame = r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025-03-26"}}"#;
    let answer = classify_initialize(&d.config, body(frame));
    assert_eq!(
        step(&mut d, answer).effects,
        [Effect::SendInitialized, Effect::Ready(Version::Legacy(2))]
    );
    assert_eq!(version_name(&d.config, d.version), Some("2025-03-26"));
    // 2025 requests carry no version in `_meta`; over HTTP it goes in the
    // header.
    assert_eq!(step(&mut d, Event::OperationSent).meta, Some(Version::None));
}

#[test]
fn a_legacy_server_is_found_by_any_other_error_no_answer_or_an_error_without_an_id() {
    for answer in [Event::LegacyAnswer, Event::ProbeTimerFired(1)] {
        let mut d = Detector::new(TEST);
        step(&mut d, Event::ProcessStarted);
        step(&mut d, Event::DetectRequested);
        assert_eq!(step(&mut d, answer).effects, [Effect::SendInitialize]);
        // A late answer to the abandoned probe changes nothing.
        let late = step(
            &mut d,
            Event::DiscoverResult(Listed {
                preferred: 1,
                legacy: false,
            }),
        );
        assert!(late.ignored);
        let out = step(&mut d, Event::InitializeResult(Version::Preferred(0)));
        assert_eq!(out.effects[0], Effect::Failed(Failure::UnsupportedVersion));
    }
}

#[test]
fn minus_32020_and_minus_32021_fail_without_falling_back_and_a_modern_server_that_goes_quiet_fails()
{
    let mut d = Detector::new(TEST);
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    assert_eq!(
        step(&mut d, Event::ModernRejection).effects,
        [Effect::Failed(Failure::Rejected)]
    );

    let mut quiet = Detector::new(TEST);
    step(&mut quiet, Event::ProcessStarted);
    step(&mut quiet, Event::DetectRequested);
    step(
        &mut quiet,
        Event::UnsupportedVersion(Listed {
            preferred: 1,
            legacy: false,
        }),
    );
    assert!(!step(&mut quiet, Event::ProbeTimerFired(1)).stepped);
    assert_eq!(
        step(&mut quiet, Event::ProbeTimerFired(2)).effects,
        [Effect::Failed(Failure::WentQuiet)]
    );
}

#[test]
fn initialize_rejected_by_a_modern_server_probes_once_more() {
    let mut d = Detector::new(TEST);
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    step(&mut d, Event::ProbeTimerFired(1));
    assert_eq!(
        step(&mut d, Event::InitializeError(true)).effects[0],
        Effect::SendDiscover(0)
    );
    step(
        &mut d,
        Event::DiscoverResult(Listed {
            preferred: 1,
            legacy: false,
        }),
    );
    assert_eq!(d.state, State::Modern);

    let mut once = Detector::new(TEST);
    step(&mut once, Event::ProcessStarted);
    step(&mut once, Event::DetectRequested);
    step(&mut once, Event::LegacyAnswer);
    assert_eq!(
        step(&mut once, Event::InitializeError(false)).effects,
        [Effect::Failed(Failure::InitializeFailed)]
    );
}

#[test]
fn a_new_process_detects_again_and_nothing_goes_out_before_detection_finishes() {
    let mut d = Detector::new(TEST);
    assert_eq!(d.step(Event::OperationSent), Err(NotReady));
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    step(
        &mut d,
        Event::DiscoverResult(Listed {
            preferred: 1,
            legacy: false,
        }),
    );
    step(&mut d, Event::ProcessStarted);
    assert_eq!(d.state, State::Unknown);
    assert_eq!(d.step(Event::OperationSent), Err(NotReady));
}

#[test]
fn classifies_answers_by_their_content() {
    let listed = |preferred: u8, legacy: bool| Listed { preferred, legacy };
    let cases = [
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"supportedVersions":["2027-01-01","2025-11-25"]}}"#,
            Event::DiscoverResult(listed(0b10, true)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            Event::LegacyAnswer,
        ),
        // Only a complete result is a DiscoverResult.
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"complete","supportedVersions":["2025-11-25"]}}"#,
            Event::DiscoverResult(listed(0, true)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"input_required","supportedVersions":["2027-01-01"]}}"#,
            Event::LegacyAnswer,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"bogus","supportedVersions":["2027-01-01"]}}"#,
            Event::LegacyAnswer,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32022,"message":"x","data":{"supported":["2026-07-28"]}}}"#,
            Event::UnsupportedVersion(listed(1, false)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32022,"message":"x"}}"#,
            Event::UnsupportedVersion(Listed::default()),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32021,"message":"x"}}"#,
            Event::ModernRejection,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#,
            Event::LegacyAnswer,
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"x"}}"#,
            Event::LegacyAnswer,
        ),
    ];
    for (frame, event) in cases {
        assert_eq!(classify_discover(&TEST, body(frame)), event, "{frame}");
    }
    let init_cases = [
        (
            r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025-11-25"}}"#,
            Event::InitializeResult(Version::Legacy(0)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025-06-18"}}"#,
            Event::InitializeResult(Version::Legacy(1)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025-03-26"}}"#,
            Event::InitializeResult(Version::Legacy(2)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2024-11-05"}}"#,
            Event::InitializeResult(Version::Other),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#,
            Event::InitializeError(false),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"use 2026-07-28"}}"#,
            Event::InitializeError(true),
        ),
        (
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32022,"message":"x"}}"#,
            Event::InitializeError(true),
        ),
    ];
    for (frame, event) in init_cases {
        assert_eq!(classify_initialize(&TEST, body(frame)), event, "{frame}");
    }
}

#[test]
fn over_http_404_with_minus_32601_to_the_probe_is_a_modern_server_without_discovery() {
    let frame = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
    let not_found = body(frame);
    assert_eq!(
        classify_discover_http(&TEST, 404, Some(not_found)),
        Event::DiscoverNotImplemented
    );
    assert_eq!(
        classify_discover_http(&TEST, 200, Some(not_found)),
        Event::LegacyAnswer
    );
    assert_eq!(
        classify_discover_http(&TEST, 400, None),
        Event::LegacyAnswer
    );
    let mut d = Detector::new(TEST);
    step(&mut d, Event::ProcessStarted);
    step(&mut d, Event::DetectRequested);
    assert_eq!(
        step(&mut d, Event::DiscoverNotImplemented).effects,
        [Effect::Ready(Version::Preferred(0))]
    );
    // The first request learns the real versions.
    assert_eq!(
        step(
            &mut d,
            Event::OperationVersionRejected(Listed {
                preferred: 0b10,
                legacy: false
            })
        )
        .effects,
        [Effect::Ready(Version::Preferred(1))]
    );
}

#[test]
fn writes_the_detection_messages_in_fxs_order() {
    // `writes the detection messages`, `era.zig:912-930`, with xfx as the
    // client: its own name and version, and no capability declared.
    let discover = super::super::transport::request_frame(
        1,
        "server/discover",
        json!({"_meta": meta(&Meta::Modern("2026-07-28"), 1)}),
    );
    let initialize = {
        let mut params = serde_json::Map::new();
        params.insert("_meta".into(), meta(&Meta::Legacy, 2));
        if let Value::Object(rest) = initialize_params(&TEST) {
            params.extend(rest);
        }
        super::super::transport::request_frame(2, "initialize", Value::Object(params))
    };
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        discover.to_string(),
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server/discover\",\"params\":{{\"_meta\":{{\"progressToken\":1,\
             \"io.modelcontextprotocol/protocolVersion\":\"2026-07-28\",\"io.modelcontextprotocol/clientCapabilities\":{{}},\
             \"io.modelcontextprotocol/clientInfo\":{{\"name\":\"xfx\",\"version\":\"{version}\"}}}}}}}}"
        )
    );
    assert_eq!(
        initialize.to_string(),
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"initialize\",\"params\":{{\"_meta\":{{\"progressToken\":2}},\
             \"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"xfx\",\"version\":\"{version}\"}}}}}}"
        )
    );
}

#[test]
fn a_version_list_is_only_compared_never_kept() {
    // A hostile list: escapes around the version, a near miss, a non-string,
    // a huge string, a nested array, an escaped spelling. Only a string
    // written exactly as the client's own counts.
    let huge = "2026-07-28".repeat(100_000);
    let list = format!(
        r#"["\u001b[2J2026-07-28", "2026-07-28\u0007", 7, "{huge}", ["2026-07-28"], "2026-07", "2026\u002d07-28"]"#
    );
    assert_eq!(listed_in(&Config::FX, &list), Ok(Listed::default()));
    assert_eq!(
        listed_in(&Config::FX, r#""2026-07-28""#),
        Err(wire::DecodeError::NotAnArray)
    );
    assert_eq!(
        listed_in(&Config::FX, r#"{"2026-07-28": true}"#),
        Err(wire::DecodeError::NotAnArray)
    );
    assert_eq!(
        listed_in(&Config::FX, r#"["x", "2026-07-28", "2025-03-26"]"#),
        Ok(Listed {
            preferred: 1,
            legacy: true
        })
    );
}

#[test]
fn an_escaped_version_is_another_version_and_a_doubled_field_no_answer() {
    // `plainString` (`era.zig:425`): `initialize` answered with an escaped
    // spelling of a 2025 version agrees on no version fx speaks.
    let escaped = r#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025\u002d06-18"}}"#;
    assert_eq!(
        classify_initialize(&Config::FX, body(escaped)),
        Event::InitializeResult(Version::Other)
    );
    let supported = r#"{"jsonrpc":"2.0","id":1,"result":{"supportedVersions":["2026\u002d07-28","2025\u002d11-25"]}}"#;
    assert_eq!(
        classify_discover(&Config::FX, body(supported)),
        Event::DiscoverResult(Listed::default())
    );
    // A doubled `supportedVersions` is no DiscoverResult; a doubled envelope
    // field is no message at all.
    let doubled = r#"{"jsonrpc":"2.0","id":1,"result":{"supportedVersions":["2026-07-28"],"supportedVersions":[]}}"#;
    assert_eq!(
        classify_discover(&Config::FX, body(doubled)),
        Event::LegacyAnswer
    );
    assert_eq!(
        wire::decode(r#"{"jsonrpc":"2.0","id":1,"id":1,"result":{}}"#).err(),
        Some(wire::DecodeError::DuplicateField)
    );
}

#[test]
fn every_failure_has_fxs_words_with_xfxs_name() {
    for (failure, words) in [
        (
            Failure::NoSharedVersion,
            "it supports no MCP version xfx speaks",
        ),
        (Failure::Rejected, "it refused xfx's MCP version"),
        (Failure::WentQuiet, "it stopped answering while starting"),
        (
            Failure::UnsupportedVersion,
            "it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)",
        ),
        (
            Failure::InitializeFailed,
            "it refused or never answered xfx's initialize request",
        ),
    ] {
        assert_eq!(failure.words(), words);
        assert_eq!(Failure::from_name(failure.name()), Some(failure));
    }
}
