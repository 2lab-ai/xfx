//! The band `/permissions remember` and `revoke` are confirmed in.
//!
//! Upstream puts a rule change through its approval band in a
//! confirmation-only form (`approval_prompt.syncRuleManagement` with
//! `confirmation_only = true`, `vercel-labs/fx@c1db9191
//! src/core/app/app_commands.zig:890-923`): the approval chrome with the kind
//! on the right reading `Permission rule`, the question, a line saying the
//! action will not run, the action under `$`, and two choices -- `Confirm`
//! and `Cancel` -- with no amendment
//! (`cap/fx-permissions3-40x120-remember-prompt.ansi`):
//!
//! ```text
//!   Permission needed · Choose one                     Permission rule
//!
//!   Remember allow for this saved session?
//!   Confirm this exact saved-session permission rule. The action will not run.
//!   $ write_file a.txt preimage=absent after=2d711642b726b044
//!
//!   ❯ 1. Confirm
//!     2. Cancel
//! ```
//!
//! A surface of its own rather than a mode of `super::approval::Panel`: that
//! one is a tool call's question, three answers wide, and its answer goes to a
//! turn on the runtime thread. This one belongs to no turn -- the UI makes
//! the change itself ([`crate::permission::slash::confirm`]) -- so nothing it
//! does can be mistaken for an approval of a call.

use super::theme::Palette;
use crate::permission::slash::Confirmation;

/// Upstream's title, the approval band's for a question with choices.
const TITLE: &str = "Permission needed · Choose one";
/// What the right of the header calls it (`approvalKind`,
/// `approval_ui.zig:1842-1852`).
const KIND: &str = "Permission rule";
/// The two choices, numbered.
const CHOICES: [&str; 2] = ["1. Confirm", "2. Cancel"];
/// The hint row while it is up (`cap/fx-permissions3-…-remember-prompt`).
const HINT: &str = "1–2 choose    enter confirm    esc cancel";
/// The approval band's inset and right margin (`approval_view`).
const INSET: usize = 2;
const RIGHT_MARGIN: usize = 2;

/// What a keystroke did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    /// Nothing this band binds.
    Ignored,
    /// The mark moved.
    Moved,
    /// `1`, or Enter on it: make the change.
    Confirm,
    /// `2`, or Enter on it: say it was cancelled.
    Cancel,
    /// Escape: close, saying nothing (`cap/fx-permissions3-…-remember-esc`).
    Dismiss,
}

/// One keystroke, in this band's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Digit(char),
    Up,
    Down,
    Enter,
    Escape,
}

/// The pending change and which choice is marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleConfirm {
    confirmation: Confirmation,
    /// `0` is Confirm, which is upstream's mark on opening.
    selected: usize,
}

impl RuleConfirm {
    pub(crate) fn new(confirmation: Confirmation) -> Self {
        Self {
            confirmation,
            selected: 0,
        }
    }

    pub(crate) fn confirmation(&self) -> &Confirmation {
        &self.confirmation
    }

    /// What one keystroke does.
    pub(crate) fn apply(&mut self, key: Key) -> Reply {
        match key {
            Key::Digit('1') => Reply::Confirm,
            Key::Digit('2') => Reply::Cancel,
            Key::Digit(_) => Reply::Ignored,
            Key::Up | Key::Down => {
                self.selected = 1 - self.selected;
                Reply::Moved
            }
            Key::Enter if self.selected == 0 => Reply::Confirm,
            Key::Enter => Reply::Cancel,
            Key::Escape => Reply::Dismiss,
        }
    }

    /// The rows, plain or in `palette`'s colours; as many as [`Self::height`].
    ///
    /// The blank rows are spent only on a screen tall enough for them, as the
    /// approval band's are (`super::approval::SPACIOUS_AT`).
    pub(crate) fn rows(
        &self,
        cols: u16,
        terminal_rows: u16,
        palette: Option<&Palette>,
    ) -> Vec<String> {
        let air = terminal_rows >= super::approval::SPACIOUS_AT;
        let paint = |style: &str, text: &str| match palette {
            Some(palette) if !text.is_empty() => format!("{style}{text}{}", palette.reset()),
            _ => text.to_string(),
        };
        let (bold, strong, quiet) = match palette {
            Some(palette) => (palette.bold(), palette.strong(), palette.statusline()),
            None => ("", "", ""),
        };
        let inset = " ".repeat(INSET);
        let mut rows = Vec::new();

        let inner = usize::from(cols).saturating_sub(INSET + RIGHT_MARGIN);
        let title_cells = super::wrap::width(TITLE) as usize;
        let kind_cells = super::wrap::width(KIND) as usize;
        let mut header = format!("{inset}{}", paint(bold, TITLE));
        if title_cells + kind_cells < inner {
            header.push_str(&" ".repeat(inner - title_cells - kind_cells));
            header.push_str(&paint(quiet, KIND));
        }
        rows.push(header);
        if air {
            rows.push(String::new());
        }
        rows.push(format!(
            "{inset}{}",
            paint(bold, self.confirmation.question)
        ));
        rows.push(format!(
            "{inset}{}",
            paint(quiet, self.confirmation.explanation)
        ));
        // The action as the store shows it -- a person's JSON went into it --
        // made row-safe and wrapped whole, never cut: the confirmation is
        // about exactly this, as a permission question is about its command.
        let display = super::approval_view::named_rows(
            &self.confirmation.display,
            cols.saturating_sub(4).max(1),
        );
        for (index, segment) in display.iter().enumerate() {
            let lead = if index == 0 { "  $ " } else { "    " };
            rows.push(paint(strong, &format!("{lead}{segment}")));
        }
        if air {
            rows.push(String::new());
        }
        for (index, choice) in CHOICES.iter().enumerate() {
            if index == self.selected {
                rows.push(format!("{inset}❯ {}", paint(strong, choice)));
            } else {
                rows.push(format!("{inset}  {choice}"));
            }
        }
        if air {
            rows.push(String::new());
            rows.push(String::new());
        }
        rows.into_iter()
            .map(|row| {
                let cut = super::frame::clip(&row, cols);
                match palette {
                    Some(palette) if cut.len() != row.len() && cut.contains('\u{1b}') => {
                        format!("{cut}{}", palette.reset())
                    }
                    _ => cut.to_string(),
                }
            })
            .collect()
    }

