//! The clock a model catalog's age, and a failed read's backoff, are counted
//! on: one that **keeps counting while the machine sleeps**, as fx's does.
//!
//! fx reads its catalog clock from `std.Io.Clock.boot` (`clockMs`,
//! `src/core/gateway/model_catalog.zig:72-79@481104fd`), which "keeps counting
//! while the machine sleeps, so a catalog goes stale overnight, and ignores
//! wall-clock changes". Zig's std names what `boot` reads: `CLOCK_MONOTONIC_RAW`
//! on Apple, `CLOCK_BOOTTIME` on Linux, `CLOCK_MONOTONIC_FAST` on FreeBSD and
//! DragonFly, `CLOCK_MONOTONIC` on other POSIX systems (`clockToPosix`,
//! `lib/std/Io/Threaded.zig:14094-14105`, zig 0.16.0; fx pins 0.17.0). xfx,
//! which ships for Apple and Linux only, reads `CLOCK_MONOTONIC` on the rest.
//! Rust's `Instant` reads `CLOCK_UPTIME_RAW` on Apple and `CLOCK_MONOTONIC`
//! elsewhere (`library/std/src/sys/time/unix.rs:67-70`, rust 1.99.1), which
//! stop while the machine is asleep -- Darwin's `clock_gettime(3)`:
//! `CLOCK_UPTIME_RAW` increments "in the same manner as CLOCK_MONOTONIC_RAW,
//! but that does not increment while the system is asleep" -- so a catalog
//! counted on it was as fresh after a night's sleep as at bedtime.
//!
//! **Only the catalog's own timing is counted here.** Request deadlines stay on
//! the awake clock, as fx's do (`.clock = .awake`, `src/builtins/gateway.zig:
//! 878`, `:1163@481104fd`).

use std::time::Duration;

/// A reading of the catalog's clock: the time since the clock's own origin,
/// which is no wall time and means nothing outside this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CatalogTime(Duration);

impl CatalogTime {
    /// How long after `earlier` this reading is; `None` for a reading earlier
    /// than it.
    #[cfg(test)]
    pub(crate) fn checked_duration_since(self, earlier: Self) -> Option<Duration> {
        self.0.checked_sub(earlier.0)
    }

    /// The reading `by` before this one, if the clock had counted that far.
    #[cfg(test)]
    pub(crate) fn checked_sub(self, by: Duration) -> Option<Self> {
        self.0.checked_sub(by).map(Self)
    }
}

impl std::ops::Add<Duration> for CatalogTime {
    type Output = Self;

    fn add(self, by: Duration) -> Self {
        Self(self.0.saturating_add(by))
    }
}

impl std::ops::Sub<Duration> for CatalogTime {
    type Output = Self;

    fn sub(self, by: Duration) -> Self {
        Self(self.0.saturating_sub(by))
    }
}

/// When the next read of a catalog is owed: at `at`, or at once for a reading
/// earlier than `since`, the start of the last read -- fx's rule for a clock
/// that moved backwards, "callers treat a reading earlier than a stored one as
/// due" (`model_catalog.zig:72-75@481104fd`; `beginLoad`'s `elapsed` and
/// `backoff_over`, `model_cache_runtime.zig:610-618`). The runtime decides by
/// it and tells it to the band, which asks by the same rule, so the two never
/// disagree on whether a read is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogDue {
    /// When the last read began; `None` before any.
    pub(crate) since: Option<CatalogTime>,
    pub(crate) at: CatalogTime,
}

impl CatalogDue {
    /// Owed at `at`, with no read on record to be earlier than.
    pub(crate) fn at(at: CatalogTime) -> Self {
        Self { since: None, at }
    }

    /// Whether a read is owed at `now`.
    pub(crate) fn is_due(self, now: CatalogTime) -> bool {
        self.since.is_some_and(|since| now < since) || now >= self.at
    }
}

/// Where the catalog's readings come from: the machine's clock that counts
/// sleep, or -- in a test -- a machine the test sleeps, so no test waits.
#[derive(Clone, Default)]
pub(crate) struct CatalogClock(Source);

#[derive(Clone, Default)]
enum Source {
    #[default]
    System,
    #[cfg(test)]
    Fake(std::sync::Arc<FakeMachine>),
}

impl CatalogClock {
    /// The clock that counts sleep ([`system::boot`]).
    pub(crate) fn system() -> Self {
        Self(Source::System)
    }

