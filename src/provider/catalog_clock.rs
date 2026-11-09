//! The clock a model catalog's age, and a failed read's backoff, are counted
//! on: one that **keeps counting while the machine sleeps**, as fx's does.
//!
//! fx reads its catalog clock from `std.Io.Clock.boot` (`clockMs`,
//! `src/core/gateway/model_catalog.zig:72-79@481104fd`), which "keeps counting
//! while the machine sleeps, so a catalog goes stale overnight, and ignores
//! wall-clock changes". Zig's std names what `boot` reads: `CLOCK_MONOTONIC_RAW`
//! on Apple, `CLOCK_BOOTTIME` on Linux, `CLOCK_MONOTONIC` on other POSIX
//! systems (`clockToPosix`, `lib/std/Io/Threaded.zig:14094-14105`, zig 0.16.0).
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

    /// A reading now.
    pub(crate) fn now(&self) -> CatalogTime {
        CatalogTime(match &self.0 {
            Source::System => system::boot(),
            #[cfg(test)]
            Source::Fake(machine) => machine.boot(),
        })
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
/// `CLOCK_UPTIME_RAW` and `CLOCK_MONOTONIC_RAW` count on a Mac.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct FakeMachine {
    awake: std::sync::Mutex<Duration>,
    slept: std::sync::Mutex<Duration>,
}

#[cfg(test)]
impl FakeMachine {
    /// A machine up for an hour, none of it asleep: far enough from the
    /// origin that a test can read earlier than now.
    pub(crate) fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            awake: std::sync::Mutex::new(Duration::from_secs(3_600)),
            slept: std::sync::Mutex::new(Duration::ZERO),
        })
    }

    /// The catalog clock this machine's readings come from.
    pub(crate) fn clock(self: &std::sync::Arc<Self>) -> CatalogClock {
        CatalogClock(Source::Fake(std::sync::Arc::clone(self)))
    }

    /// Runs for `by`: both clocks move.
    pub(crate) fn run(&self, by: Duration) {
        *lock(&self.awake) += by;
    }

    /// Sleeps for `by`: only the clock that counts sleep moves.
    pub(crate) fn sleep(&self, by: Duration) {
        *lock(&self.slept) += by;
    }

    /// The boot clock set back `by`, as a host that backs it with wall time
    /// can (`model_catalog.zig:73-75@481104fd`).
    pub(crate) fn set_back(&self, by: Duration) {
        let mut slept = lock(&self.slept);
        let mut awake = lock(&self.awake);
        let from_slept = by.min(*slept);
        *slept -= from_slept;
        *awake = awake.saturating_sub(by - from_slept);
    }

    /// What its awake clock reads: what `Instant` counts.
    pub(crate) fn awake(&self) -> Duration {
        *lock(&self.awake)
    }

    /// What its boot clock reads.
    fn boot(&self) -> Duration {
        *lock(&self.awake) + *lock(&self.slept)
    }
}

#[cfg(test)]
fn lock(time: &std::sync::Mutex<Duration>) -> std::sync::MutexGuard<'_, Duration> {
    time.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

    /// A reading of the clock that counts sleep.
    #[cfg(unix)]
    pub(super) fn boot() -> Duration {
        read(BOOT)
    }

    /// No POSIX clock to read: the time since the first reading, on the clock
    /// `Instant` has.
    #[cfg(not(unix))]
    pub(super) fn boot() -> Duration {
        static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        ORIGIN.get_or_init(std::time::Instant::now).elapsed()
    }

    /// A reading of the clock `Instant` reads.
    #[cfg(all(test, unix))]
    pub(super) fn awake() -> Duration {
        read(AWAKE)
    }

    #[cfg(unix)]
    pub(super) fn read(clock: libc::clockid_t) -> Duration {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `now` is a local that outlives the call, and `clock` is one
        // of the ids above, each of which this platform's C library defines.
        let read = unsafe { libc::clock_gettime(clock, &mut now) };
        // Every id above is one the platform has, so the call does not fail.
        // Were one call to fail among good ones, its reading is the origin's,
        // earlier than the last read's start, which is due (`CatalogDue`);
        // were every call to fail, only the first load would come due.
        if read != 0 {
            return Duration::ZERO;
        }
        let seconds = u64::try_from(now.tv_sec).unwrap_or(0);
        let nanos = u32::try_from(now.tv_nsec).unwrap_or(0);
        Duration::new(seconds, nanos)
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
        let awake = system::awake();
        let catalog = CatalogClock::system().now();
        let boot = system::read(system::BOOT);
        assert!(catalog.0 <= boot, "{catalog:?} after {boot:?}");
        assert!(
            catalog.0 + Duration::from_millis(1) >= awake,
            "the catalog's clock {catalog:?} is behind the awake one {awake:?}"
        );
    }
}
