//! The inline menu that completes a slash command.
//!
//! It is the band's third elastic block, and it is deliberately **not** a
//! second approval panel. A question owns the focus, because the session is
//! waiting to be told what to do and every keystroke is an answer to it; a
//! completion menu is a *view of what the composer already holds*, so the
//! caret stays where the text is being typed and only the five keys the menu
//! binds are taken out of the stream ([`PickerAction::of`]). The two can
//! therefore never both be up: `super::shell`'s `ask` dismisses a menu before
//! it installs a question, and the band paints one block, not two.
//!
//! # What opens it, and what closes it
//!
//! A **trigger** is a property of the whole composer rather than of the last
//! keystroke: the draft is one word beginning with `/` and nothing else
//! ([`Trigger::of`]). That is the same shape `crate::interactive::classify`
//! calls a command, which is what keeps the menu from offering a completion for
//! a line that would not be run as one.
//!
//! Escape closes it, and the closing has to *stay* closed while the user keeps
//! typing the same word -- otherwise the next keystroke re-opens the thing they
//! just dismissed. What is remembered is the **trigger kind** and not the query
//! ([`Dismissed`]): `/he` dismissed and grown to `/hel` is still dismissed, and
//! a composer that stops being a slash word at all forgets the dismissal, so
//! typing `/` again opens a menu.
//!
//! # Ranking
//!
//! Three tiers, in this order: a command whose **name** begins with the query,
//! a command one of whose **aliases** begins with it, then a command whose name
//! or alias merely **contains** it. Within a tier the order is
//! `crate::interactive::SLASH_REGISTRY`'s, which is `/help`'s -- so the list is
//! a pure function of the query, and two sessions that typed the same letters
//! see the same rows in the same order. The match is exact and case-sensitive
//! for the reason the parser's is: a surface that guessed would be a surface
//! whose answer depends on how it was feeling.

use crate::interactive::{SlashSpec, SLASH_REGISTRY};

/// The most rows the menu takes, however many commands match.
///
/// Six is the whole registry today, so an ordinary screen shows every match
/// without scrolling; the window exists for the screens -- and the registries --
/// where it cannot ([`Picker::rows`]).
const MOST_ROWS: u16 = 6;

/// The rows the rest of the band needs before the menu may have any.
///
/// The activity row, the divider, one composer row, the hint row, and one row
/// of the terminal's own document. Reserving them here is what makes the menu
/// *unrefusable*: on the shortest screen a band fits on
/// ([`super::layout::MIN_ROWS`]) this leaves one row for the menu, and
/// `super::layout::solve_band` has an answer for that band even with a turn
/// running -- so unlike a question, a completion never has to be refused on the
/// user's behalf.
const RESERVED_ROWS: u16 = 5;

/// What marks the row a completion would take.
///
/// The approval panel's marker (`super::approval`'s `MARKER`), because it means
/// the same thing in the same place: *this is the row the next key acts on*.
const MARKER: &str = "> ";

/// What every other row is written into, so the names line up under each other.
const INDENT: &str = "  ";

/// How wide the name column is: the longest canonical name plus a space.
const NAME_CELLS: usize = 9;

/// How well a command answers a query.
///
/// Ordered by declaration, which is the ranking: `derive(PartialOrd, Ord)` on a
/// fieldless enum is its variant order, and the sort below is what turns that
/// into rows on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    /// The command's own name begins with the query.
    CommandPrefix,
    /// One of its aliases does.
    AliasPrefix,
    /// Its name or an alias contains the query somewhere else in it.
    Substring,
}

/// What kind of completion the composer is asking for.
///
/// One variant, and it is an `enum` rather than a `bool` because
/// [`Dismissed`] remembers *which* kind was dismissed: a file or an at-mention
/// trigger added later must not inherit the dismissal of a slash one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// The draft is one word beginning with `/`.
    Slash,
}

impl Trigger {
    /// The completion `text` is asking for, if it is asking for one.
    ///
    /// The **whole** draft, not the word under the caret: a slash command is a
    /// line whose first character is `/` and whose first word is the name, so
    /// `say /help` is a prompt here exactly as it is to
    /// `crate::interactive::classify`, and `/model acme` has stopped being a
    /// name and become a name with an argument.
    pub(crate) fn of(text: &str) -> Option<Self> {
        (text.starts_with('/') && !text.contains(char::is_whitespace)).then_some(Self::Slash)
    }
}

/// The trigger whose menu the user closed, while it is still the trigger.
///
/// Held by the shell across keystrokes. The whole of its job is that a menu
/// dismissed by Escape does not come back on the next letter of the same word,
/// and that it *does* come back once the word it belonged to is gone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Dismissed(Option<Trigger>);

