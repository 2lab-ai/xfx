//! The dashboard against a loopback Gateway with fake keys and a clock the
//! test moves. Every wait is bounded; every server thread ends within 20 s.

use super::*;
use crate::config::RuntimeConfig;
use crate::gateway::Endpoint;
use crate::usage::report::utc_day;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, AtomicUsize};
use std::sync::Mutex;

const KEY_A: &str = "xfx-usage-dashboard-test-key-a-must-not-appear";
const KEY_B: &str = "xfx-usage-dashboard-test-key-b-must-not-appear";

/// The name of every worker thread a test starts.
const WORKER: &str = "xfx-usage-dashboard-test";

const MINUTE_MS: i64 = 60 * 1000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// 2026-10-11 03:00 UTC, fx's `FakeGateway` clock (`usage.zig:913@481104fd`).
const NOW: i64 = 20737 * DAY_MS + 3 * HOUR_MS;

/// How long a test waits for anything before it fails.
const PATIENCE: Duration = Duration::from_secs(20);

// -- the loopback Gateway -----------------------------------------------------

/// One request the Gateway received.
#[derive(Clone)]
struct Seen {
    path: String,
    bearer: String,
    team: bool,
    /// Set when the client hung up while the answer was held.
    hung_up: Arc<AtomicBool>,
}

/// What the Gateway answers: fx's `today_body` for a one-day range and its
/// `window_body` otherwise (`usage.zig:931-932@481104fd`), the model names
/// prefixed by whose key asked, so whose answer is whose stays visible.
#[derive(Clone)]
struct Knobs {
    status: Arc<AtomicU16>,
    hold: Arc<AtomicBool>,
    /// Replaces the model names' middle part, to tell two answers apart.
    tag: Arc<Mutex<String>>,
    /// Replaces every cost, to make an answer the cache refuses.
    cost: Arc<Mutex<Option<f64>>>,
}

struct Gateway {
    origin: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    knobs: Knobs,
    stop: Arc<AtomicBool>,
    accept: Option<std::thread::JoinHandle<Vec<std::thread::JoinHandle<()>>>>,
}

