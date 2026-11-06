//! What `/usage` shows for one period, built from a usage cache entry
//! (`vercel-labs/fx@481104fd src/usage/view.zig`). Pure: no clock, no file,
//! no screen -- the caller passes the time, and the renderer (U4) formats it.
//!
//! A period's rows are its finished days merged with today, per model, highest
//! cost first. Today's numbers count only when they belong to the current UTC
//! day (`view.zig:75`, fx's `TodayIsToday`): after midnight, yesterday's
//! "today" is not shown as today, and nothing else stands in for it until a
//! fetch brings the new day.

use super::cache::Entry;
use super::report::{utc_day, Budget, ModelRow, Totals};

/// The periods the dashboard offers (`Period`, `view.zig:11-42`), in fx's
/// order, which Tab and Shift-Tab walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Period {
    Today,
    /// The six finished days before today, and today.
    Days7,
    /// The calendar month so far, in UTC; matches a monthly budget.
    Month,
}

impl Period {
    /// Every period, in order (`Period.all`).
    pub const ALL: [Period; 3] = [Period::Today, Period::Days7, Period::Month];

    /// The name the period table and the model table's heading use.
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Days7 => "7d",
            Self::Month => "month",
        }
    }

    /// The next period, wrapping after the month.
    pub fn next(self) -> Self {
        match self {
            Self::Today => Self::Days7,
            Self::Days7 => Self::Month,
            Self::Month => Self::Today,
        }
    }

    /// The previous period, wrapping before today.
    pub fn previous(self) -> Self {
        match self {
            Self::Today => Self::Month,
            Self::Days7 => Self::Today,
            Self::Month => Self::Days7,
        }
    }
}

/// One period's numbers (`View`, `view.zig:44-66`).
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub period: Period,
    /// Per-model numbers, highest cost first, then by name. A model's name is
    /// the server's text, untrusted: see [`ModelRow::model`].
    pub models: Vec<ModelRow>,
    pub totals: Totals,
    /// Whether today's numbers are present for the current UTC day.
    pub today_current: bool,
    /// For 7d and month: whether the finished days were fetched today.
    pub finished_current: bool,
    /// When today's numbers were fetched, if any are shown.
    pub fetched_at_ms: Option<i64>,
    pub budget: Option<Budget>,
}

impl View {
    /// Nothing is cached yet for this period (`empty`, `view.zig:62-65`).
    pub fn empty(&self) -> bool {
        !self.today_current
            && (self.period == Period::Today || !self.finished_current)
            && self.models.is_empty()
    }
}

