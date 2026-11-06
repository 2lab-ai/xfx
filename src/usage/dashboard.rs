//! `/usage`'s runtime: when the dashboard fetches, what it holds, and what it
//! shows at one moment (`Dashboard` and `Snapshot`, `vercel-labs/fx@481104fd
//! src/usage/usage.zig:114-616`). No screen and no keys: the screen (U4)
//! calls [`Dashboard::open`], [`Dashboard::open_unavailable`],
//! [`Dashboard::close`], [`Dashboard::refresh`] (`r`), [`Dashboard::tick`]
//! (every event-loop pass) and [`Dashboard::quit`], and draws a
//! [`Snapshot`] whenever [`Dashboard::redraw_due`] says so.
//!
//! # What it does, as fx does it
//!
//! - **The cache first.** Opening shows what the usage cache holds for the
//!   sign-in at once ([`super::cache::read`]) and never waits on the network.
//!   Every snapshot reads the file again and keeps the newer of each part, so
//!   another xfx process's fetch shows at the next redraw (`reloadEntryLocked`,
//!   `usage.zig:484-505`).
//! - **When it fetches** is [`super::policy`]: on open past a 60 s floor, on
//!   `r` whenever nothing is in flight, and every 15 minutes while open.
//! - **What one fetch reads** is [`super::policy::plan`]: today, and the
//!   finished days once per UTC day. A fetch runs on a worker thread of its
//!   own, which stores the answer through [`super::cache::store`]
//!   (`fetchAll`, `usage.zig:692-711`); an answer the cache would refuse is
//!   the [`Failure::Cache`] failure (`:705-708`), and a cache that cannot be
//!   written shows the answer from memory (`unsaved`, `:713-723`).
//! - **Redraws** happen when [`Dashboard::generation`] moves -- fx's
//!   counter, bumped on every change the screen should show (`:243-245`) --
//!   or the minute turns (`collectUsageDashboardFacts`,
//!   `app_commands.zig:849-860`).
//!
//! # Where it is stricter than fx
//!
//! These requests carry the Gateway key, so:
//!
//! - **Nothing new is requested unless the dashboard is open.** Building one
//!   asks nothing; [`Dashboard::tick`] and [`Dashboard::refresh`] start
//!   nothing while it is closed, and nothing at all after
//!   [`Dashboard::quit`]. Closing does not cancel the fetch in flight, as in
//!   fx (`usage.zig:300-304`): it finishes and stores its answer under its own
//!   sign-in, so reopening within the floor shows fresh numbers from the
//!   cache.
//! - **A sign-in switch, an unavailable sign-in and quitting cancel the fetch
//!   in flight**: no further request goes out with the old key, and a
//!   dashboard switched to another key, or to no sign-in, never shows its
//!   answer. fx's switch lets the old sign-in's fetch finish and caches it for
//!   its own identity (`:596-601`). The dashboard holds at most one fetch;
//!   letting go of it and the worker's store race for one compare-and-swap. A
//!   worker that loses drops its answer. One that won just before -- its
//!   answer was already in -- stores it under its own identity, the old key's,
//!   which another key's dashboard never reads. The same key on another
//!   origin shares that identity (next item), so it can show that answer from
//!   the cache, as it shows anything else the key's entry holds.
//! - **A switch is a different key, or the same key on another Gateway
//!   origin**: either cancels the held fetch. The cache entry and the floor
//!   are the key's, as fx's identity has no origin (`identityOf`,
//!   `cache.zig:21-35`): the same key on another origin shows the numbers
//!   cached from the first and waits out the same floor.
//! - **An answer from a closed generation is never shown but through the
//!   cache.** Each fetch carries the epoch it started in, and closing moves
//!   it. A fetch that a close left running stamps its sign-in's floor --
//!   with the time it failed, not the time it was taken in -- and clears an
//!   older failure when it kept its answer, stored or (when the cache cannot
//!   be written) held in memory: fx's `unsaved` is a `.stored` outcome too,
//!   and its `finishJob` clears the held sign-in's failure on any `.stored`
//!   (`:587-595,713-723`). Its numbers and its own failure reach the screen
//!   only through the cache, as any other process's would. fx's generation counter
//!   is a redraw counter, kept as one ([`Dashboard::generation`]).
//! - **The floor is kept per key, across switches.** fx forgets it when the
//!   sign-in changes (`:465-470`), so switching away and back fetches again
//!   at once. Here each identity keeps its own [`policy::State`], so no key is
//!   asked more than once a minute by opening or the timer. `r` is not held by
//!   the floor, as in fx.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::cache::{self, Entry, Finished, Home, Identity, StoreError, Today};
use super::policy::{self, Plan, Trigger};
use super::report::{self, Budget, Failure, ModelRow, Outcome, Question, Unavailable};
use super::view::{self, Period, View};