impl Gateway {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.set_nonblocking(true).expect("a polled listener");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let knobs = Knobs {
            status: Arc::new(AtomicU16::new(200)),
            hold: Arc::new(AtomicBool::new(false)),
            tag: Arc::new(Mutex::new("m".to_string())),
            cost: Arc::new(Mutex::new(None)),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let accept = {
            let (seen, knobs, stop) = (Arc::clone(&seen), knobs.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                let mut connections = Vec::new();
                let started = Instant::now();
                while !stop.load(Ordering::SeqCst) && started.elapsed() < 4 * PATIENCE {
                    match listener.accept() {
                        Ok((connection, _)) => {
                            let (seen, knobs, stop) =
                                (Arc::clone(&seen), knobs.clone(), Arc::clone(&stop));
                            connections.push(std::thread::spawn(move || {
                                serve(connection, &seen, &knobs, &stop);
                            }));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(2)),
                    }
                }
                connections
            })
        };
        Self {
            origin,
            seen,
            knobs,
            stop,
            accept: Some(accept),
        }
    }

    fn hold(&self, on: bool) {
        self.knobs.hold.store(on, Ordering::SeqCst);
    }

    fn status(&self, status: u16) {
        self.knobs.status.store(status, Ordering::SeqCst);
    }

    fn tag(&self, tag: &str) {
        *self.knobs.tag.lock().expect("the tag") = tag.to_string();
    }

    fn cost(&self, cost: Option<f64>) {
        *self.knobs.cost.lock().expect("the cost") = cost;
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().expect("the requests").clone()
    }

    fn count(&self) -> usize {
        self.seen().len()
    }

    fn paths(&self) -> Vec<String> {
        self.seen().into_iter().map(|seen| seen.path).collect()
    }

    /// Waits until `n` requests have arrived.
    fn wait_for(&self, n: usize) {
        let started = Instant::now();
        while self.count() < n {
            assert!(
                started.elapsed() < PATIENCE,
                "{} of {n} requests arrived",
                self.count()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits until the client hung up on request `i`.
    fn hung_up(&self, i: usize) -> bool {
        let started = Instant::now();
        let hung_up = Arc::clone(&self.seen()[i].hung_up);
        while !hung_up.load(Ordering::SeqCst) {
            if started.elapsed() >= Duration::from_secs(5) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(accept) = self.accept.take() {
            for connection in accept.join().unwrap_or_default() {
                let _ = connection.join();
            }
        }
    }
}

/// One connection: one request, held while `hold` is set -- watching for the
/// client to hang up -- then answered and closed.
fn serve(mut connection: TcpStream, seen: &Mutex<Vec<Seen>>, knobs: &Knobs, stop: &AtomicBool) {
    let _ = connection.set_nonblocking(false);
    let _ = connection.set_read_timeout(Some(Duration::from_secs(5)));
    let request = crate::test_net::read_request(&mut connection);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or_default()
        .to_string();
    let header = |name: &str| {
        request.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let bearer = header("authorization")
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_string))
        .unwrap_or_default();
    let hung_up = Arc::new(AtomicBool::new(false));
    seen.lock().expect("the requests").push(Seen {
        path: path.clone(),
        bearer: bearer.clone(),
        team: header("x-vercel-ai-gateway-team").is_some(),
        hung_up: Arc::clone(&hung_up),
    });

    let _ = connection.set_read_timeout(Some(Duration::from_millis(10)));
    let held = Instant::now();
    while knobs.hold.load(Ordering::SeqCst)
        && !stop.load(Ordering::SeqCst)
        && held.elapsed() < PATIENCE
    {
        let mut byte = [0u8; 1];
        match connection.read(&mut byte) {
            Ok(0) => {
                hung_up.store(true, Ordering::SeqCst);
                return;
            }
            Ok(_) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                hung_up.store(true, Ordering::SeqCst);
                return;
            }
        }
    }
    let answer = answer(&path, &bearer, knobs);
    if connection
        .write_all(answer.as_bytes())
        .and_then(|()| connection.flush())
        .is_err()
    {
        hung_up.store(true, Ordering::SeqCst);
    }
    let _ = connection.shutdown(std::net::Shutdown::Write);
}

fn answer(path: &str, bearer: &str, knobs: &Knobs) -> String {
    let status = knobs.status.load(Ordering::SeqCst);
    if status != 200 {
        return format!("HTTP/1.1 {status} No\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
    }
    let owner = match bearer {
        KEY_A => "vendor-a",
        KEY_B => "vendor-b",
        _ => "nobody",
    };
    let tag = knobs.tag.lock().expect("the tag").clone();
    let cost = *knobs.cost.lock().expect("the cost");
    let query = |name: &str| {
        path.split(['?', '&'])
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
            .map(str::to_string)
    };
    let one_day = query("start_date").is_some() && query("start_date") == query("end_date");
    let body = if one_day {
        format!(
            r#"{{"results":[{{"model":"{owner}/{tag}-opus","request_count":10,"input_tokens":5,"total_cost":{}}}]}}"#,
            cost.unwrap_or(2.5)
        )
    } else {
        format!(
            r#"{{"results":[{{"model":"{owner}/{tag}-opus","request_count":90,"total_cost":{}}},{{"model":"{owner}/{tag}-luna","request_count":7,"total_cost":{}}}]}}"#,
            cost.unwrap_or(40.0),
            cost.map_or(0.5, |_| 0.0)
        )
    };
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

// -- the clock and the workers ---------------------------------------------------

/// A clock the test sets. While `hold_worker` is set, a worker that reads it
/// -- once, between its answer and the store -- waits there, so a test can act
/// at exactly that moment.
#[derive(Clone)]
struct TestClock {
    now: Arc<AtomicI64>,
    hold_worker: Arc<AtomicBool>,
    workers_held: Arc<AtomicUsize>,
}

impl TestClock {
    fn at(now: i64) -> Self {
        Self {
            now: Arc::new(AtomicI64::new(now)),
            hold_worker: Arc::new(AtomicBool::new(false)),
            workers_held: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn clock(&self) -> Clock {
        let (now, hold, held) = (
            Arc::clone(&self.now),
            Arc::clone(&self.hold_worker),
            Arc::clone(&self.workers_held),
        );
        Arc::new(move || {
            if hold.load(Ordering::SeqCst) && std::thread::current().name() == Some(WORKER) {
                held.fetch_add(1, Ordering::SeqCst);
                let started = Instant::now();
                while hold.load(Ordering::SeqCst) && started.elapsed() < PATIENCE {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            now.load(Ordering::SeqCst)
        })
    }

    fn get(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }

    fn set(&self, now: i64) {
        self.now.store(now, Ordering::SeqCst);
    }

    fn advance(&self, by: i64) {
        self.now.fetch_add(by, Ordering::SeqCst);
    }

    /// Waits until `n` workers wait in the clock with their answers.
    fn wait_for_held_workers(&self, n: usize) {
        let started = Instant::now();
        while self.workers_held.load(Ordering::SeqCst) < n {
            assert!(started.elapsed() < PATIENCE, "no worker reached its store");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// The worker threads a dashboard starts, counted.
#[derive(Clone, Default)]
struct Workers {
    started: Arc<AtomicUsize>,
    alive: Arc<AtomicUsize>,
    refuse: Arc<AtomicBool>,
}

impl Workers {
    fn spawn(&self) -> Spawn {
        let workers = self.clone();
        Box::new(move |job| {
            if workers.refuse.load(Ordering::SeqCst) {
                return Err(io::Error::other("no thread for a test"));
            }
            workers.started.fetch_add(1, Ordering::SeqCst);
            workers.alive.fetch_add(1, Ordering::SeqCst);
            let alive = Arc::clone(&workers.alive);
            std::thread::Builder::new()
                .name(WORKER.to_string())
                .spawn(move || {
                    job();
                    alive.fetch_sub(1, Ordering::SeqCst);
                })
        })
    }

    /// How many fetches were started.
    fn fetches(&self) -> usize {
        self.started.load(Ordering::SeqCst)
    }

    fn all_ended_within(&self, limit: Duration) -> bool {
        let started = Instant::now();
        while self.alive.load(Ordering::SeqCst) > 0 {
            if started.elapsed() >= limit {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }
}

// -- the rig ----------------------------------------------------------------------

struct Rig {
    gateway: Gateway,
    clock: TestClock,
    workers: Workers,
    home: tempfile::TempDir,
}

fn rig() -> Rig {
    Rig {
        gateway: Gateway::new(),
        clock: TestClock::at(NOW),
        workers: Workers::default(),
        home: tempfile::tempdir().expect("a home"),
    }
}

impl Rig {
    fn dashboard(&self) -> Dashboard {
        self.dashboard_with(&self.clock)
    }

    /// Another xfx process's dashboard over the same home and Gateway.
    fn dashboard_with(&self, clock: &TestClock) -> Dashboard {
        Dashboard::new(
            Home::new(Some(self.home.path())),
            clock.clock(),
            self.workers.spawn(),
        )
    }

    fn question(&self, key: &str) -> Question {
        question_at(&self.gateway.origin, key)
    }

    fn cache_file(&self, key: &str) -> std::path::PathBuf {
        self.home
            .path()
            .join(".xfx")
            .join(cache::DIR_NAME)
            .join(format!("{}.json", self.question(key).identity().as_str()))
    }

    fn cached(&self, key: &str) -> Option<String> {
        std::fs::read_to_string(self.cache_file(key)).ok()
    }
}

/// The question `/usage` would ask with `key` for the Gateway at `origin`.
fn question_at(origin: &str, key: &str) -> Question {
    let home = tempfile::tempdir().expect("a home");
    let workspace = tempfile::tempdir().expect("a workspace");
    let config = RuntimeConfig::load_with(
        &crate::config::Environment::new(
            Some(home.path().to_path_buf()),
            BTreeMap::from([("AI_GATEWAY_API_KEY".to_string(), key.to_string())]),
        ),
        workspace.path(),
    )
    .expect("a configuration");
    let endpoint =
        Endpoint::resolve(Some(&format!("{origin}/v4/ai/language-model"))).expect("loopback");
    report::question(&config, &endpoint).expect("a key")
}

/// Ticks until the fetch in flight has ended (fx's `waitForFetch`).
fn wait_for_fetch(dashboard: &mut Dashboard) {
    let started = Instant::now();
    loop {
        dashboard.tick();
        if dashboard.fetch.is_none() {
            return;
        }
        assert!(started.elapsed() < PATIENCE, "the fetch never ended");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn approx(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

fn models(view: &View) -> Vec<String> {
    view.models.iter().map(|row| row.model.clone()).collect()
}

// -- fx's dashboard tests, ported -------------------------------------------------

#[test]
fn opening_shows_the_cache_at_once_and_fills_it_from_ai_gateway_in_the_background() {
    // `usage.zig:1011-1051@481104fd`, for an API key: no budget, no team.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    // Today, then the finished week and month.
    assert_eq!(
        rig.gateway.paths(),
        [
            "/v1/report?start_date=2026-10-11&end_date=2026-10-11&group_by=model&api_key_id=self",
            "/v1/report?start_date=2026-10-05&end_date=2026-10-10&group_by=model&api_key_id=self",
            "/v1/report?start_date=2026-10-01&end_date=2026-10-10&group_by=model&api_key_id=self",
        ]
    );
    assert!(rig
        .gateway
        .seen()
        .iter()
        .all(|seen| seen.bearer == KEY_A && !seen.team));

    let snap = dashboard.snapshot();
    assert_eq!(snap.status, Status::Ready);
    assert_eq!(snap.label, "API key (AI_GATEWAY_API_KEY)");
    approx(snap.of(Period::Today).totals.total_cost, 2.5);
    approx(snap.of(Period::Days7).totals.total_cost, 43.0);
    approx(snap.of(Period::Month).totals.total_cost, 43.0);
    assert_eq!(snap.of(Period::Days7).models.len(), 2);
    assert_eq!(snap.budget(), None);
    assert_eq!(snap.fetched_at_ms(), Some(NOW));

    // A second process opening later reads the shared cache immediately.
    let other_clock = TestClock::at(NOW + 1000);
    let mut other = rig.dashboard_with(&other_clock);
    other.open(rig.question(KEY_A));
    approx(other.snapshot().of(Period::Today).totals.total_cost, 2.5);
    wait_for_fetch(&mut other);
    // It already has today's finished days: only today.
    assert_eq!(rig.gateway.count(), 4);
}

#[test]
fn a_dashboard_reads_what_another_process_stored_since() {
    // `usage.zig:1103-1163@481104fd`.
    let rig = rig();
    // The first process fetches on Oct 10, finished days included.
    let first_clock = TestClock::at(20736 * DAY_MS + 23 * HOUR_MS);
    let mut first = rig.dashboard_with(&first_clock);
    first.open(rig.question(KEY_A));
    wait_for_fetch(&mut first);
    assert_eq!(rig.gateway.count(), 3);
    first.close();

    // On Oct 11 a second process stores today and today's finished days.
    let second_clock = TestClock::at(NOW);
    let mut second = rig.dashboard_with(&second_clock);
    second.open(rig.question(KEY_A));
    wait_for_fetch(&mut second);
    assert_eq!(rig.gateway.count(), 6);

    // The first reopens and its own fetch fails: it shows the second's today.
    first_clock.set(second_clock.get() + 30 * 1000);
    rig.gateway.status(500);
    first.open(rig.question(KEY_A));
    wait_for_fetch(&mut first);
    assert_eq!(rig.gateway.count(), 7);
    assert_eq!(
        first.snapshot().of(Period::Today).fetched_at_ms,
        Some(second_clock.get())
    );

    // While the first stays open, the second stores Oct 12: the first's next
    // redraw shows it.
    rig.gateway.status(200);
    second_clock.advance(DAY_MS);
    assert!(second.refresh());
    wait_for_fetch(&mut second);
    first_clock.set(second_clock.get() + 30 * 1000);
    assert_eq!(
        first.snapshot().of(Period::Today).fetched_at_ms,
        Some(second_clock.get())
    );

    // On Oct 13 the second stores the finished days first; the first's
    // refresh then reads only today.
    second_clock.advance(DAY_MS);
    assert!(second.refresh());
    wait_for_fetch(&mut second);
    first_clock.set(second_clock.get() + 30 * 1000);
    let before = rig.gateway.count();
    assert!(first.refresh());
    wait_for_fetch(&mut first);
    assert_eq!(rig.gateway.count() - before, 1);
}

#[test]
fn a_first_fetch_creates_a_private_profile_for_its_cache() {
    // `usage.zig:1165-1182@481104fd`: the home has no `~/.xfx` yet.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    assert_eq!(dashboard.snapshot().status, Status::Ready);
    let profile = std::fs::symlink_metadata(rig.home.path().join(".xfx")).expect("a profile");
    assert!(profile.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(profile.permissions().mode() & 0o777, 0o700);
    }
}

#[test]
fn the_floor_holds_open_and_the_15_minute_timer_never_r() {
    // `usage.zig:1333-1375@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.gateway.count(), 3); // no budget for an API key
    assert!(!rig.gateway.seen()[0].team);
    assert!(rig.gateway.paths()[0].ends_with("&api_key_id=self"));

    rig.clock.advance(30 * 1000);
    dashboard.tick();
    assert_eq!(rig.gateway.count(), 3);
    // r fetches at once, inside the floor or not, and again after that.
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.gateway.count(), 4);
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.gateway.count(), 5);
    // Reopening inside the floor shows the cache without fetching.
    dashboard.close();
    dashboard.open(rig.question(KEY_A));
    assert_eq!(rig.gateway.count(), 5);

    rig.clock.advance(policy::TIMER_MS - 1);
    dashboard.tick();
    assert_eq!(rig.gateway.count(), 5);
    rig.clock.advance(1);
    dashboard.tick();
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.gateway.count(), 6);

    dashboard.close();
    rig.clock.advance(policy::TIMER_MS);
    dashboard.tick();
    assert_eq!(rig.gateway.count(), 6);
}

#[test]
fn a_refused_refresh_keeps_the_cached_numbers_and_says_why() {
    // `usage.zig:1377-1396@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);

    rig.gateway.status(403);
    rig.clock.advance(policy::FLOOR_MS);
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    let snap = dashboard.snapshot();
    assert_eq!(snap.status, Status::Failed(Failure::Refused));
    approx(snap.of(Period::Today).totals.total_cost, 2.5);
}

#[test]
fn another_sign_in_never_shows_the_previous_ones_numbers() {
    // `usage.zig:1398-1432@481104fd`, with a second API key for fx's second
    // member. fx's `no_team` is the member scope's, which xfx has no sign-in
    // for; xfx's unavailable reasons stand in (`report::question`).
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);

    dashboard.close();
    dashboard.open(rig.question(KEY_B));
    let other = dashboard.snapshot();
    assert!(other.of(Period::Today).models.is_empty());
    assert!(other.of(Period::Month).models.is_empty());
    wait_for_fetch(&mut dashboard);

    for reason in [
        Unavailable::NoGatewaySignIn,
        Unavailable::UnsupportedProvider,
        Unavailable::ProviderRejected,
    ] {
        dashboard.open_unavailable(reason);
        let none = dashboard.snapshot();
        assert_eq!(none.status, Status::Unavailable(reason));
        assert!(none.of(Period::Month).models.is_empty());
        assert_eq!(none.label, "");
    }
}

#[test]
fn a_clock_corrected_backward_does_not_freeze_usage() {
    // `usage.zig:1434-1471@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    // The clock ran a day ahead when this was fetched and stored.
    rig.clock.set(NOW + DAY_MS);
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    dashboard.close();

    // Corrected: reopening fetches at once, and the result is shown and kept.
    rig.clock.set(NOW);
    let before = rig.gateway.count();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    // Today plus the finished days, which were stored for tomorrow.
    assert_eq!(rig.gateway.count() - before, 3);
    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Ready);
    assert!(shown.of(Period::Today).today_current);
    assert_eq!(shown.of(Period::Today).fetched_at_ms, Some(NOW));
    assert!(shown.of(Period::Days7).finished_current);

    // The next refresh reads only today again.
    rig.clock.set(NOW + 2 * MINUTE_MS);
    let again = rig.gateway.count();
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.gateway.count() - again, 1);
}

#[test]
fn an_unreadable_cache_file_keeps_the_finished_days_on_screen_and_in_the_file() {
    // `usage.zig:1473-1512@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);

    // Something else (garbage, or another version's schema) replaces the file.
    std::fs::write(rig.cache_file(KEY_A), "{\"schema_version\":3}").expect("a garbage file");

    rig.clock.advance(2 * MINUTE_MS);
    let before = rig.gateway.count();
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    // Only today is read; the finished days come from what the fetch was
    // planned from.
    assert_eq!(rig.gateway.count() - before, 1);
    let shown = dashboard.snapshot();
    assert!(shown.of(Period::Days7).finished_current);
    assert!(shown.of(Period::Days7).totals.total_cost > 40.0);

    // The file has them again for every other process.
    let mut other = rig.dashboard();
    other.open(rig.question(KEY_A));
    assert!(other.snapshot().of(Period::Days7).finished_current);
}

#[test]
fn a_cache_that_cannot_be_written_still_shows_what_was_fetched() {
    // `usage.zig:1514-1532@481104fd`: `usage-cache` is a file.
    let rig = rig();
    let profile = rig.home.path().join(".xfx");
    std::fs::create_dir(&profile).expect("a profile");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700))
            .expect("a private profile");
    }
    std::fs::write(profile.join(cache::DIR_NAME), "not a directory").expect("a file");
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Ready);
    assert!(shown.of(Period::Today).today_current);
    assert!(shown.of(Period::Days7).finished_current);
}

#[test]
fn a_dashboard_never_shows_less_than_it_had_when_the_file_goes_back() {
    // `usage.zig:1534-1557@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);

    // A store that started over from an unreadable file left an older entry.
    let identity = rig.question(KEY_A).identity();
    let older = format!(
        r#"{{"schema_version":2,"identity":"{}","today":{{"day":{},"at_ms":{},"models":[]}},"finished":null,"budget":null,"codex":null,"grok":null}}"#,
        identity.as_str(),
        utc_day(NOW),
        NOW - HOUR_MS
    );
    std::fs::write(rig.cache_file(KEY_A), older).expect("an older entry");
    assert!(
        cache::read(
            &Home::new(Some(rig.home.path())).expect("a home"),
            &identity
        )
        .is_some(),
        "the older entry is one the cache reads"
    );

    let shown = dashboard.snapshot();
    assert_eq!(shown.of(Period::Today).fetched_at_ms, Some(NOW));
    assert!(shown.of(Period::Days7).finished_current);
}

#[test]
fn a_key_switch_fetches_the_new_sign_in_at_once_and_r_works_for_it() {
    // `usage.zig:1559-1588@481104fd`, a provider switch there; a key switch
    // here, the only other sign-in xfx's `/usage` reads with.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);

    // Seconds later the key changes and /usage opens again.
    dashboard.close();
    rig.clock.advance(5 * 1000);
    let before_switch = rig.gateway.count();
    dashboard.open(rig.question(KEY_B));
    wait_for_fetch(&mut dashboard);
    assert!(rig.gateway.count() > before_switch);
    assert_eq!(dashboard.snapshot().fetched_at_ms(), Some(rig.clock.get()));

    // r fetches for it at once too.
    rig.clock.advance(5 * 1000);
    let before_r = rig.gateway.count();
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert!(rig.gateway.count() > before_r);
}

#[test]
fn a_sign_in_that_arrives_during_anothers_fetch_stops_it_and_fetches_at_once() {
    // `usage.zig:1590-1621@481104fd`, where the first sign-in's fetch runs on
    // and the second's is owed until it ends. Here the first is cancelled at
    // once, so the second need not wait, and nothing is owed afterwards.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.gateway.hold(true);
    dashboard.open(rig.question(KEY_A));
    rig.gateway.wait_for(1);

    dashboard.close();
    dashboard.open(rig.question(KEY_B));
    assert_eq!(
        rig.workers.fetches(),
        2,
        "the second sign-in fetches at once"
    );
    assert!(
        rig.gateway.hung_up(0),
        "the first sign-in's request was dropped"
    );
    rig.gateway.hold(false);
    wait_for_fetch(&mut dashboard);
    assert_eq!(dashboard.snapshot().fetched_at_ms(), Some(rig.clock.get()));
    // Later passes wait for the 15-minute timer.
    let fetched = rig.workers.fetches();
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), fetched);
    assert!(
        rig.cached(KEY_A).is_none(),
        "the first sign-in's answer was dropped"
    );
}

#[test]
fn an_empty_or_relative_home_is_no_home_and_a_fetch_still_shows() {
    // `usage.zig:1623-1636@481104fd`.
    let gateway = Gateway::new();
    for home in ["", "relative/home"] {
        assert_eq!(Home::new(Some(Path::new(home))), None, "{home:?}");
        let clock = TestClock::at(NOW);
        let workers = Workers::default();
        let mut dashboard = Dashboard::new(None, clock.clock(), workers.spawn());
        dashboard.open(question_at(&gateway.origin, KEY_A));
        wait_for_fetch(&mut dashboard);
        let shown = dashboard.snapshot();
        assert_eq!(shown.status, Status::Ready);
        assert_eq!(shown.of(Period::Today).fetched_at_ms, Some(NOW));
    }
    assert!(!Path::new("relative/home").exists());
}

#[test]
fn an_answer_the_cache_would_refuse_is_the_cache_failure() {
    // `usage.zig:705-708@481104fd`: `error.InvalidCache` is `.cache`. Each
    // report is finite, and today added to the week is not.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.gateway.cost(Some(1.7e308));
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Failed(Failure::Cache));
    assert!(shown.of(Period::Today).models.is_empty());
    assert!(
        rig.cached(KEY_A).is_none(),
        "nothing the cache refuses is written"
    );

    // The failure holds the floor as any other does; `r` is not held.
    dashboard.close();
    dashboard.open(rig.question(KEY_A));
    assert_eq!(rig.workers.fetches(), 1);
    rig.gateway.cost(None);
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(dashboard.snapshot().status, Status::Ready);
}

// -- the redraw counter and the minute ------------------------------------------------

#[test]
fn the_screen_redraws_when_the_dashboard_changes_or_the_minute_turns() {
    // `collectUsageDashboardFacts`, `app_commands.zig:849-860@481104fd`.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.clock.set(NOW + 30 * 1000);
    dashboard.open(rig.question(KEY_A));
    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Fetching);
    assert!(!dashboard.redraw_due(&shown));
    wait_for_fetch(&mut dashboard);
    assert!(dashboard.redraw_due(&shown), "the fetch settled");

    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Ready);
    dashboard.tick();
    assert!(
        !dashboard.redraw_due(&shown),
        "an idle pass changes nothing"
    );
    rig.clock.set(NOW + MINUTE_MS - 1);
    assert!(!dashboard.redraw_due(&shown));
    rig.clock.set(NOW + MINUTE_MS);
    assert!(dashboard.redraw_due(&shown), "the minute turned");

    let shown = dashboard.snapshot();
    assert!(dashboard.refresh());
    assert!(dashboard.redraw_due(&shown), "r started a fetch");
    wait_for_fetch(&mut dashboard);
}