    /// How many rows it takes.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        u16::try_from(self.rows(cols, terminal_rows, None).len()).unwrap_or(u16::MAX)
    }

    /// Which of its rows the marked choice is on, where the caret goes.
    pub(crate) fn marked_row(&self, cols: u16, terminal_rows: u16) -> u16 {
        let rows = self.rows(cols, terminal_rows, None);
        let first = rows
            .iter()
            .position(|row| row.ends_with(CHOICES[0]))
            .unwrap_or(0);
        u16::try_from(first + self.selected).unwrap_or(u16::MAX)
    }

    /// The hint row while it is up.
    pub(crate) fn hint(cols: u16, palette: &Palette) -> String {
        let text = super::frame::clip(HINT, cols);
        if text.is_empty() {
            return String::new();
        }
        format!("{}{text}{}", palette.dim(), palette.reset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::session_rules::{Decision, Event, KeyKind, RuleKey};
    use crate::permission::slash;

    fn remember() -> RuleConfirm {
        RuleConfirm::new(Confirmation {
            question: slash::remember_question(Decision::Allow),
            explanation: slash::remember_explanation(false),
            display: "write_file a.txt preimage=absent after=2d711642b726b044".to_string(),
            path: "/s/permissions.json".into(),
            event: Event::Set {
                key: RuleKey::new(KeyKind::FileMutation, b"k".to_vec()).unwrap(),
                display: "d".to_string(),
                decision: Decision::Allow,
                expected: None,
            },
        })
    }

    #[test]
    fn the_rows_are_upstreams_on_a_tall_screen() {
        // `cap/fx-permissions3-40x120-remember-prompt.txt`, rows 4-13.
        let rows = remember().rows(120, 40, None);
        let header = format!(
            "  Permission needed · Choose one{}Permission rule",
            " ".repeat(116 - 30 - 15)
        );
        assert_eq!(
            rows,
            vec![
                header,
                String::new(),
                "  Remember allow for this saved session?".to_string(),
                "  Confirm this exact saved-session permission rule. The action will not run."
                    .to_string(),
                "  $ write_file a.txt preimage=absent after=2d711642b726b044".to_string(),
                String::new(),
                "  ❯ 1. Confirm".to_string(),
                "    2. Cancel".to_string(),
                String::new(),
                String::new(),
            ]
        );
        assert_eq!(remember().height(120, 40), 10);
        assert_eq!(remember().marked_row(120, 40), 6);
        // A short screen keeps every row that says something.
        assert_eq!(remember().height(120, 24), 6);
    }

    #[test]
    fn the_keys_are_one_two_the_arrows_enter_and_escape() {
        let mut band = remember();
        assert_eq!(band.apply(Key::Digit('3')), Reply::Ignored);
        assert_eq!(band.apply(Key::Enter), Reply::Confirm);
        assert_eq!(band.apply(Key::Down), Reply::Moved);
        assert_eq!(band.marked_row(120, 40), 7);
        assert_eq!(band.apply(Key::Enter), Reply::Cancel);
        assert_eq!(band.apply(Key::Up), Reply::Moved);
        assert_eq!(band.apply(Key::Digit('2')), Reply::Cancel);
        assert_eq!(band.apply(Key::Digit('1')), Reply::Confirm);
        assert_eq!(band.apply(Key::Escape), Reply::Dismiss);
    }

    #[test]
    fn a_display_is_named_as_a_grant_is_and_cannot_be_drawn_past_the_margin() {
        // The rule is what the user saves, so it is spelled as a grant's
        // fields are (`super::super::approval_view::named_rows`).
        use super::super::approval_view::wider_terminal::{hides, runs};
        let mut hidden = Vec::new();
        for (run, character, cells) in runs() {
            let mut confirmation = remember().confirmation().clone();
            confirmation.display = format!("terminal.exec echo x{run}TAILSEEN in .");
            let rows = RuleConfirm::new(confirmation).rows(120, 40, None);
            if let Some(why) = hides(&rows, 120, character, cells, "TAILSEEN") {
                hidden.push(why);
            }
        }
        assert!(hidden.is_empty(), "{hidden:#?}");
        let mut confirmation = remember().confirmation().clone();
        // The RGI three-person family, which text keeps and a grant names.
        confirmation.display =
            "write_file \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}.txt".to_string();
        let rows = RuleConfirm::new(confirmation).rows(120, 40, None);
        assert!(rows.concat().contains("\\u{200D}"), "{rows:?}");
    }

    #[test]
    fn a_display_with_a_control_in_it_reaches_no_terminal() {
        let mut confirmation = remember().confirmation().clone();
        confirmation.display = "terminal.exec printf '\u{1b}[2J' in .".to_string();
        let rows = RuleConfirm::new(confirmation).rows(120, 40, None);
        assert!(rows.iter().all(|row| !row.contains('\u{1b}')), "{rows:?}");
    }
}