/// The dashboard's clock: milliseconds since the Unix epoch. Injected, so a
/// test can move it; [`wall_clock`] in use.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Starts a fetch's worker thread and hands back its handle. The full-screen
/// shell's is a closure around `crate::tui::signals::spawn_unsignalled`.
pub type Spawn = Box<dyn FnMut(report::Job) -> io::Result<JoinHandle<()>> + Send>;

/// Test-only: what a worker runs between its answer and its admission.
#[cfg(test)]
type Pause = Arc<dyn Fn() + Send + Sync>;

/// The wall clock.
pub fn wall_clock() -> Clock {
    Arc::new(crate::session::event::system_now_ms)
}

/// How long [`Dashboard::quit`] waits for a cancelled worker to end before it
/// lets go of it. A cancelled fetch ends at its next poll; this is a bound,
/// not a wait anyone should see.
const QUIT_WAIT: Duration = Duration::from_secs(1);

/// How the worker and the dashboard agree on whether an answer is kept.
const LIVE: u8 = 0;
/// The worker took the answer: it stores it, and nothing can take it back.
const ADMITTED: u8 = 1;
/// The dashboard let go first: the answer is dropped.
const RETIRED: u8 = 2;

/// What `/usage` says about its numbers (`Status`, `usage.zig:107-112`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Fetching,
    /// The last fetch failed; what is cached stays on screen.
    Failed(Failure),
    /// Nothing can be fetched with this sign-in.
    Unavailable(Unavailable),
}

/// Everything `/usage` shows at one moment (`Snapshot`, `usage.zig:114-210`,
/// less the Codex and Grok plans, which xfx has no sign-in for).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// In [`Period::ALL`]'s order.
    views: [View; 3],
    pub status: Status,
    /// Who the numbers belong to, for the heading; empty with no sign-in.
    /// Never a secret.
    pub label: &'static str,
    /// When the snapshot was taken, for "updated Nm ago".
    pub now_ms: i64,
    /// [`Dashboard::generation`] when the snapshot was taken, read before it,
    /// so a fetch that settles in between still moves it.
    pub generation: u64,
}

impl Snapshot {
    /// One period's numbers.
    pub fn of(&self, period: Period) -> &View {
        match period {
            Period::Today => &self.views[0],
            Period::Days7 => &self.views[1],
            Period::Month => &self.views[2],
        }
    }

    /// The budget, when the cache holds one (`budget`, `usage.zig:137-140`).
    pub fn budget(&self) -> Option<Budget> {
        self.views[0].budget
    }

    /// The UTC day the budget next resets, when known (`budgetResetDay`,
    /// `usage.zig:142-146`).
    pub fn budget_reset_day(&self) -> Option<i64> {
        let budget = self.budget()?;
        report::budget_reset_day(budget.period, report::utc_day(self.now_ms))
    }

    /// When the numbers on screen were fetched, if any are shown
    /// (`fetchedAtMs`, `usage.zig:199-204`).
    pub fn fetched_at_ms(&self) -> Option<i64> {
        self.views[0].fetched_at_ms
    }
}

/// The sign-in the dashboard reads with.
struct Session {
    question: Question,
    identity: Identity,
}

/// The one fetch the dashboard holds.
struct InFlight {
    identity: Identity,
    /// The dashboard's [`Dashboard::epoch`] when the fetch started.
    epoch: u64,
    done: Receiver<Done>,
    cancel: CancellationToken,
    admission: Arc<AtomicU8>,
    worker: JoinHandle<()>,
}