#[test]
fn a_worker_that_cannot_start_is_a_failure_not_a_fetch_for_ever() {
    // fx records nothing when its thread cannot start (`usage.zig:570-574`)
    // and tries again on every pass; here it is a network failure of this
    // sign-in's, so the floor holds and the timer counts from it.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.workers.refuse.store(true, Ordering::SeqCst);
    dashboard.open(rig.question(KEY_A));
    assert_eq!(
        dashboard.snapshot().status,
        Status::Failed(Failure::Network)
    );
    for _ in 0..50 {
        dashboard.tick();
    }
    assert_eq!(rig.workers.fetches(), 0);
    assert!(!dashboard.refresh(), "r tries, and says nothing started");

    rig.workers.refuse.store(false, Ordering::SeqCst);
    rig.clock.advance(policy::TIMER_MS);
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), 1, "the timer tried again");
    wait_for_fetch(&mut dashboard);
    assert_eq!(dashboard.snapshot().status, Status::Ready);
    assert_eq!(rig.gateway.count(), 3);
}

// -- the five security properties -----------------------------------------------------

#[test]
fn nothing_is_requested_until_the_dashboard_opens() {
    // S1: building a dashboard -- what a launch would do -- asks nothing, and
    // nor does any number of passes, refreshes or snapshots while it has
    // never been open, past the timer and past midnight.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    for _ in 0..50 {
        dashboard.tick();
        assert!(!dashboard.refresh());
        let _ = dashboard.snapshot();
        rig.clock.advance(policy::TIMER_MS);
    }
    rig.clock.advance(DAY_MS);
    dashboard.tick();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.workers.fetches(), 0);
    assert_eq!(rig.gateway.count(), 0);
}