/// The view of `period` at wall time `now_ms` (`build`, `view.zig:68-106`).
/// No entry is an empty one.
pub fn build(entry: Option<&Entry>, period: Period, now_ms: i64) -> View {
    let day = utc_day(now_ms);
    let today = entry
        .and_then(|e| e.today.as_ref())
        .filter(|today| today.day == day);
    let finished: &[ModelRow] = match (period, entry.and_then(|e| e.finished.as_ref())) {
        (Period::Today, _) | (_, None) => &[],
        (Period::Days7, Some(finished)) => &finished.days_7,
        (Period::Month, Some(finished)) => &finished.month,
    };

    let mut models: Vec<ModelRow> = Vec::new();
    let mut totals = Totals::default();
    let today_rows: &[ModelRow] = today.map_or(&[], |today| &today.models);
    for row in finished.iter().chain(today_rows) {
        totals.add(&row.totals);
        match models.iter_mut().find(|merged| merged.model == row.model) {
            Some(merged) => merged.totals.add(&row.totals),
            None => models.push(row.clone()),
        }
    }
    // Highest cost first, then by name (`costDescending`, `view.zig:113-116`).
    // A cache entry's costs are finite (`Entry::valid`); a sum that overflowed
    // to infinity still orders, and NaN cannot arise from finite non-negative
    // parts.
    models.sort_by(|a, b| {
        b.totals
            .total_cost
            .partial_cmp(&a.totals.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.model.cmp(&b.model))
    });

    View {
        period,
        models,
        totals,
        today_current: today.is_some(),
        finished_current: entry
            .and_then(|e| e.finished.as_ref())
            .is_some_and(|finished| finished.day == day),
        fetched_at_ms: today.map(|today| today.at_ms),
        budget: entry.and_then(|e| e.budget.as_ref()).map(|b| b.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::cache::{Budget as CachedBudget, Finished, Today};
    use crate::usage::report::RefreshPeriod;

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    const HOUR_MS: i64 = 60 * 60 * 1000;

    fn row(model: &str, cost: f64, requests: u64) -> ModelRow {
        ModelRow {
            model: model.to_string(),
            totals: Totals {
                request_count: requests,
                total_cost: cost,
                ..Totals::default()
            },
        }
    }

    #[test]
    fn seven_days_add_todays_numbers_to_the_finished_days_per_model() {
        // `view.zig:128-147@481104fd`.
        let now = 20737 * DAY_MS + 5 * HOUR_MS;
        let entry = Entry {
            today: Some(Today {
                day: 20737,
                at_ms: now - 1000,
                models: vec![row("a", 1.0, 1), row("b", 4.0, 2)],
            }),
            finished: Some(Finished {
                day: 20737,
                at_ms: now - 1000,
                days_7: vec![row("a", 10.0, 5)],
                month: vec![row("a", 30.0, 9)],
            }),
            budget: Some(CachedBudget {
                at_ms: now,
                value: Budget {
                    limit: 100.0,
                    spent: 41.0,
                    period: RefreshPeriod::Monthly,
                },
            }),
        };
        let week = build(Some(&entry), Period::Days7, now);
        assert_eq!(week.models.len(), 2);
        assert_eq!(week.models[0].model, "a");
        assert_eq!(week.models[0].totals.total_cost, 11.0);
        assert_eq!(week.totals.request_count, 8);
        assert!(week.today_current && week.finished_current);
        assert_eq!(week.budget.expect("the budget").spent, 41.0);
        assert_eq!(week.fetched_at_ms, Some(now - 1000));

        let day = build(Some(&entry), Period::Today, now);
        assert_eq!(day.totals.total_cost, 5.0);
        // The month is the finished month and today, the same way.
        let month = build(Some(&entry), Period::Month, now);
        assert_eq!(month.totals.total_cost, 35.0);
        assert_eq!(month.models[0].totals.request_count, 10);
    }

    #[test]
    fn after_midnight_yesterdays_numbers_are_not_shown_as_today() {
        // `view.zig:149-165@481104fd`.
        let yesterday = 20736 * DAY_MS + 23 * HOUR_MS;
        let entry = Entry {
            today: Some(Today {
                day: 20736,
                at_ms: yesterday,
                models: vec![row("a", 9.0, 3)],
            }),
            finished: Some(Finished {
                day: 20736,
                at_ms: yesterday,
                days_7: vec![row("a", 1.0, 1)],
                month: vec![row("a", 2.0, 1)],
            }),
            budget: None,
        };
        let just_after = 20737 * DAY_MS + 1000;
        let today = build(Some(&entry), Period::Today, just_after);
        assert!(!today.today_current);
        assert!(today.models.is_empty());
        assert!(today.empty());
        assert_eq!(today.fetched_at_ms, None);
        let week = build(Some(&entry), Period::Days7, just_after);
        assert!(!week.finished_current);
        assert_eq!(week.totals.total_cost, 1.0);
        // Yesterday's finished days are still shown, so the week is not empty.
        assert!(!week.empty());
    }

    #[test]
    fn periods_cycle_both_ways() {
        // `view.zig:167-171@481104fd`, and every other step.
        assert_eq!(Period::Today.next(), Period::Days7);
        assert_eq!(Period::Month.next(), Period::Today);
        assert_eq!(Period::Today.previous(), Period::Month);
        for period in Period::ALL {
            assert_eq!(period.next().previous(), period);
        }
        let labels: Vec<_> = Period::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels, ["today", "7d", "month"]);
    }

    #[test]
    fn no_entry_is_empty_and_equal_costs_sort_by_name() {
        let view = build(None, Period::Month, 20737 * DAY_MS);
        assert!(view.empty());
        assert_eq!(view.totals, Totals::default());
        assert_eq!(view.budget, None);

        let now = 20737 * DAY_MS;
        let entry = Entry {
            today: Some(Today {
                day: 20737,
                at_ms: now,
                models: vec![row("b", 1.0, 1), row("a", 1.0, 1), row("c", 2.0, 1)],
            }),
            ..Entry::default()
        };
        let names: Vec<_> = build(Some(&entry), Period::Today, now)
            .models
            .into_iter()
            .map(|row| row.model)
            .collect();
        assert_eq!(names, ["c", "a", "b"]);
    }
}
