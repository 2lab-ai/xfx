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
//! does can be mistaken for an approval of a call. Its **keys** are the
//! approval's all the same, as upstream's are (the shell's
//! `approval_action`), answered over two choices ([`RuleConfirm::apply`]).

use super::approval::Action;
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
    /// `1`, or Enter or C-j on it: make the change.
    Confirm,
    /// `2`, Enter or C-j on it, or Ctrl-C: say it was cancelled.
    Cancel,
    /// Escape: close, saying nothing; the shell stops a running turn with it.
    Dismiss,
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

    /// What one keystroke does, read as a permission question with no draft
    /// reads it, over upstream's confirmation-only choices
    /// (`setConfirmationOnly`, `approval_decision.zig:82-85@481104fd`): two,
    /// so a third digit is nothing and a move wraps (`optionCount`,
    /// `moveChoice`, `:214-229`; `.number`, `:157-158`); Confirm is `once`
    /// and Cancel the refusal (`selectedDecision`, `:75-80`), so Ctrl-C, the
    /// refusal (`.deny`, `:137-140`), is Cancel. Tab opens no draft -- the
    /// change takes none (`amendment_allowed = false`,
    /// `app_commands.zig:691`) -- so it moves the mark (`.tab`,
    /// `approval_decision.zig:142-151`).
    pub(crate) fn apply(&mut self, action: Action) -> Reply {
        match action {
            Action::Text('1') => Reply::Confirm,
            Action::Text('2') => Reply::Cancel,
            Action::Up | Action::Down | Action::Tab => {
                self.selected = 1 - self.selected;
                Reply::Moved
            }
            Action::Submit if self.selected == 0 => Reply::Confirm,
            Action::Submit | Action::Cancel => Reply::Cancel,
            Action::Escape => Reply::Dismiss,
            Action::Text(_) | Action::Edit(_) | Action::PasteByte(_) => Reply::Ignored,
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
    fn the_keys_are_the_approvals_over_two_choices() {
        let mut band = remember();
        assert_eq!(band.apply(Action::Text('3')), Reply::Ignored);
        assert_eq!(band.apply(Action::Text('j')), Reply::Ignored);
        assert_eq!(band.apply(Action::Submit), Reply::Confirm);
        assert_eq!(band.apply(Action::Down), Reply::Moved);
        assert_eq!(band.marked_row(120, 40), 7);
        assert_eq!(band.apply(Action::Submit), Reply::Cancel);
        assert_eq!(band.apply(Action::Up), Reply::Moved);
        assert_eq!(band.apply(Action::Tab), Reply::Moved);
        assert_eq!(band.marked_row(120, 40), 7, "Tab moves, wrapping");
        assert_eq!(band.apply(Action::Tab), Reply::Moved);
        assert_eq!(band.marked_row(120, 40), 6);
        assert_eq!(band.apply(Action::Cancel), Reply::Cancel);
        assert_eq!(band.apply(Action::Text('2')), Reply::Cancel);
        assert_eq!(band.apply(Action::Text('1')), Reply::Confirm);
        assert_eq!(band.apply(Action::Escape), Reply::Dismiss);
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