/// How a worker's fetch ended, as it tells the dashboard.
#[derive(Debug)]
enum Done {
    /// Stored, or shown from memory when the cache could not be written.
    Kept(Entry),
    /// Why, and when by the dashboard's clock: the floor counts from then.
    Failed(Failure, i64),
    /// Cancelled, or not admitted.
    Dropped,
}

/// `/usage`'s state and its fetches (`Dashboard`, `usage.zig:229-616`).
pub struct Dashboard {
    home: Option<Home>,
    clock: Clock,
    spawn: Spawn,
    open: bool,
    quit: bool,
    session: Option<Session>,
    unavailable: Option<Unavailable>,
    entry: Option<Entry>,
    failure: Option<Failure>,
    /// Each sign-in's own floor: when its last fetch started and failed.
    floors: HashMap<Identity, policy::State>,
    fetch: Option<InFlight>,
    /// Workers of fetches let go of, until they end.
    retired: Vec<JoinHandle<()>>,
    /// The redraw counter.
    generation: u64,
    /// Moves on every close: an answer from an earlier epoch is shown only
    /// through the cache.
    epoch: u64,
    /// Test-only: a pause in every worker between its answer and its
    /// admission.
    #[cfg(test)]
    before_admission: Option<Pause>,
}

impl std::fmt::Debug for Dashboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dashboard")
            .field("open", &self.open)
            .field("quit", &self.quit)
            .field("signed_in", &self.session.is_some())
            .field("unavailable", &self.unavailable)
            .field("failure", &self.failure)
            .field("in_flight", &self.fetch.is_some())
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Dashboard {
    /// A closed dashboard over the usage cache under `home` (none: nothing is
    /// cached, and fetches are shown from memory). Asks nothing.
    pub fn new(home: Option<Home>, clock: Clock, spawn: Spawn) -> Self {
        Self {
            home,
            clock,
            spawn,
            open: false,
            quit: false,
            session: None,
            unavailable: None,
            entry: None,
            failure: None,
            floors: HashMap::new(),
            fetch: None,
            retired: Vec::new(),
            generation: 0,
            epoch: 0,
            #[cfg(test)]
            before_admission: None,
        }
    }

    /// Enter on `/usage` with a sign-in to read with (`open`,
    /// `usage.zig:280-289`): shows what is cached for it at once, and fetches
    /// when its floor allows. Never waits on the network.
    ///
    /// Another sign-in than the one held -- another key, or the same key on
    /// another origin -- takes effect here: the held fetch is cancelled, and
    /// what is shown is the new sign-in's cache entry (the key's: the same key
    /// on another origin keeps its numbers).
    pub fn open(&mut self, question: Question) {
        if self.quit {
            return;
        }
        // A fetch a close left running may have ended since the last pass.
        self.settle();
        self.open = true;
        let identity = question.identity();
        let same = self.session.as_ref().is_some_and(|held| {
            held.identity == identity && held.question.origin() == question.origin()
        });
        if !same {
            self.retire_fetch();
            self.entry = self
                .home
                .as_ref()
                .and_then(|home| cache::read(home, &identity));
            self.failure = None;
            self.unavailable = None;
            self.session = Some(Session { question, identity });
        }
        self.maybe_start(Trigger::Open);
        self.bump();
    }

    /// Enter on `/usage` with nothing to read with: shows why
    /// (`openUnavailable`, `usage.zig:291-298`). The sign-in held before is
    /// let go of, and its fetch cancelled.
    pub fn open_unavailable(&mut self, reason: Unavailable) {
        if self.quit {
            return;
        }
        self.settle();
        self.open = true;
        self.retire_fetch();
        self.session = None;
        self.entry = None;
        self.failure = None;
        self.unavailable = Some(reason);
        self.bump();
    }

    /// Esc, or anything else that takes the dashboard away (`close`,
    /// `usage.zig:300-304`). The fetch in flight runs on and stores its answer
    /// under its own sign-in; nothing new starts, and its answer is not shown
    /// but through the cache.
    pub fn close(&mut self) {
        self.open = false;
        self.epoch += 1;
    }

    /// `r` (`refresh`, `usage.zig:306-313`): fetches unless a fetch is in
    /// flight. Returns whether one started.
    pub fn refresh(&mut self) -> bool {
        self.settle();
        let started = self.maybe_start(Trigger::Manual);
        if started {
            self.bump();
        }
        started
    }

    /// Every event-loop pass (`tick`, `usage.zig:315-327`): takes in a fetch
    /// that ended, and starts the 15-minute refresh when due. Asks nothing
    /// while the dashboard is closed: the timer needs it open
    /// (`policy::due`).
    pub fn tick(&mut self) {
        self.settle();
        self.retired.retain(|worker| !worker.is_finished());
        if self.maybe_start(Trigger::Timer) {
            self.bump();
        }
    }

    /// Stops for good (`deinit`, `usage.zig:265-278`): cancels the fetch in
    /// flight, lets go of the key, and waits a bounded moment for the workers
    /// to end. Nothing is asked after it; `open`, `refresh` and `tick` do
    /// nothing. Dropping the dashboard quits it.
    pub fn quit(&mut self) {
        if self.quit {
            return;
        }
        self.quit = true;
        self.open = false;
        self.retire_fetch();
        self.session = None;
        let deadline = Instant::now() + QUIT_WAIT;
        while self.retired.iter().any(|worker| !worker.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        for worker in std::mem::take(&mut self.retired) {
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }

    /// Whether the dashboard is on screen.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Moves whenever a snapshot may differ (`generationCount`,
    /// `usage.zig:334-337`). Only compared, never counted.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The clock snapshots are taken by (`nowMs`, `usage.zig:329-332`).
    pub fn now_ms(&self) -> i64 {
        (self.clock)()
    }

    /// Whether `shown` is out of date: the dashboard changed, or the minute
    /// turned -- "updated 2m ago", and today at midnight
    /// (`collectUsageDashboardFacts`, `app_commands.zig:849-860`).
    pub fn redraw_due(&self, shown: &Snapshot) -> bool {
        const MINUTE_MS: i64 = 60 * 1000;
        self.generation != shown.generation
            || self.now_ms().div_euclid(MINUTE_MS) != shown.now_ms.div_euclid(MINUTE_MS)
    }

    /// What `/usage` shows now (`snapshot`, `usage.zig:339-380`), after
    /// reading the cache again for what other processes stored since.
    pub fn snapshot(&mut self) -> Snapshot {
        let generation = self.generation;
        let now = self.now_ms();
        self.reload(now);
        let entry = self.entry.as_ref();
        let status = if let Some(reason) = self.unavailable {
            Status::Unavailable(reason)
        } else if self.fetch.is_some() {
            // A fetch a close left running is still this sign-in's: no other
            // starts beside it.
            Status::Fetching
        } else if let Some(failure) = self.failure {
            Status::Failed(failure)
        } else {
            Status::Ready
        };
        Snapshot {
            views: Period::ALL.map(|period| view::build(entry, period, now)),
            status,
            label: self
                .session
                .as_ref()
                .map_or("", |session| session.question.label()),
            now_ms: now,
            generation,
        }
    }

    fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// The held sign-in's floor, with whether a fetch is in flight.
    fn state(&self, identity: &Identity) -> policy::State {
        policy::State {
            in_flight: self.fetch.is_some(),
            ..self.floors.get(identity).copied().unwrap_or_default()
        }
    }

    /// Records a start or a failure for `identity`, and forgets every other
    /// sign-in whose floor has passed: it would fetch at once anyway.
    fn stamp(&mut self, identity: Identity, now: i64, change: impl FnOnce(&mut policy::State)) {
        change(self.floors.entry(identity).or_default());
        self.floors
            .retain(|kept, state| *kept == identity || !policy::floor_clear(*state, now));
    }

    /// Starts a fetch for the held sign-in if `trigger` is due. Returns
    /// whether one started.
    ///
    /// Only `open` asks with [`Trigger::Open`], which `policy::due` does not
    /// hold to the dashboard being open, and `open` opens it first; `r` and
    /// the timer are held to it. A quit dashboard is closed and cannot open.
    fn maybe_start(&mut self, trigger: Trigger) -> bool {
        let Some(identity) = self.session.as_ref().map(|session| session.identity) else {
            return false;
        };
        let now = self.now_ms();
        if !policy::due(trigger, self.open, self.state(&identity), now) {
            return false;
        }
        // Another process may already hold today's finished days.
        self.reload(now);
        let finished_day = self
            .entry
            .as_ref()
            .and_then(|entry| entry.finished.as_ref())
            .map(|finished| finished.day);
        let plan = policy::plan(finished_day, now);
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let (done, receive) = mpsc::channel();
        let cancel = CancellationToken::new();
        let admission = Arc::new(AtomicU8::new(LIVE));
        let job = Worker {
            question: session.question.clone(),
            identity,
            plan,
            started_ms: now,
            home: self.home.clone(),
            held: self.entry.clone(),
            clock: Arc::clone(&self.clock),
            cancel: cancel.clone(),
            admission: Arc::clone(&admission),
            #[cfg(test)]
            before_admission: self.before_admission.clone(),
        }
        .job(done);
        match (self.spawn)(job) {
            Ok(worker) => {
                self.stamp(identity, now, |state| state.last_start_ms = Some(now));
                self.fetch = Some(InFlight {
                    identity,
                    epoch: self.epoch,
                    done: receive,
                    cancel,
                    admission,
                    worker,
                });
            }
            Err(_) => {
                // No thread, no fetch: a failure of this sign-in's, which
                // holds its floor and gives the timer a start to count from.
                self.stamp(identity, now, |state| {
                    state.last_start_ms = Some(now);
                    state.failed_at_ms = Some(now);
                });
                self.failure = Some(Failure::Network);
                self.bump();
                return false;
            }
        }
        true
    }

    /// Takes in the held fetch's end, if it has ended (`finishJob`,
    /// `usage.zig:582-615`). Always: the floor, with the failure's own time,
    /// and a kept answer -- stored, or in memory when the cache could not be
    /// written, as fx's `unsaved` is `.stored` -- clears the held sign-in's
    /// failure. Only when it ended in the epoch it started in: its numbers, and
    /// its own failure.
    fn settle(&mut self) {
        let Some(fetch) = self.fetch.as_ref() else {
            return;
        };
        let done = match fetch.done.try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Done::Failed(Failure::Network, self.now_ms()),
        };
        let Some(InFlight {
            identity,
            epoch,
            worker,
            ..
        }) = self.fetch.take()
        else {
            return;
        };
        self.retired.push(worker);
        let now = self.now_ms();
        let current = epoch == self.epoch;
        let held = self
            .session
            .as_ref()
            .is_some_and(|session| session.identity == identity);
        match done {
            Done::Kept(kept) => {
                self.stamp(identity, now, |state| state.failed_at_ms = None);
                if held {
                    // A later answer of this sign-in's than any failure shown,
                    // whichever epoch it ended in, and stored or not
                    // (`usage.zig:587-595`).
                    self.failure = None;
                }
                if current {
                    // What the worker kept already holds what the dashboard
                    // held and what the file held at the store, and is an
                    // entry the cache reads; the next snapshot merges anything
                    // newer.
                    self.entry = Some(kept);
                }
            }
            Done::Failed(failure, at_ms) => {
                self.stamp(identity, now, |state| state.failed_at_ms = Some(at_ms));
                if current {
                    self.failure = Some(failure);
                }
            }
            Done::Dropped => {}
        }
        self.bump();
    }

    /// Lets go of the held fetch: its answer is dropped unless the worker
    /// already took it, and the request in flight is cancelled.
    fn retire_fetch(&mut self) {
        let Some(fetch) = self.fetch.take() else {
            return;
        };
        // Lost only to a worker that already admitted its answer, which
        // happened before this: it was the current answer when it was taken.
        let _ = fetch
            .admission
            .compare_exchange(LIVE, RETIRED, Ordering::SeqCst, Ordering::SeqCst);
        fetch.cancel.cancel();
        self.retired.push(fetch.worker);
    }

    /// Reads the held sign-in's entry again, keeping the newer of each part
    /// (`reloadEntryLocked`, `usage.zig:484-505`). A missing or unreadable
    /// file keeps what is in memory.
    fn reload(&mut self, now: i64) {
        let (Some(home), Some(session)) = (self.home.as_ref(), self.session.as_ref()) else {
            return;
        };
        let Some(fresh) = cache::read(home, &session.identity) else {
            return;
        };
        self.entry = Some(match self.entry.take() {
            None => fresh,
            Some(held) => {
                let merged = cache::merge(held, fresh.clone(), now);
                if merged.valid() {
                    merged
                } else {
                    fresh
                }
            }
        });
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        self.quit();
    }
}

/// What a fetch's worker thread owns: its own copy of the key, and nothing of
/// the dashboard but the cache's home and the clock.
struct Worker {
    question: Question,
    identity: Identity,
    plan: Plan,
    started_ms: i64,
    home: Option<Home>,
    held: Option<Entry>,
    clock: Clock,
    cancel: CancellationToken,
    admission: Arc<AtomicU8>,
    #[cfg(test)]
    before_admission: Option<Pause>,
}

impl Worker {
    fn job(self, done: mpsc::Sender<Done>) -> report::Job {
        Box::new(move || {
            // Nobody left to tell is a fetch nobody waits for.
            let _ = done.send(self.run());
        })
    }

    /// Fetches, then stores the answer if it is admitted (`fetchAll`,
    /// `usage.zig:692-711`), reading the time under the cache's lock.
    fn run(self) -> Done {
        let ranges = self.plan.ranges();
        let reports = match report::fetch_on_this_thread(&self.question, &ranges, &self.cancel) {
            Outcome::Fetched(reports) => reports,
            Outcome::Failed(failed) => return Done::Failed(failed.failure, (self.clock)()),
            Outcome::Cancelled => return Done::Dropped,
        };
        let fetched = entry_of(self.plan, self.started_ms, reports);
        #[cfg(test)]
        if let Some(pause) = &self.before_admission {
            pause();
        }
        if self
            .admission
            .compare_exchange(LIVE, ADMITTED, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Done::Dropped;
        }
        keep(
            self.home.as_ref(),
            &self.identity,
            fetched,
            &*self.clock,
            self.held.as_ref(),
        )
    }
}

/// One fetch's reports as a cache entry (`collect`, `usage.zig:725-747`):
/// today, and the finished days when the plan read them, stamped with when
/// the fetch started.
fn entry_of(plan: Plan, started_ms: i64, reports: Vec<Vec<ModelRow>>) -> Entry {
    let mut reports = reports.into_iter();
    let today = Today {
        day: plan.day,
        at_ms: started_ms,
        models: reports.next().unwrap_or_default(),
    };
    let finished = plan.finished.then(|| Finished {
        day: plan.day,
        at_ms: started_ms,
        days_7: reports.next().unwrap_or_default(),
        month: if plan.month_range().is_some() {
            reports.next().unwrap_or_default()
        } else {
            Vec::new()
        },
    });
    Entry {
        today: Some(today),
        finished,
        budget: None,
    }
}

/// Stores `fetched` (`usage.zig:703-710`): an entry the cache would refuse is
/// the cache failure, and a cache that cannot be written shows the fetch from
/// memory.
fn keep(
    home: Option<&Home>,
    identity: &Identity,
    fetched: Entry,
    clock: &dyn Fn() -> i64,
    held: Option<&Entry>,
) -> Done {
    if let Some(home) = home {
        match cache::store_now(home, identity, fetched.clone(), clock, held) {
            Ok(stored) => return Done::Kept(stored),
            Err(StoreError::Invalid) => return Done::Failed(Failure::Cache, clock()),
            Err(StoreError::Refused(_) | StoreError::Io(_)) => {}
        }
    }
    unsaved(home, identity, fetched, clock(), held)
}

/// What a fetch shows when the cache cannot be written (`unsaved`,
/// `usage.zig:713-723`): the fetch merged with whatever can still be read,
/// or else with what the dashboard held, kept in memory only.
fn unsaved(
    home: Option<&Home>,
    identity: &Identity,
    fetched: Entry,
    now: i64,
    held: Option<&Entry>,
) -> Done {
    let on_disk = home.and_then(|home| cache::read(home, identity));
    let base = match (held, on_disk) {
        (Some(held), Some(on_disk)) => cache::merge(held.clone(), on_disk, now),
        (None, Some(on_disk)) => on_disk,
        (Some(held), None) => held.clone(),
        (None, None) => Entry::default(),
    };
    let merged = cache::merge(base, fetched, now);
    if merged.valid() {
        Done::Kept(merged)
    } else {
        Done::Failed(Failure::Cache, now)
    }
}

#[cfg(test)]
mod tests;