#[test]
fn nothing_is_requested_while_the_dashboard_is_closed() {
    // S1: after a fetch and a close, neither the timer, `r`, a snapshot nor
    // midnight asks again; nor does an unavailable sign-in's dashboard.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    dashboard.close();
    for _ in 0..50 {
        rig.clock.advance(policy::TIMER_MS);
        dashboard.tick();
        assert!(!dashboard.refresh());
        let _ = dashboard.snapshot();
    }
    dashboard.open_unavailable(Unavailable::UnsupportedProvider);
    for _ in 0..50 {
        rig.clock.advance(policy::TIMER_MS);
        dashboard.tick();
        assert!(!dashboard.refresh());
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.workers.fetches(), 1);
    assert_eq!(rig.gateway.count(), 3);
}

#[test]
fn close_cancels_the_fetch_in_flight_and_its_worker_ends() {
    // S1: the request held at the Gateway is dropped by the client, and the
    // worker thread ends, within a bound.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.gateway.hold(true);
    dashboard.open(rig.question(KEY_A));
    rig.gateway.wait_for(1);
    let closed = Instant::now();
    dashboard.close();
    assert!(
        closed.elapsed() < Duration::from_millis(100),
        "close does not wait"
    );
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));
    assert!(rig.gateway.hung_up(0));
    rig.gateway.hold(false);
    dashboard.tick();
    std::thread::sleep(Duration::from_millis(100));
    // The week and the month were never asked, and nothing was kept.
    assert_eq!(rig.gateway.count(), 1);
    assert!(rig.cached(KEY_A).is_none());
}