impl Dismissed {
    /// Records that the menu for `trigger` was closed by the user.
    pub(crate) fn dismiss(&mut self, trigger: Trigger) {
        self.0 = Some(trigger);
    }

    /// Whether a menu may be open for `trigger` now.
    ///
    /// **Reconciling rather than asking**, which is why it takes `&mut self`:
    /// a dismissal outlives only the trigger it was made against, so a call
    /// that finds a different trigger -- or none -- forgets it here rather than
    /// leaving a stale `Some` for some later keystroke to be confused by.
    pub(crate) fn admits(&mut self, trigger: Option<Trigger>) -> bool {
        if self.0.is_some() && self.0 == trigger {
            return false;
        }
        self.0 = None;
        trigger.is_some()
    }
}

/// One command the query found, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Match {
    spec: &'static SlashSpec,
    rank: Rank,
    /// The alias the query matched, when it is not the name that matched.
    ///
    /// Shown on the row, because a menu that listed `/quit` for the letters
    /// `ex` without saying why would look like a bug.
    alias: Option<&'static str>,
}

/// What one keystroke means to an open menu.
///
/// The menu's own vocabulary rather than [`super::input::Action`], for the
/// reason `super::approval::Action` is one: the shell translates, so a key the
/// menu does not bind cannot be swallowed by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerAction {
    /// The previous match.
    Up,
    /// The next one.
    Down,
    /// Put the marked name in the composer. Tab.
    Complete,
    /// The line is being run. Enter -- which the menu gets out of the way of
    /// rather than swallows.
    Submit,
    /// Close it and leave the draft alone. Escape.
    Escape,
}

impl PickerAction {
    /// What an ordinary decoded action means to a menu, and `None` for the
    /// keystrokes that go on meaning what they mean.
    pub(crate) fn of(action: super::input::Action) -> Option<Self> {
        match action {
            super::input::Action::Up => Some(Self::Up),
            super::input::Action::Down => Some(Self::Down),
            super::input::Action::Tab => Some(Self::Complete),
            super::input::Action::Submit => Some(Self::Submit),
            super::input::Action::Escape => Some(Self::Escape),
            _ => None,
        }
    }
}

/// What one keystroke did to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerOutcome {
    /// The marked row moved. A frame, and nothing else.
    Changed,
    /// This canonical name is what the composer should hold.
    Complete(&'static str),
    /// The menu is gone. **Whether the keystroke was also meant for something
    /// else is the caller's to know**: Escape stops here, Enter goes on to
    /// submit the line.
    Dismiss,
}

/// The matches for one query, and which of them is marked.
#[derive(Debug)]
pub(crate) struct Picker {
    /// What produced [`Self::matches`], so a keystroke that did not change the
    /// draft's text does not rebuild the list and move the mark back to the
    /// top.
    query: String,
    /// The trigger this menu belongs to, for the dismissal it may become.
    trigger: Trigger,
    /// Never empty: [`Self::over`] is the only constructor and it refuses a
    /// query nothing matches, which is what makes the arithmetic below total.
    matches: Vec<Match>,
    /// An index into [`Self::matches`].
    selected: usize,
}

impl Picker {
    /// The menu `query` deserves, or `None` when it names nothing.
    pub(crate) fn open(query: &str) -> Option<Self> {
        Self::over(SLASH_REGISTRY, query)
    }

    /// The same, over a registry named by the caller.
    ///
    /// The seam the ranking is tested through: the live registry has no
    /// command whose name begins with an alias's first letters, so the whole
    /// three-tier order cannot be produced from it.
    fn over(registry: &'static [SlashSpec], query: &str) -> Option<Self> {
        let trigger = Trigger::of(query)?;
        let mut matches: Vec<Match> = registry
            .iter()
            .filter_map(|spec| found(spec, query))
            .collect();
        // Stable, so the tie inside a rank is the registry's order rather than
        // whatever the sort happened to do with it.
        matches.sort_by_key(|found| found.rank);
        (!matches.is_empty()).then(|| Self {
            query: query.to_string(),
            trigger,
            matches,
            selected: 0,
        })
    }

    /// What this menu was built for.
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The trigger it belongs to.
    pub(crate) fn trigger(&self) -> Trigger {
        self.trigger
    }

