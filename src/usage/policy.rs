//! When `/usage` fetches, and what one fetch asks for (`vercel-labs/fx@481104fd
//! src/usage/policy.zig`). Pure: no clock, no network -- the dashboard
//! (`super::dashboard`) passes the time and its own state.
//!
//! fx's rules, kept as fx has them:
//! - **Opening** fetches unless a fetch is in flight, or one started or failed
//!   within [`FLOOR_MS`] (`policy.zig:44-48`).
//! - **`r`** fetches whenever the dashboard is open and nothing is in flight.
//!   The floor never holds it (`policy.zig:8-9,17-18,49`).
//! - **The timer** fetches every [`TIMER_MS`] while the dashboard is open,
//!   through the same floor, and only after a first fetch started
//!   (`policy.zig:50-51`).
//! - **A clock that now reads earlier** than a stamp was corrected backward,
//!   and counts as time having passed (`policy.zig:30-36`), so a correction
//!   never holds fetches until the clock catches up.
//!
//! The floor is per sign-in: the dashboard keeps one [`State`] for each
//! identity it has fetched for.

use super::report::{month_start, utc_day, DayRange};

/// The least time before opening `/usage` or the timer fetches again, after a
/// fetch starts or fails (`floor_ms`, `policy.zig:8-10`). `r` is never held by
/// it.
pub const FLOOR_MS: i64 = 60 * 1000;

/// The refresh period while the dashboard stays open (`timer_ms`,
/// `policy.zig:11-12`).
pub const TIMER_MS: i64 = 15 * 60 * 1000;

/// What asks for a fetch (`Trigger`, `policy.zig:14-21`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Enter on `/usage`.
    Open,
    /// `r` with the dashboard open: fetches whenever none is in flight.
    Manual,
    /// The periodic refresh with the dashboard open.
    Timer,
}

/// What one process knows about one sign-in's fetches (`State`,
/// `policy.zig:23-28`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub in_flight: bool,
    pub last_start_ms: Option<i64>,
    pub failed_at_ms: Option<i64>,
}

/// Whether `n` ms have passed since `t` (`since`, `policy.zig:30-36`). A clock
/// that now reads earlier than `t` was corrected backward; waiting for it to
/// catch up would hold every fetch for as long as the correction, so that
/// counts as passed.
fn since(t: Option<i64>, n: i64, now_ms: i64) -> bool {
    let Some(at) = t else {
        return true;
    };
    now_ms < at || now_ms.saturating_sub(at) >= n
}

/// Nothing in flight, and the floor has passed since the last start and the
/// last failure (`floorClear`, `policy.zig:38-42`).
pub fn floor_clear(state: State, now_ms: i64) -> bool {
    !state.in_flight
        && since(state.last_start_ms, FLOOR_MS, now_ms)
        && since(state.failed_at_ms, FLOOR_MS, now_ms)
}

/// Whether `trigger` starts a fetch now (`due`, `policy.zig:44-53`). `open` is
/// whether the dashboard is on screen; [`Trigger::Open`] is the moment it
/// opens, so it does not look at `open`.
pub fn due(trigger: Trigger, open: bool, state: State, now_ms: i64) -> bool {
    match trigger {
        Trigger::Open => floor_clear(state, now_ms),
        Trigger::Manual => open && !state.in_flight,
        Trigger::Timer => {
            open && floor_clear(state, now_ms)
                && state.last_start_ms.is_some()
                && since(state.last_start_ms, TIMER_MS, now_ms)
        }
    }
}

/// The requests of one fetch (`Plan`, `policy.zig:55-78`). Today is always
/// read. The finished days of the 7d window and of the calendar month are
/// read only when the cache lacks today's copy of them, so finished days cost
/// at most one read per UTC day.
///
/// fx's `budget` half is the member scope's (an fx login with a team), which
/// xfx has no sign-in for: an API key never reads a budget
/// (`policy.zig:87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub day: i64,
    pub finished: bool,
}

impl Plan {
    /// Today alone (`todayRange`).
    pub fn today_range(self) -> DayRange {
        DayRange {
            first: self.day,
            last: self.day,
        }
    }

    /// The six finished days before today (`weekRange`).
    pub fn week_range(self) -> DayRange {
        DayRange {
            first: self.day - 6,
            last: self.day - 1,
        }
    }

    /// The 1st of this month through yesterday, or `None` on the 1st
    /// (`monthRange`).
    pub fn month_range(self) -> Option<DayRange> {
        let first = month_start(self.day);
        (first != self.day).then_some(DayRange {
            first,
            last: self.day - 1,
        })
    }

    /// Every range the fetch reads, in the order it reads them: today, then,
    /// when due, the week's and the month's finished days (`collect`,
    /// `usage.zig:725-739`).
    pub fn ranges(self) -> Vec<DayRange> {
        let mut ranges = vec![self.today_range()];
        if self.finished {
            ranges.push(self.week_range());
            ranges.extend(self.month_range());
        }
        ranges
    }
}