#[test]
fn quit_cancels_the_fetch_in_flight_and_nothing_is_asked_after_it() {
    // S1: quit cancels, waits a bounded moment for the worker, and leaves a
    // dashboard that asks nothing however it is called.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.gateway.hold(true);
    dashboard.open(rig.question(KEY_A));
    rig.gateway.wait_for(1);
    let quitting = Instant::now();
    dashboard.quit();
    assert!(quitting.elapsed() <= QUIT_WAIT + Duration::from_millis(100));
    assert!(
        rig.workers.all_ended_within(Duration::from_millis(100)),
        "quit waited for it"
    );
    assert!(rig.gateway.hung_up(0));
    rig.gateway.hold(false);

    rig.clock.advance(policy::TIMER_MS);
    dashboard.open(rig.question(KEY_A));
    assert!(!dashboard.refresh());
    dashboard.tick();
    assert!(!dashboard.is_open());
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(rig.workers.fetches(), 1);
    assert_eq!(rig.gateway.count(), 1);

    // Dropping a dashboard is quitting it.
    rig.gateway.hold(true);
    let mut dropped = rig.dashboard();
    dropped.open(rig.question(KEY_A));
    rig.gateway.wait_for(2);
    drop(dropped);
    assert!(rig.workers.all_ended_within(Duration::from_millis(100)));
    assert!(rig.gateway.hung_up(1));
    rig.gateway.hold(false);
}