    /// The menu's rows, top first, exactly as many as [`Self::height`] says.
    ///
    /// **One iterator for both**, the same rule the approval panel is built on:
    /// the height the band solved its geometry from is the length of this, so a
    /// row painted without a row of geometry to put it on -- or a row of
    /// geometry with nothing painted into it -- is not a state this can reach.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        // The composer's own window rule, reused rather than restated: which of
        // `n` rows a `limit`-row viewport shows, given the one that must stay
        // visible. A menu is the same question with the mark for a caret.
        let window =
            super::editor::window(self.matches.len(), self.selected, visible(terminal_rows));
        window
            .clone()
            .map(|index| {
                let found = self.matches[index];
                let marker = if index == self.selected {
                    MARKER
                } else {
                    INDENT
                };
                let name = match found.alias {
                    Some(alias) => format!("{} ({alias})", found.spec.name),
                    None => found.spec.name.to_string(),
                };
                // Cut here, by the painter's own rule, for the reason the panel
                // cuts its own rows: a row measured by the band at one width
                // and drawn at another is a band whose height is a guess.
                super::frame::clip(
                    &format!("{marker}{name:<NAME_CELLS$} {}", found.spec.summary),
                    cols,
                )
                .to_string()
            })
            .collect()
    }

    /// How many rows the band has to give it.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        // Bounded by `visible` above, so the narrowing is a proof rather than a
        // policy -- the same shape `super::approval::Panel::height` has.
        u16::try_from(self.rows(cols, terminal_rows).len()).unwrap_or(MOST_ROWS)
    }

    /// What one keystroke does to it.
    pub(crate) fn apply(&mut self, action: PickerAction) -> PickerOutcome {
        let count = self.matches.len();
        match action {
            // Wrapping, like the panel's marker: a list this short has no
            // "past the end" worth stopping at.
            PickerAction::Up => {
                self.selected = (self.selected + count - 1) % count;
                PickerOutcome::Changed
            }
            PickerAction::Down => {
                self.selected = (self.selected + 1) % count;
                PickerOutcome::Changed
            }
            PickerAction::Complete => {
                PickerOutcome::Complete(self.matches[self.selected].spec.name)
            }
            PickerAction::Submit | PickerAction::Escape => PickerOutcome::Dismiss,
        }
    }
}

/// How many matches a screen of `terminal_rows` shows at once.
///
/// At least one on any screen a band fits on: see [`RESERVED_ROWS`].
fn visible(terminal_rows: u16) -> u16 {
    MOST_ROWS
        .min(terminal_rows.saturating_sub(RESERVED_ROWS))
        .max(1)
}

/// Whether `spec` answers `query`, and how well.
///
/// The tiers are tried in their own order and the **first** one that matches is
/// the answer, so a command is on the list once however many of its names the
/// query could be read as.
fn found(spec: &'static SlashSpec, query: &str) -> Option<Match> {
    if spec.name.starts_with(query) {
        return Some(Match {
            spec,
            rank: Rank::CommandPrefix,
            alias: None,
        });
    }
    if let Some(alias) = spec.aliases.iter().find(|alias| alias.starts_with(query)) {
        return Some(Match {
            spec,
            rank: Rank::AliasPrefix,
            alias: Some(alias),
        });
    }
    // The slash is what makes the draft a command rather than a word, so it is
    // not part of what a *substring* is looked for in: `/e` is the letter `e`
    // somewhere in a name, and no name has a slash anywhere but the front.
    let stem = query.strip_prefix('/').unwrap_or(query);
    if stem.is_empty() {
        return None;
    }
    if spec.name.contains(stem) {
        return Some(Match {
            spec,
            rank: Rank::Substring,
            alias: None,
        });
    }
    spec.aliases
        .iter()
        .find(|alias| alias.contains(stem))
        .map(|alias| Match {
            spec,
            rank: Rank::Substring,
            alias: Some(alias),
        })
}