    /// A reading now. **A reading that failed is the origin**, as fx's is:
    /// Zig's `nowPosix` returns `.zero` when `clock_gettime` fails
    /// (`lib/std/Io/Threaded.zig:11428-11435`, zig 0.16.0), and fx's callers
    /// treat a reading earlier than a stored one as due (`clockMs`,
    /// `model_catalog.zig:72-75@481104fd`) -- so one failed reading costs at
    /// most one read more than a good one would have, and only when something
    /// asks.
    pub(crate) fn now(&self) -> CatalogTime {
        let reading = match &self.0 {
            Source::System => system::boot(),
            #[cfg(test)]
            Source::Fake(machine) => machine.boot(),
        };
        CatalogTime(reading.unwrap_or(Duration::ZERO))
    }
}

impl std::fmt::Debug for CatalogClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Source::System => f.write_str("CatalogClock::System"),
            #[cfg(test)]
            Source::Fake(machine) => write!(f, "CatalogClock::Fake({machine:?})"),
        }
    }
}

/// A machine a test runs and puts to sleep: its awake clock counts only the
/// time it ran, its boot clock that and the time it slept -- what
/// `CLOCK_UPTIME_RAW` and `CLOCK_MONOTONIC_RAW` count on a Mac -- and whose
/// boot clock a test can make fail. One lock over all of it.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct FakeMachine(std::sync::Mutex<Machine>);

#[cfg(test)]
#[derive(Debug)]
struct Machine {
    awake: Duration,
    slept: Duration,
    /// The fates of the next boot-clock readings, in turn: `true` fails.
    /// Past them, every reading succeeds.
    fates: std::collections::VecDeque<bool>,
}

#[cfg(test)]
impl FakeMachine {
    /// A machine up for an hour, none of it asleep: far enough from the
    /// origin that a test can read earlier than now.
    pub(crate) fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self(std::sync::Mutex::new(Machine {
            awake: Duration::from_secs(3_600),
            slept: Duration::ZERO,
            fates: std::collections::VecDeque::new(),
        })))
    }

    fn machine(&self) -> std::sync::MutexGuard<'_, Machine> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The catalog clock this machine's readings come from.
    pub(crate) fn clock(self: &std::sync::Arc<Self>) -> CatalogClock {
        CatalogClock(Source::Fake(std::sync::Arc::clone(self)))
    }

    /// Runs for `by`: both clocks move.
    pub(crate) fn run(&self, by: Duration) {
        self.machine().awake += by;
    }

    /// Sleeps for `by`: only the clock that counts sleep moves.
    pub(crate) fn sleep(&self, by: Duration) {
        self.machine().slept += by;
    }

    /// The boot clock set back `by`, as a host that backs it with wall time
    /// can (`model_catalog.zig:73-75@481104fd`).
    pub(crate) fn set_back(&self, by: Duration) {
        let mut machine = self.machine();
        let from_slept = by.min(machine.slept);
        machine.slept -= from_slept;
        machine.awake = machine.awake.saturating_sub(by - from_slept);
    }

    /// The next readings of the boot clock, in turn, after any already
    /// scripted: `true` fails, as a `clock_gettime` that returns an error
    /// does; `false` succeeds.
    pub(crate) fn script(&self, fates: &[bool]) {
        self.machine().fates.extend(fates.iter().copied());
    }

    /// What its awake clock reads: what `Instant` counts.
    pub(crate) fn awake(&self) -> Duration {
        self.machine().awake
    }

    /// What its boot clock reads; `None` for a reading that failed.
    fn boot(&self) -> Option<Duration> {
        let mut machine = self.machine();
        if machine.fates.pop_front() == Some(true) {
            return None;
        }
        Some(machine.awake + machine.slept)
    }
}

/// The machine's clocks.
mod system {
    use std::time::Duration;