#[test]
fn no_sign_in_is_asked_more_than_once_a_floor_by_opening_or_the_timer() {
    // S2: however fast `r` and passes arrive while a fetch is in flight, one
    // fetch; reopening inside the floor, from this sign-in or after another,
    // fetches nothing.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.gateway.hold(true);
    dashboard.open(rig.question(KEY_A));
    rig.gateway.wait_for(1);
    for _ in 0..200 {
        assert!(!dashboard.refresh(), "r starts no second fetch");
        dashboard.tick();
        rig.clock.advance(100);
    }
    rig.gateway.hold(false);
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.workers.fetches(), 1);
    assert_eq!(rig.gateway.count(), 3);
    let started = NOW;

    // Twenty reopenings inside the floor.
    for _ in 0..20 {
        dashboard.close();
        dashboard.open(rig.question(KEY_A));
        dashboard.tick();
        rig.clock.advance(1000);
    }
    assert_eq!(rig.workers.fetches(), 1);

    // The floor is this sign-in's: another key fetches at once, and coming
    // back -- straight, or through an unavailable sign-in -- does not.
    dashboard.open(rig.question(KEY_B));
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.workers.fetches(), 2);
    dashboard.open(rig.question(KEY_A));
    dashboard.open_unavailable(Unavailable::UnsupportedProvider);
    dashboard.open(rig.question(KEY_A));
    assert_eq!(rig.workers.fetches(), 2);
    approx(
        dashboard.snapshot().of(Period::Today).totals.total_cost,
        2.5,
    );

    // A's floor passes a minute after its fetch started.
    rig.clock.set(started + policy::FLOOR_MS - 1);
    dashboard.close();
    dashboard.open(rig.question(KEY_A));
    assert_eq!(rig.workers.fetches(), 2);
    rig.clock.set(started + policy::FLOOR_MS);
    dashboard.close();
    dashboard.open(rig.question(KEY_A));
    assert_eq!(rig.workers.fetches(), 3);
    wait_for_fetch(&mut dashboard);

    // `r` is never held by the floor (`policy.zig:8-9@481104fd`): once the
    // fetch has ended, it fetches again at once.
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.workers.fetches(), 4);
}