/// What completing `name` puts in the composer.
///
/// A command that takes an argument is completed **with the space**, because
/// the next thing the user types is the argument and a menu that made them
/// press the space bar would have done half a job. One that takes none is
/// completed exactly, so Enter runs it.
pub(crate) fn completed(name: &str) -> String {
    if SLASH_REGISTRY
        .iter()
        .any(|spec| spec.name == name && spec.has_args)
    {
        return format!("{name} ");
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interactive::{Slash, SlashSpec, SLASH_REGISTRY};

    /// A registry that produces all three ranks for one query.
    ///
    /// The live one cannot: no canonical name begins with `e`, so `/ex` has no
    /// command-prefix match to outrank `/exit`. Rather than assert two thirds
    /// of the order and call it the order, the ranking is asked the whole
    /// question here and the **live** alias path is asserted separately, below,
    /// against `SLASH_REGISTRY` itself.
    static THREE_TIERS: &[SlashSpec] = &[
        SlashSpec {
            command: Slash::Help,
            name: "/exercise",
            aliases: &[],
            summary: "a name that begins with the query",
            has_args: false,
        },
        SlashSpec {
            command: Slash::New,
            name: "/settle",
            aliases: &["/exit"],
            summary: "an alias that begins with the query",
            has_args: false,
        },
        SlashSpec {
            command: Slash::Clear,
            name: "/annex",
            aliases: &[],
            summary: "a name that merely contains it",
            has_args: false,
        },
    ];

    fn names(picker: &Picker) -> Vec<&'static str> {
        picker.matches.iter().map(|found| found.spec.name).collect()
    }

    fn ranks(picker: &Picker) -> Vec<Rank> {
        picker.matches.iter().map(|found| found.rank).collect()
    }

    #[test]
    fn command_prefix_ranks_before_alias_prefix_before_substring() {
        let picker = Picker::over(THREE_TIERS, "/ex").expect("three matches");
        assert_eq!(names(&picker), vec!["/exercise", "/settle", "/annex"]);
        assert_eq!(
            ranks(&picker),
            vec![Rank::CommandPrefix, Rank::AliasPrefix, Rank::Substring]
        );

        // The live alias, through the same function: `/e` names no command, so
        // `/quit` is on the screen because `/exit` reaches it -- above every
        // command that merely has an `e` in it.
        let live = Picker::open("/e").expect("the alias and the substrings");
        assert_eq!(live.matches[0].spec.name, "/quit");
        assert_eq!(live.matches[0].rank, Rank::AliasPrefix);
        assert_eq!(live.matches[0].alias, Some("/exit"));
        assert!(
            live.matches[1..]
                .iter()
                .all(|found| found.rank == Rank::Substring),
            "{:?}",
            ranks(&live)
        );
        // Deterministic within a rank: registry order, which is `/help`'s.
        assert_eq!(
            names(&live)[1..],
            ["/help", "/new", "/clear", "/model", "/version"]
        );
    }

    #[test]
    fn dismissal_survives_query_growth_until_the_trigger_kind_changes() {
        let mut dismissed = Dismissed::default();
        assert!(dismissed.admits(Trigger::of("/he")));
        dismissed.dismiss(Trigger::Slash);
        // The query grows; the dismissal is about the trigger, not the text.
        assert!(!dismissed.admits(Trigger::of("/hel")));
        assert!(!dismissed.admits(Trigger::of("/help")));
        // The trigger goes away, and with it the dismissal.
        assert!(!dismissed.admits(Trigger::of("")));
        assert!(dismissed.admits(Trigger::of("/h")));
    }

    #[test]
    fn picker_rows_and_height_come_from_one_iterator() {
        for (rows, cols) in [(24_u16, 80_u16), (6, 20), (40, 200), (10, 24)] {
            let picker = Picker::open("/").expect("every command matches a bare slash");
            assert_eq!(
                picker.height(cols, rows),
                u16::try_from(picker.rows(cols, rows).len()).expect("a bounded count"),
                "{rows}x{cols}"
            );
            assert!(
                picker
                    .rows(cols, rows)
                    .iter()
                    .all(|row| super::super::wrap::width(row) <= cols),
                "{rows}x{cols}"
            );
        }
    }

    #[test]
    fn a_query_that_names_nothing_opens_no_picker() {
        assert!(Picker::open("/zzz").is_none());
        assert!(Trigger::of("say /help to me").is_none());
        assert!(Trigger::of("/model acme").is_none());
    }

    #[test]
    fn completing_a_command_that_takes_an_argument_leaves_room_for_one() {
        assert_eq!(completed("/model"), "/model ");
        assert_eq!(completed("/help"), "/help");
        for spec in SLASH_REGISTRY {
            assert!(completed(spec.name).starts_with(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn the_marked_row_is_the_one_completion_takes() {
        let mut picker = Picker::open("/").expect("every command");
        assert!(matches!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/help")
        ));
        assert!(matches!(
            picker.apply(PickerAction::Down),
            PickerOutcome::Changed
        ));
        assert!(matches!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/new")
        ));
        // Up from the top wraps, exactly as the approval panel's marker does.
        assert!(matches!(
            picker.apply(PickerAction::Up),
            PickerOutcome::Changed
        ));
        assert!(matches!(
            picker.apply(PickerAction::Up),
            PickerOutcome::Changed
        ));
        assert!(matches!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/quit")
        ));
        assert!(matches!(
            picker.apply(PickerAction::Escape),
            PickerOutcome::Dismiss
        ));
        assert!(matches!(
            picker.apply(PickerAction::Submit),
            PickerOutcome::Dismiss
        ));
    }
}