/// The plan for an API key's fetch at `now_ms`, when the cache holds finished
/// days read on `cached_finished_day` (`plan`, `policy.zig:80-89`, for the
/// `api_key` scope, which reads reports).
pub fn plan(cached_finished_day: Option<i64>, now_ms: i64) -> Plan {
    let day = utc_day(now_ms);
    Plan {
        day,
        finished: cached_finished_day != Some(day),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR_MS: i64 = 60 * 60 * 1000;
    const DAY_MS: i64 = 24 * HOUR_MS;

    #[test]
    fn opening_fetches_unless_the_floor_or_a_recent_failure_holds_it() {
        // `policy.zig:96-104@481104fd`.
        let t0: i64 = 1_000_000;
        assert!(due(Trigger::Open, true, State::default(), t0));
        let started = State {
            last_start_ms: Some(t0),
            ..State::default()
        };
        assert!(!due(Trigger::Open, true, started, t0 + FLOOR_MS - 1));
        assert!(due(Trigger::Open, true, started, t0 + FLOOR_MS));
        let failed = State {
            failed_at_ms: Some(t0),
            ..State::default()
        };
        assert!(!due(Trigger::Open, true, failed, t0 + 1));
        let in_flight = State {
            in_flight: true,
            ..State::default()
        };
        assert!(!due(Trigger::Open, true, in_flight, t0));
    }

    #[test]
    fn r_needs_the_dashboard_open_and_nothing_in_flight_never_the_floor() {
        // `policy.zig:106-113@481104fd`.
        let t0: i64 = 1_000_000;
        assert!(!due(Trigger::Manual, false, State::default(), t0));
        assert!(due(Trigger::Manual, true, State::default(), t0));
        let started = State {
            last_start_ms: Some(t0),
            ..State::default()
        };
        assert!(due(Trigger::Manual, true, started, t0 + 1));
        let failed = State {
            failed_at_ms: Some(t0),
            ..State::default()
        };
        assert!(due(Trigger::Manual, true, failed, t0 + 1));
        let in_flight = State {
            in_flight: true,
            ..State::default()
        };
        assert!(!due(Trigger::Manual, true, in_flight, t0));
    }

    #[test]
    fn a_clock_corrected_backward_does_not_hold_fetches_until_it_catches_up() {
        // `policy.zig:115-122@481104fd`.
        let t0: i64 = 1_000_000;
        let back = t0 - HOUR_MS;
        let started_t0 = State {
            last_start_ms: Some(t0),
            ..State::default()
        };
        assert!(due(Trigger::Open, false, started_t0, back));
        let failed_t0 = State {
            failed_at_ms: Some(t0),
            ..State::default()
        };
        assert!(due(Trigger::Manual, true, failed_t0, back));
        assert!(due(Trigger::Timer, true, started_t0, back));
        let started_back = State {
            last_start_ms: Some(back),
            ..State::default()
        };
        assert!(!due(Trigger::Open, false, started_back, back + 1));
    }

    #[test]
    fn the_timer_fires_every_15_minutes_while_open_never_while_closed() {
        // `policy.zig:124-131@481104fd`.
        let t0: i64 = 1_000_000;
        let started = State {
            last_start_ms: Some(t0),
            ..State::default()
        };
        assert!(!due(Trigger::Timer, true, State::default(), t0));
        assert!(!due(Trigger::Timer, true, started, t0 + TIMER_MS - 1));
        assert!(due(Trigger::Timer, true, started, t0 + TIMER_MS));
        assert!(!due(Trigger::Timer, false, started, t0 + TIMER_MS));
    }

    #[test]
    fn finished_days_are_read_once_per_utc_day() {
        // `policy.zig:133-148@481104fd`, the scope-free lines: the ranges and
        // the once-a-day rule, here for the `api_key` scope. The budget flags
        // (`:137,143`) have no field here -- only the member scope reads a
        // budget, and an API key never does -- and the Codex and Grok plans
        // (`:144-147`) are sign-ins xfx does not have (U9).
        let now: i64 = 1_791_676_800_000 + 3 * HOUR_MS; // 2026-10-11 03:00 UTC
        let today = utc_day(now);
        let first = plan(None, now);
        assert!(first.finished);
        assert_eq!(
            first.week_range(),
            DayRange {
                first: today - 6,
                last: today - 1
            }
        );
        assert_eq!(
            first.month_range(),
            Some(DayRange {
                first: today - 10,
                last: today - 1
            })
        );
        assert_eq!(plan(None, now - 10 * DAY_MS).month_range(), None);
        assert!(!plan(Some(today), now).finished);
        assert!(plan(Some(today - 1), now).finished);
        // xfx's own: what one fetch reads, in order (`collect`,
        // `usage.zig:725-739@481104fd`).
        assert_eq!(
            first.ranges(),
            vec![
                first.today_range(),
                first.week_range(),
                first.month_range().unwrap()
            ]
        );
        assert_eq!(
            plan(None, now - 10 * DAY_MS).ranges().len(),
            2,
            "on the 1st the month has no finished days"
        );
        assert_eq!(plan(Some(today), now).ranges(), vec![first.today_range()]);
        // It agrees with the cache's own once-a-day rule.
        assert_eq!(
            plan(Some(today), now).finished,
            crate::usage::cache::finished_due(
                Some(&crate::usage::cache::Entry {
                    finished: Some(crate::usage::cache::Finished {
                        day: today,
                        at_ms: now,
                        days_7: Vec::new(),
                        month: Vec::new(),
                    }),
                    ..Default::default()
                }),
                now
            )
        );
    }
}