    /// The clock that counts sleep, as Zig's `boot` reads it.
    #[cfg(target_vendor = "apple")]
    pub(super) const BOOT: libc::clockid_t = libc::CLOCK_MONOTONIC_RAW;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(super) const BOOT: libc::clockid_t = libc::CLOCK_BOOTTIME;
    #[cfg(all(
        unix,
        not(target_vendor = "apple"),
        not(any(target_os = "linux", target_os = "android"))
    ))]
    pub(super) const BOOT: libc::clockid_t = libc::CLOCK_MONOTONIC;

    /// The clock `Instant` reads, which stops while the machine sleeps.
    #[cfg(all(test, target_vendor = "apple"))]
    pub(super) const AWAKE: libc::clockid_t = libc::CLOCK_UPTIME_RAW;
    #[cfg(all(test, unix, not(target_vendor = "apple")))]
    pub(super) const AWAKE: libc::clockid_t = libc::CLOCK_MONOTONIC;

    /// A reading of the clock that counts sleep; `None` if it failed.
    #[cfg(unix)]
    pub(super) fn boot() -> Option<Duration> {
        read(BOOT)
    }

    /// No POSIX clock to read: the time since the first reading, on the clock
    /// `Instant` has, which does not count sleep (no target xfx ships).
    #[cfg(not(unix))]
    pub(super) fn boot() -> Option<Duration> {
        static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        Some(ORIGIN.get_or_init(std::time::Instant::now).elapsed())
    }

    /// A reading of the clock `Instant` reads.
    #[cfg(all(test, unix))]
    pub(super) fn awake() -> Option<Duration> {
        read(AWAKE)
    }

    /// One `clock_gettime` of `clock`; `None` when the call fails, which an id
    /// above, each one this platform defines, is not expected to.
    #[cfg(unix)]
    pub(super) fn read(clock: libc::clockid_t) -> Option<Duration> {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `now` is a local that outlives the call, and `clock` is one
        // of the ids above, each of which this platform's C library defines.
        let read = unsafe { libc::clock_gettime(clock, &mut now) };
        if read != 0 {
            return None;
        }
        let seconds = u64::try_from(now.tv_sec).unwrap_or(0);
        let nanos = u32::try_from(now.tv_nsec).unwrap_or(0);
        Some(Duration::new(seconds, nanos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_earlier_than_the_last_reads_start_is_due() {
        // fx: "callers treat a reading earlier than a stored one as due"
        // (`model_catalog.zig:72-75@481104fd`; `beginLoad`,
        // `model_cache_runtime.zig:610-618`).
        let machine = FakeMachine::new();
        let clock = machine.clock();
        let began = clock.now();
        let due = CatalogDue {
            since: Some(began),
            at: began + Duration::from_secs(300),
        };
        assert!(!due.is_due(began));
        assert!(!due.is_due(began + Duration::from_secs(299)));
        assert!(due.is_due(began + Duration::from_secs(300)));
        machine.set_back(Duration::from_secs(1));
        assert!(
            due.is_due(clock.now()),
            "a reading a second before the read began was not due"
        );
        // With no read on record, only `at` decides.
        assert!(!CatalogDue::at(began).is_due(began - Duration::from_secs(1)));
    }

    #[test]
    fn the_catalog_clock_counts_the_time_the_machine_slept() {
        // fx's `boot`: "keeps counting while the machine sleeps"
        // (`model_catalog.zig:72-74@481104fd`).
        let machine = FakeMachine::new();
        let clock = machine.clock();
        let before = clock.now();
        machine.run(Duration::from_secs(60));
        machine.sleep(Duration::from_secs(8 * 3_600));
        assert_eq!(
            clock.now().checked_duration_since(before),
            Some(Duration::from_secs(60 + 8 * 3_600)),
            "the catalog's clock did not count the night"
        );
        // What `Instant` would have counted: the minute.
        assert_eq!(machine.awake(), Duration::from_secs(3_600 + 60));
    }

    #[test]
    fn a_reading_that_failed_is_the_origin_as_fxs_is() {
        // Zig 0.16's `nowPosix` returns `.zero` on a failed `clock_gettime`
        // (`lib/std/Io/Threaded.zig:11428-11435`): the reading is the clock's
        // origin, earlier than any stored one. The next reading is itself.
        let machine = FakeMachine::new();
        let clock = machine.clock();
        let before = clock.now();
        machine.script(&[true, false]);
        assert_eq!(clock.now(), CatalogTime(Duration::ZERO));
        assert_eq!(clock.now(), before);
        let due = CatalogDue {
            since: Some(before),
            at: before + Duration::from_secs(300),
        };
        machine.script(&[true]);
        assert!(due.is_due(clock.now()), "a failed reading was not due");
        assert!(!due.is_due(clock.now()));
    }

    /// The machine's own clocks, on a platform that has the two Zig and Rust
    /// name: the catalog reads the one Zig's `boot` reads, not the one
    /// `Instant` reads -- they are equal only on a machine that has not slept
    /// since it started, and the one that counts sleep is never behind.
    #[cfg(unix)]
    #[test]
    fn the_system_catalog_clock_is_the_one_that_counts_sleep() {
        #[cfg(target_vendor = "apple")]
        assert_eq!(system::BOOT, libc::CLOCK_MONOTONIC_RAW);
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert_eq!(system::BOOT, libc::CLOCK_BOOTTIME);
        let awake = system::awake().expect("the awake clock");
        let catalog = CatalogClock::system().now();
        let boot = system::read(system::BOOT).expect("the boot clock");
        assert!(catalog.0 <= boot, "{catalog:?} after {boot:?}");
        assert!(
            catalog.0 + Duration::from_millis(1) >= awake,
            "the catalog's clock {catalog:?} is behind the awake one {awake:?}"
        );
    }
}