#[test]
fn the_timer_refreshes_only_while_open_and_only_through_the_floor() {
    // S3: every 15 minutes while open; never while a fetch is in flight;
    // never while closed.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_A));
    wait_for_fetch(&mut dashboard);
    rig.clock.advance(policy::TIMER_MS - 1);
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), 1);
    rig.clock.advance(1);
    rig.gateway.hold(true);
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), 2);

    // In flight past the next period: no second fetch beside it.
    for _ in 0..20 {
        rig.clock.advance(policy::TIMER_MS);
        dashboard.tick();
    }
    assert_eq!(rig.workers.fetches(), 2);
    rig.gateway.hold(false);
    wait_for_fetch(&mut dashboard);
    // Twenty periods were missed; the timer, overdue, fetches once when the
    // fetch in flight has ended, not once for each.
    assert_eq!(rig.workers.fetches(), 3);
    for _ in 0..20 {
        dashboard.tick();
    }
    assert_eq!(rig.workers.fetches(), 3);

    // A failed fetch holds the timer for a floor as well: the floor is set so
    // that the timer is due by its start and held by the failure only.
    rig.gateway.status(500);
    let failed_at = rig.clock.get();
    assert!(dashboard.refresh());
    wait_for_fetch(&mut dashboard);
    assert_eq!(rig.workers.fetches(), 4);
    rig.gateway.status(200);
    dashboard.floors.insert(
        rig.question(KEY_A).identity(),
        policy::State {
            in_flight: false,
            last_start_ms: Some(failed_at - policy::TIMER_MS),
            failed_at_ms: Some(failed_at),
        },
    );
    rig.clock.set(failed_at + policy::FLOOR_MS - 1);
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), 4, "the failure holds the timer");
    rig.clock.set(failed_at + policy::FLOOR_MS);
    dashboard.tick();
    assert_eq!(rig.workers.fetches(), 5);
    wait_for_fetch(&mut dashboard);

    // Closed, the timer never fires.
    dashboard.close();
    for _ in 0..20 {
        rig.clock.advance(policy::TIMER_MS);
        dashboard.tick();
    }
    assert_eq!(rig.workers.fetches(), 5);
}

#[test]
fn a_stale_answer_is_dropped_never_stored_or_shown() {
    // S4. (a) The answer is in, and the worker is between it and the store
    // when the dashboard closes: it is not admitted.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    rig.clock.hold_worker.store(true, Ordering::SeqCst);
    dashboard.open(rig.question(KEY_A));
    rig.clock.wait_for_held_workers(1);
    assert_eq!(rig.gateway.count(), 3, "every range was answered");
    dashboard.close();
    rig.clock.hold_worker.store(false, Ordering::SeqCst);
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));
    dashboard.tick();
    assert!(
        rig.cached(KEY_A).is_none(),
        "the closed dashboard's answer was stored"
    );
    dashboard.open(rig.question(KEY_A));
    let shown = dashboard.snapshot();
    assert!(!shown.of(Period::Today).today_current, "it was shown");
    assert_eq!(shown.status, Status::Ready);

    // (b) Reopened since: the newer fetch's answer is the one kept, whatever
    // order the two workers end in.
    rig.clock.advance(policy::FLOOR_MS);
    rig.gateway.tag("stale");
    rig.clock.hold_worker.store(true, Ordering::SeqCst);
    dashboard.close();
    dashboard.open(rig.question(KEY_A));
    rig.clock.wait_for_held_workers(2);
    dashboard.close();
    rig.clock.advance(policy::FLOOR_MS);
    rig.gateway.tag("fresh");
    dashboard.open(rig.question(KEY_A));
    rig.clock.wait_for_held_workers(3);
    rig.clock.hold_worker.store(false, Ordering::SeqCst);
    wait_for_fetch(&mut dashboard);
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));
    let file = rig.cached(KEY_A).expect("the fresh answer is stored");
    assert!(
        file.contains("fresh-opus") && !file.contains("stale"),
        "{file}"
    );
    let shown = dashboard.snapshot();
    assert_eq!(
        models(shown.of(Period::Days7)),
        ["vendor-a/fresh-opus", "vendor-a/fresh-luna"]
    );

    // (c) Closed mid-request: the request is dropped, and what the Gateway
    // answers afterwards reaches nobody.
    rig.clock.advance(policy::FLOOR_MS);
    rig.gateway.tag("late");
    rig.gateway.hold(true);
    dashboard.close();
    let before = rig.gateway.count();
    dashboard.open(rig.question(KEY_A));
    rig.gateway.wait_for(before + 1);
    dashboard.close();
    assert!(rig.gateway.hung_up(before));
    rig.gateway.hold(false);
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));
    dashboard.tick();
    assert!(!rig.cached(KEY_A).expect("the fresh file").contains("late"));
    dashboard.open(rig.question(KEY_A));
    assert!(models(dashboard.snapshot().of(Period::Today))
        .iter()
        .all(|model| !model.contains("late")));
}

#[test]
fn a_sign_in_switch_takes_effect_at_once_and_never_sends_or_keeps_the_old_key() {
    // S5. B has numbers cached; A's fetch is held at the Gateway when the
    // key changes under an open dashboard.
    let rig = rig();
    let mut dashboard = rig.dashboard();
    dashboard.open(rig.question(KEY_B));
    wait_for_fetch(&mut dashboard);
    dashboard.close();
    rig.clock.advance(policy::FLOOR_MS);

    rig.gateway.hold(true);
    dashboard.open(rig.question(KEY_A));
    let switched_at = rig.gateway.count();
    rig.gateway.wait_for(switched_at + 1);
    assert_eq!(rig.gateway.seen()[switched_at].bearer, KEY_A);
    dashboard.open(rig.question(KEY_B));

    // At once: B's own numbers, and A's request dropped.
    let shown = dashboard.snapshot();
    assert_eq!(shown.status, Status::Fetching, "B's own fetch");
    assert!(models(shown.of(Period::Month))
        .iter()
        .all(|model| model.starts_with("vendor-b/")));
    assert!(!models(shown.of(Period::Month)).is_empty());
    assert!(
        rig.gateway.hung_up(switched_at),
        "A's request was not dropped"
    );
    rig.gateway.hold(false);
    wait_for_fetch(&mut dashboard);
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));

    // Nothing after the switch carried A's key; A's answer was kept nowhere.
    let after: Vec<Seen> = rig.gateway.seen().split_off(switched_at + 1);
    assert!(!after.is_empty());
    assert!(after.iter().all(|seen| seen.bearer == KEY_B));
    assert!(rig.cached(KEY_A).is_none());
    let b_file = rig.cached(KEY_B).expect("B's file");
    assert!(!b_file.contains("vendor-a"), "{b_file}");
    for period in Period::ALL {
        assert!(models(dashboard.snapshot().of(period))
            .iter()
            .all(|model| model.starts_with("vendor-b/")));
    }

    // The same at the store: A's answer is in, its worker waits to store it,
    // and the key changes.
    rig.clock.advance(policy::FLOOR_MS);
    rig.clock.hold_worker.store(true, Ordering::SeqCst);
    dashboard.open(rig.question(KEY_A));
    rig.clock.wait_for_held_workers(1);
    dashboard.open(rig.question(KEY_B));
    rig.clock.hold_worker.store(false, Ordering::SeqCst);
    wait_for_fetch(&mut dashboard);
    assert!(rig.workers.all_ended_within(Duration::from_secs(2)));
    assert!(
        rig.cached(KEY_A).is_none(),
        "A's answer was stored after the switch"
    );
    assert!(!rig.cached(KEY_B).expect("B's file").contains("vendor-a"));

    // No cache file holds either key.
    for key in [KEY_A, KEY_B] {
        let file = rig.cached(key).unwrap_or_default();
        assert!(!file.contains(KEY_A) && !file.contains(KEY_B));
    }
}
