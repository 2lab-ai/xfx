//! The tool group: what the transcript shows for the calls a turn makes
//! between two pieces of its text (`vercel-labs/fx@c1db9191
//! src/ui/transcript/tool_group_projection.zig`).
//!
//! One header and one row per call:
//!
//! ```text
//! ● 5 tool calls · 3 read · 1 list · 1 command
//! ├ Read a.txt
//! ├ Matched **/*.rs
//! └ Running sleep 4; echo hello from shell; ls
//! ```
//!
//! The header counts every call the group shows, by what kind of work it is and
//! then by how the settled ones ended (`formatGroupHeader`, `:497-541`); each
//! row is the call's verb and the one argument it names (`formatGroupBlock`,
//! `:543-654`). A group spans model steps and is closed by the next thing the
//! transcript shows that is not one of its calls -- an answer's text, a line of
//! xfx's own, the end of the turn (`:1280-1375`). That closing is the shell's to
//! decide; this module is the group and the rows it is drawn as.
//!
//! **The rows are rebuilt whole on every change**, because the header counts
//! every child: a call that settles changes its own row and the header above
//! it. So the group is a block the transcript can rewrite while it is open
//! (`super::transcript::Transcript::queue_block`), not lines appended one by
//! one.
//!
//! **The running call is the block's last row**, `└ Running <command>`, and is
//! rewritten in place by its settled row. Upstream draws that row on the
//! footer's side and splices it under the group (`render_input.zig:761-795`,
//! `formatGroupBlock`'s focused entry); here the band has no row of its own for
//! it, and a row the band painted would be a second copy of a call the block
//! already counts. What reaches the screen is the same: the group, its running
//! call connected under it, a blank row, the activity row.
//!
//! Nothing a call printed is shown here: a command's output is not part of the
//! inline transcript (`tool_group_projection.zig:1150-1158`).

use crate::markdown::code_highlight::{self, Variant};
use crate::markdown::{code_highlight_languages, theme as syntax};
use crate::output::safe_one_line;
use crate::tools::{mcp, Activity, LineChanges, Outcome, Shown};

use super::theme::{DocumentRow, Mode, Palette};

/// What a cancelled call's own line ends with, after its mark and its row
/// (`vercel-labs/fx@c1db9191 src/ui/transcript/runtime.zig:644`), in this
/// product's name.
const CANCELLED_FOLLOW_UP: &str = " \u{b7} What can xfx do differently?";

/// How many bytes of a call's own argument, or of its refusal, a row keeps.
///
/// Not the row's width: the row is cut at the screen's, with an ellipsis, when
/// it is written (`super::transcript`'s `ellipsized`). This bound only keeps a
/// four-megabyte command from being copied into every rebuild of the block, and
/// it is wider than any screen a row is cut for.
const TARGET_BYTES: usize = 1024;

/// The kinds of work a header counts, in the order upstream breaks a tie in
/// (`tool_group_projection.zig:140-150`: read, list, write, edit, open,
/// command, ...). `open`, `subagent` and `browser` are upstream tools xfx does
/// not have.
const CATEGORIES: [(Activity, &str); 5] = [
    (Activity::Read, "read"),
    (Activity::List, "list"),
    (Activity::Write, "write"),
    (Activity::Edit, "edit"),
    (Activity::Command, "command"),
];

/// What a call to one of an MCP server's own tools is counted as: a kind of
/// its own, after every one in [`CATEGORIES`] when two counts tie
/// (`vercel-labs/fx@481104fd src/ui/transcript/tool_group_projection.zig:
/// 143-156`, `MCP call` at index 8, after `command`'s 5), and `MCP calls` for
/// more than one (`:542`). Until fx v0.0.16 such a call was counted as a
/// command (`fx-p5-mcp-turn-40x120-turn`: `2 commands` on 0.0.13, `2 MCP
/// calls` on 0.0.16).
const MCP_CALL: &str = "MCP call";

/// The calls of one group, in the order they started.
#[derive(Debug, Default)]
pub(crate) struct ToolGroup {
    calls: Vec<Call>,
    /// The rows last handed to the transcript, or empty while the group has no
    /// block on the screen. A rebuild that comes out the same is not queued
    /// again.
    drawn: Vec<String>,
    /// Whether the block is its header alone: `/settings`' `Collapse tool
    /// calls`, as the shell last set it ([`Self::set_collapsed`]).
    collapsed: bool,
}

#[derive(Debug)]
struct Call {
    id: String,
    shown: Shown,
    state: State,
}

/// Where a call has got to, as its row says it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Running,
    /// It did what it was asked; a file change carries what it changed.
    Done(Option<LineChanges>),
    /// A file change that changed nothing.
    Unchanged,
    /// It refused on its own, for this reason.
    Failed(String),
    /// A command that ran to this status. Zero is a success.
    Exited(i32),
    Signalled(i32),
    TimedOut,
    Denied,
    Cancelled,
}

impl Call {
    /// Whether the call has a row yet.
    ///
    /// A file change has none while it runs, because upstream starts its row
    /// only once the change is settled: the approval that change waits on is
    /// the review of it, and the group shows the calls already decided
    /// (capture `fx-approvals-40x120-write-ask`: the pending write is neither a
    /// row nor counted).
    fn visible(&self) -> bool {
        !(self.changes_a_file() && self.state == State::Running)
    }

    fn changes_a_file(&self) -> bool {
        matches!(
            self.shown.presentation.activity,
            Activity::Write | Activity::Edit
        )
    }

    /// Whether this is a call to one of an MCP server's own tools: upstream
    /// gives every such call a kind of its own, by the `mcp_` its name always
    /// has (`unregisteredActivityKind`, `vercel-labs/fx@481104fd
    /// src/core/tooling/tool_dispatch.zig:576-577`). Here it is the one
    /// presentation every such call is shown with ([`mcp::shown`]), whose
    /// activity stays a command's for what a stop does to it
    /// ([`ToolGroup::running_command`]).
    fn calls_mcp(&self) -> bool {
        self.shown.presentation == mcp::DYNAMIC
    }

    /// Whether the row is a shell command's, whose words the highlighter
    /// colours: not an MCP call's, which upstream leaves in the row's grey
    /// since it stopped counting one as a command (`highlightCommandPhrase`,
    /// `tool_group_projection.zig:749@481104fd`; `fx-p5-mcp-turn-40x120-turn`
    /// row 7, `MCP` in 252 on 0.0.13 and 245 on 0.0.16).
    fn runs_a_command(&self) -> bool {
        self.shown.presentation.activity == Activity::Command && !self.calls_mcp()
    }

    /// The argument the row names, flattened to one line.
    ///
    /// A file change that was refused or stopped is named by the tool's own
    /// word (`Denied file`, `Cancelled file`), as upstream's is: its row names
    /// the file it was given only once it has been given one
    /// (`tool_presentation.zig` `formatPlainAction`, `display_target`).
    ///
    /// An MCP call that did not run to its end -- it failed, was refused or
    /// was stopped -- is named by its tool alone: `MCP` belongs to the verbs
    /// upstream gives such a call while it runs and once it has (`Running
    /// MCP`, `Ran MCP`), and to no other outcome (`dynamicMcpActionLabel`,
    /// `vercel-labs/fx@481104fd src/core/app/app_agent_runtime.zig:1376-1381`,
    /// the same at c1db9191; `fx-p5-mcp-live-40x120-cancel` row 17,
    /// `Cancelled mcp_web_echo`, and `-failed`, `Failed mcp_web_echo: …`).
    /// The name is a server's tool's, so it is spelled as every name a server
    /// supplies is ([`crate::mcp::terminal_safe`]).
    fn target(&self) -> String {
        let default = self.shown.presentation.default_target;
        let refused = matches!(self.state, State::Denied | State::Cancelled);
        let ran = matches!(self.state, State::Running | State::Done(_));
        match &self.shown.target {
            Some(target) if self.calls_mcp() => {
                let named = if ran {
                    target.as_str()
                } else {
                    target.strip_prefix("MCP ").unwrap_or(target)
                };
                crate::mcp::terminal_safe(named)
            }
            Some(target) if !(refused && self.changes_a_file()) => spelled(target),
            _ => default.to_string(),
        }
    }

    /// The row's words before the argument: the verb, or the outcome.
    fn label(&self) -> String {
        let presentation = self.shown.presentation;
        match &self.state {
            State::Running => presentation.running.to_string(),
            State::Done(_) | State::Exited(0) => presentation.done.to_string(),
            // Upstream's row is the tool's own sentence, `No changes to a.txt;
            // it already contains the requested content` (the result text,
            // `file_mutation_execution.zig:88`, used as the status at
            // `tool_presentation.zig:1046-1047`): the verb would claim an edit
            // that was not made. [`Self::phrase`] finishes the sentence.
            State::Unchanged => "No changes to".to_string(),
            State::Failed(_) => "Failed".to_string(),
            State::Exited(code) => format!("Exited {code}"),
            // Upstream's spelling (`commandOutcomeDecision`).
            State::Signalled(signal) => format!("Signaled {signal}"),
            State::TimedOut => "Timed out".to_string(),
            State::Denied => "Denied".to_string(),
            State::Cancelled => "Cancelled".to_string(),
        }
    }

    /// The row after its connector, plain: what [`Self::row`] paints.
    fn phrase(&self) -> String {
        let mut phrase = format!("{} {}", self.label(), self.target());
        match &self.state {
            State::Failed(reason) if !reason.is_empty() => {
                phrase.push_str(": ");
                if self.calls_mcp() {
                    phrase.push_str(&server_spelled(reason));
                } else {
                    phrase.push_str(&flattened(reason));
                }
            }
            // `formatToolStatusWithStats`: both counts, or the one that is
            // not zero, and nothing for a change of nothing. Monochrome, as
            // upstream's are until a theme is pinned (`render.zig:93-105`).
            State::Done(Some(changes)) => match (changes.added, changes.removed) {
                (0, 0) => {}
                (added, 0) => phrase.push_str(&format!(" +{added}")),
                (0, removed) => phrase.push_str(&format!(" -{removed}")),
                (added, removed) => phrase.push_str(&format!(" +{added} / -{removed}")),
            },
            State::Unchanged => phrase.push_str("; it already contains the requested content"),
            _ => {}
        }
        phrase
    }

    /// The row, after its connector, in the row's colour -- with a command's
    /// words re-coloured by the shell highlighter inside it
    /// (`highlightCommandPhrase`, `:728`).
    fn painted(&self, base: &str, palette: Palette) -> String {
        if !self.runs_a_command() {
            return self.phrase();
        }
        format!(
            "{} {}",
            self.label(),
            highlighted(&self.target(), base, palette)
        )
    }

    fn settled(&self) -> bool {
        self.state != State::Running
    }
}

/// `command` with its shell words coloured and everything between them in
/// `base`, or `command` as it is when there is nothing in it to colour
/// (`highlightCommandPhrase`: "commands without tokens keep their exact plain
/// bytes").
fn highlighted(command: &str, base: &str, palette: Palette) -> String {
    let Some(profile) = code_highlight_languages::resolve("sh") else {
        return command.to_string();
    };
    let light = palette.mode == Mode::Light;
    let variant = if light { Variant::Light } else { Variant::Dark };
    let theme = syntax::builtin(light);
    if code_highlight::highlight(command, profile, variant, None, &theme) == command {
        return command.to_string();
    }
    code_highlight::highlight(command, profile, variant, Some(base), &theme)
}

impl ToolGroup {
    /// Records a call that started. A question is not recorded at all: it is
    /// never part of a group (`statusNamesAsk`, `:228`), and its answer is the
    /// question surface's to show.
    pub(crate) fn start(&mut self, id: String, shown: Shown) {
        if shown.presentation.activity == Activity::Ask {
            return;
        }
        self.calls.push(Call {
            id,
            shown,
            state: State::Running,
        });
    }

    /// Settles the running call `id`, and says whether there was one.
    ///
    /// `false` is an answer: a question's result, or a call this group already
    /// settled another way.
    pub(crate) fn finish(
        &mut self,
        id: &str,
        ok: bool,
        detail: &str,
        outcome: Outcome,
        changes: Option<LineChanges>,
    ) -> bool {
        let Some(call) = self
            .calls
            .iter_mut()
            .find(|call| call.id == id && call.state == State::Running)
        else {
            return false;
        };
        call.state = match outcome {
            Outcome::Unchanged => State::Unchanged,
            Outcome::Denied => State::Denied,
            Outcome::Exited(code) => State::Exited(code),
            Outcome::Signalled(signal) => State::Signalled(signal),
            Outcome::TimedOut => State::TimedOut,
            Outcome::Cancelled => State::Cancelled,
            Outcome::Finished if ok => State::Done(changes),
            Outcome::Finished => State::Failed(detail.to_string()),
        };
        true
    }

    /// Settles every call still running as cancelled, and answers with their
    /// ids: the turn they belonged to has stopped.
    pub(crate) fn cancel_running(&mut self) -> Vec<String> {
        self.calls
            .iter_mut()
            .filter(|call| call.state == State::Running)
            .map(|call| {
                call.state = State::Cancelled;
                call.id.clone()
            })
            .collect()
    }

    /// Draws the block as its header alone, or as the header and its calls
    /// (`formatGroupBlock`'s `collapse_tool_calls`,
    /// `tool_group_projection.zig:561-565`): collapsed, the calls' rows go --
    /// the running one too (`fx-settings-collapse-40x120-running`: the header,
    /// a blank row and the activity row) -- and so does a cancelled call's own
    /// line, which upstream builds after that early return. The header still
    /// counts every call.
    ///
    /// Read by the next [`Self::redraw`], so a change is drawn the next time
    /// the block is: the shell redraws an open group at once.
    pub(crate) fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
    }

    /// Whether any call of the group has not returned yet.
    pub(crate) fn running(&self) -> bool {
        self.calls.iter().any(|call| call.state == State::Running)
    }

    /// Whether a command is among the calls that have not returned: the one
    /// kind of call a stop reaches while it runs (`crate::tools::terminal`'s
    /// supervisor polls the turn's flag), so the one a keystroke may settle.
    pub(crate) fn running_command(&self) -> bool {
        self.calls.iter().any(|call| {
            call.state == State::Running && call.shown.presentation.activity == Activity::Command
        })
    }

    /// Whether the group's block is on the screen (or on its way there).
    pub(crate) fn drawn(&self) -> bool {
        !self.drawn.is_empty()
    }

    /// The rows the block should hold now, or `None` when they are the ones it
    /// already holds -- or when the group has nothing to show.
    ///
    /// `trailer` is what was written under the group while it was open --
    /// xfx's own lines, already laid out -- and is the block's last rows: the
    /// group goes on growing above them, as upstream's does above whatever
    /// follows its first call in the same turn (`tool_presentation.zig:667-708`,
    /// one presentation group per turn, drawn at its anchor).
    pub(crate) fn redraw(&mut self, palette: Palette, trailer: &[String]) -> Option<Vec<String>> {
        let mut rows = self.rows(palette);
        if rows.is_empty() {
            return None;
        }
        rows.extend_from_slice(trailer);
        if rows == self.drawn {
            return None;
        }
        self.drawn.clone_from(&rows);
        Some(rows)
    }

    /// What is left of this group once its block is closed: the calls that
    /// have not settled, with no block on the screen.
    ///
    /// They stay a group's because their results are still coming, and a
    /// result has to find the call it settles. The block they were drawn in is
    /// the terminal's now and keeps saying what was true when it closed; the
    /// next change draws them again under whatever closed it.
    pub(crate) fn unsettled(self) -> Self {
        Self {
            calls: self
                .calls
                .into_iter()
                .filter(|call| !call.settled())
                .collect(),
            drawn: Vec::new(),
            collapsed: self.collapsed,
        }
    }

    /// What each row of [`Self::rows`] is, in the same order, for a theme
    /// report that later repaints the block (`super::theme::DocumentRow`): the
    /// header and the call rows are the group's, a cancelled call's own line
    /// is a stopped one, and the blank rows are plain.
    ///
    /// Beside [`Self::rows`] rather than inside it, so the rows stay the text
    /// they are; [`Self::redraw`]'s caller pairs the two.
    pub(crate) fn kinds(&self) -> Vec<DocumentRow> {
        let shown = self.calls.iter().filter(|call| call.visible()).count();
        if shown == 0 {
            return Vec::new();
        }
        if self.collapsed {
            return vec![DocumentRow::ToolGroup, DocumentRow::Plain];
        }
        let mut kinds = vec![DocumentRow::ToolGroup; shown + 1];
        kinds.push(DocumentRow::Plain);
        for _ in self
            .calls
            .iter()
            .filter(|call| call.visible() && call.state == State::Cancelled)
        {
            kinds.push(DocumentRow::ToolStopped);
            kinds.push(DocumentRow::Plain);
        }
        kinds
    }

    /// The block: the header, one row per call that has one, a blank row, and
    /// for each cancelled call its own line and a blank row under it
    /// (`formatGroupBlock`, `:637-651`).
    ///
    /// The blank row is the block's own and is there while it is open, because
    /// the block is the document's last thing and every block in this
    /// transcript ends on one: it is what stands between the group and the
    /// activity row under it, and between the group and whatever closes it.
    fn rows(&self, palette: Palette) -> Vec<String> {
        let shown: Vec<&Call> = self.calls.iter().filter(|call| call.visible()).collect();
        let mut rows = self.header_and_calls(palette, palette.statusline());
        if rows.is_empty() {
            return rows;
        }
        if self.collapsed {
            rows.truncate(1);
            rows.push(String::new());
            return rows;
        }
        rows.push(String::new());
        for call in shown.iter().filter(|call| call.state == State::Cancelled) {
            rows.push(cancelled_line(call, palette));
            rows.push(String::new());
        }
        rows
    }

    /// The header and one row per call that has one, as the full transcript
    /// shows them (`super::full_transcript`): the calls in the default
    /// foreground with only a command's words coloured, where the inline block
    /// writes them in the statusline grey (`fx-fulltranscript-40x120-full.ansi`
    /// against `-end.ansi`), and nothing under them -- the blank row and a
    /// cancelled call's own line are the inline block's.
    pub(crate) fn expanded(&self, palette: Palette) -> Vec<String> {
        self.header_and_calls(palette, "")
    }

    /// The header, then each visible call's row with `text` around it.
    fn header_and_calls(&self, palette: Palette, text: &str) -> Vec<String> {
        let shown: Vec<&Call> = self.calls.iter().filter(|call| call.visible()).collect();
        let Some(last) = shown.len().checked_sub(1) else {
            return Vec::new();
        };
        let reset = palette.reset();
        let mut rows = Vec::with_capacity(shown.len() + 2);
        rows.push(format!(
            "{}\u{25cf}{reset} {}{}{reset}",
            palette.hint(),
            palette.statusline(),
            summary(&shown)
        ));
        for (index, call) in shown.iter().enumerate() {
            let connector = if index == last {
                '\u{2514}'
            } else {
                '\u{251c}'
            };
            rows.push(format!(
                "{text}{connector} {}{reset}",
                call.painted(text, palette)
            ));
        }
        rows
    }
}

/// The header's words after its mark: the count, then each kind of work by how
/// many calls did it, then each way the settled ones went wrong
/// (`formatGroupHeader`, `:497-541`). An MCP call is counted as [`MCP_CALL`]
/// and as nothing else.
fn summary(calls: &[&Call]) -> String {
    let total = calls.len();
    let mut out = format!("{total} tool call{}", if total == 1 { "" } else { "s" });
    let mcp_calls = calls.iter().filter(|call| call.calls_mcp()).count();
    let mut counted: Vec<(usize, usize, &str)> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(order, (activity, label))| {
            let count = calls
                .iter()
                .filter(|call| call.shown.presentation.activity == *activity && !call.calls_mcp())
                .count();
            (count, order, *label)
        })
        .chain([(mcp_calls, CATEGORIES.len(), MCP_CALL)])
        .filter(|(count, _, _)| *count > 0)
        .collect();
    // By count, most first; a tie keeps the categories' own order.
    counted.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (count, _, label) in counted {
        let plural = if matches!(label, "command" | MCP_CALL) && count != 1 {
            "s"
        } else {
            ""
        };
        out.push_str(&format!(" \u{b7} {count} {label}{plural}"));
    }
    for label in WENT_WRONG {
        let count = calls
            .iter()
            .filter(|call| call.state.went_wrong() == Some(label))
            .count();
        if count > 0 {
            out.push_str(&format!(" \u{b7} {count} {label}"));
        }
    }
    out
}

/// The ways a settled call can have gone wrong, in the order a header counts
/// them (`tool_group_projection.zig:529-534`; `unreported` and `not executed`
/// are upstream dispositions xfx's runtime has no way to reach).
const WENT_WRONG: [&str; 4] = ["timed out", "failed", "denied", "cancelled"];

impl State {
    /// Which of [`WENT_WRONG`] this is, if any.
    ///
    /// A command that ran and exited non-zero is a failure here though it was
    /// not one to the model (`observeTool`, `:269-303`).
    fn went_wrong(&self) -> Option<&'static str> {
        match self {
            State::TimedOut => Some("timed out"),
            State::Failed(_) | State::Signalled(_) => Some("failed"),
            State::Exited(code) if *code != 0 => Some("failed"),
            State::Denied => Some("denied"),
            State::Cancelled => Some("cancelled"),
            State::Running | State::Done(_) | State::Unchanged | State::Exited(_) => None,
        }
    }
}

/// A cancelled call's own line under the group: the warning mark, the outcome
/// in the hint colour, the argument in the tool colour, and the question
/// upstream asks after a stop (`transcript/runtime.zig:633-680`,
/// `lifecycleTerminalLine`).
fn cancelled_line(call: &Call, palette: Palette) -> String {
    let reset = palette.reset();
    // Upstream's `tool_stdout_style`: 245 on either screen, where the rows
    // above it are the statusline's 241 on a light one.
    let base = palette.tool_output();
    let target = if call.runs_a_command() {
        highlighted(&call.target(), base, palette)
    } else {
        call.target()
    };
    format!(
        "{}\u{25a0}{reset}{} Cancelled{reset} {base}{target}{reset}{CANCELLED_FOLLOW_UP}",
        palette.warning(),
        palette.hint(),
    )
}

/// A call's argument as the row shows it: as the model wrote it, spaces
/// around it included -- upstream's `Failed   missing.txt  :`
/// (`fx-read-failures-40x120-live`) -- with every control character a space
/// and the length bounded, as [`safe_one_line`] bounds it.
fn spelled(target: &str) -> String {
    let mut out = String::new();
    for character in target.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if out.len() + character.len_utf8() > TARGET_BYTES {
            out.push('\u{2026}');
            break;
        }
        out.push(character);
    }
    out
}

/// A failure's reason as the row shows it: one line, every run of white
/// space one space and none at either end (`encodeTerminalSafeInline`,
/// `vercel-labs/fx@c1db9191 src/core/shared/text_utils.zig:387-407`, used for
/// the row at `tool_presentation.zig:1265-1290`).
fn flattened(reason: &str) -> String {
    let words: Vec<&str> = reason
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect();
    safe_one_line(&words.join(" "), TARGET_BYTES)
}

/// An MCP call's failure reason as the row shows it. Its words can be the
/// server's own (an `isError` result's text), so it is spelled as every text
/// a server supplies is: upstream's `encodeTerminalSafeInline`
/// (`vercel-labs/fx@481104fd src/core/shared/text_utils.zig:397-417`), ASCII
/// white space flattened to one space and then [`crate::mcp::terminal_safe`]
/// -- every other control and every invisible or bidirectional code point
/// written out rather than dropped, and the length bounded. Since fx v0.0.16
/// such a row carries its reason, where a command's row did not
/// (`fx-p5-mcp-live-40x120-failed` row 25).
fn server_spelled(reason: &str) -> String {
    let words: Vec<&str> = reason
        .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
        .filter(|word| !word.is_empty())
        .collect();
    crate::mcp::terminal_safe(&words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tools::mutate::{EDIT_FILE, WRITE_FILE};
    use crate::tools::question::ASK_USER_QUESTION;
    use crate::tools::ToolSpec;
    use crate::tools::{read, terminal};
    use serde_json::json;

    use super::super::theme::Depth;

    const DARK: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// Upstream's grayscale greys, written out rather than read off the
    /// palette, so a palette that drifted is caught here rather than agreed
    /// with.
    const HINT: &str = "\u{1b}[38;5;255m";
    const TEXT: &str = "\u{1b}[38;5;245m";
    const WORD: &str = "\u{1b}[38;5;252m";
    const RESET: &str = "\u{1b}[0m";
    const FG_OFF: &str = "\u{1b}[39m";

    fn shown(spec: &ToolSpec, input: serde_json::Value) -> Shown {
        spec.shown(&input)
    }

    fn command(text: &str) -> Shown {
        shown(
            &terminal::TERMINAL,
            json!({"action": "exec", "command": text}),
        )
    }

    /// The rows with every colour taken off, which is what the shape of the
    /// block is about; the colours have cases of their own.
    fn plain(group: &ToolGroup) -> Vec<String> {
        group.rows(DARK).iter().map(|row| strip(row)).collect()
    }

    fn strip(row: &str) -> String {
        let mut out = String::new();
        let mut rest = row;
        while let Some(at) = rest.find('\u{1b}') {
            out.push_str(&rest[..at]);
            let tail = &rest[at..];
            let end = tail.find('m').map_or(tail.len(), |end| end + 1);
            rest = &tail[end..];
        }
        out.push_str(rest);
        out
    }

    fn ok(group: &mut ToolGroup, id: &str) {
        assert!(group.finish(id, true, "", Outcome::Finished, None));
    }

    /// Upstream's `tools-auto` scenario at its first capture (`t1`): four reads
    /// done and the command still running.
    fn five_calls_one_running() -> ToolGroup {
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "a.txt"})),
        );
        group.start(
            "2".into(),
            shown(&read::READ_FILE, json!({"path": "b.txt"})),
        );
        ok(&mut group, "1");
        ok(&mut group, "2");
        group.start(
            "3".into(),
            shown(&read::GLOB_FILES, json!({"pattern": "**/*.rs"})),
        );
        ok(&mut group, "3");
        group.start(
            "4".into(),
            shown(&read::GREP_FILES, json!({"pattern": "alpha"})),
        );
        ok(&mut group, "4");
        group.start("5".into(), command("sleep 4; echo hello from shell; ls"));
        group
    }

    #[test]
    fn a_running_command_is_the_last_row_and_is_already_counted() {
        // fx `fx-tools-auto-40x120-t1.txt` rows 7-13.
        assert_eq!(
            plain(&five_calls_one_running()),
            [
                "\u{25cf} 5 tool calls \u{b7} 3 read \u{b7} 1 list \u{b7} 1 command",
                "\u{251c} Read a.txt",
                "\u{251c} Read b.txt",
                "\u{251c} Matched **/*.rs",
                "\u{251c} Searched alpha",
                "\u{2514} Running sleep 4; echo hello from shell; ls",
                "",
            ]
        );
    }

    #[test]
    fn the_header_and_every_row_carry_upstreams_colours() {
        // The capture's own bytes, as tmux re-serializes them, say: the mark in
        // the hint grey, the rest of the header and every row in the
        // statusline grey, and a command's words lifted to 252 inside it. Each
        // word is closed with the highlighter's own foreground reset and the
        // row's grey taken up again after it, which tmux shows as the bare
        // switch back to 245.
        let rows = five_calls_one_running().rows(DARK);
        assert_eq!(
            rows[0],
            format!("{HINT}\u{25cf}{RESET} {TEXT}5 tool calls \u{b7} 3 read \u{b7} 1 list \u{b7} 1 command{RESET}")
        );
        assert_eq!(rows[1], format!("{TEXT}\u{251c} Read a.txt{RESET}"));
        let running = &rows[5];
        assert!(
            running.starts_with(&format!(
                "{TEXT}\u{2514} Running {TEXT}{WORD}sleep{FG_OFF}{TEXT} 4"
            )),
            "{running:?}"
        );
        assert!(
            running.ends_with(&format!("{WORD}ls{FG_OFF}{TEXT}{RESET}")),
            "{running:?}"
        );
    }

    #[test]
    fn a_settled_group_counts_by_kind_most_first_and_names_its_changes() {
        // fx `t10`: the command ran, then an edit and a write.
        let mut group = five_calls_one_running();
        assert!(group.finish("5", true, "", Outcome::Exited(0), None));
        group.start(
            "6".into(),
            shown(
                &EDIT_FILE,
                json!({"path": "a.txt", "old_string": "a", "new_string": "b"}),
            ),
        );
        assert!(group.finish(
            "6",
            true,
            "",
            Outcome::Finished,
            Some(LineChanges {
                added: 1,
                removed: 1
            })
        ));
        group.start(
            "7".into(),
            shown(&WRITE_FILE, json!({"path": "c.txt", "content": "x"})),
        );
        assert!(group.finish(
            "7",
            true,
            "",
            Outcome::Finished,
            Some(LineChanges {
                added: 2,
                removed: 0
            })
        ));
        assert_eq!(
            plain(&group)[0],
            "\u{25cf} 7 tool calls \u{b7} 3 read \u{b7} 1 list \u{b7} 1 write \u{b7} 1 edit \u{b7} 1 command"
        );
        assert_eq!(
            &plain(&group)[5..],
            [
                "\u{251c} Ran sleep 4; echo hello from shell; ls",
                "\u{251c} Edited a.txt +1 / -1",
                "\u{2514} Wrote c.txt +2",
                "",
            ]
        );
    }

    #[test]
    fn each_way_a_call_goes_wrong_has_its_own_row_and_its_own_count() {
        // fx `fx-tools-single-40x120-s5`: a missing file, a command that exited
        // 3, then a read that worked.
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "missing.txt"})),
        );
        assert!(group.finish(
            "1",
            false,
            "Path not found: missing.txt",
            Outcome::Finished,
            None
        ));
        group.start("2".into(), command("echo boom; echo err >&2; exit 3"));
        assert!(group.finish("2", true, "", Outcome::Exited(3), None));
        group.start(
            "3".into(),
            shown(&read::READ_FILE, json!({"path": "a.txt"})),
        );
        ok(&mut group, "3");
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 3 tool calls \u{b7} 2 read \u{b7} 1 command \u{b7} 2 failed",
                "\u{251c} Failed missing.txt: Path not found: missing.txt",
                "\u{251c} Exited 3 echo boom; echo err >&2; exit 3",
                "\u{2514} Read a.txt",
                "",
            ]
        );
    }

    #[test]
    fn a_target_keeps_its_spaces_and_a_reason_is_one_flattened_line_or_nothing() {
        // `fx-read-failures-40x120-live`: the argument as written
        // (`Failed   missing.txt  :`), the preflight's sentence with its runs
        // of spaces made one (`encodeTerminalSafeInline`,
        // `text_utils.zig:387-407`), and an in-call failure with no reason.
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "  missing.txt  "})),
        );
        assert!(group.finish(
            "1",
            false,
            "Path not found:   missing.txt  ",
            Outcome::Finished,
            None
        ));
        group.start("2".into(), shown(&read::READ_FILE, json!({"path": "src"})));
        assert!(group.finish("2", false, "", Outcome::Finished, None));
        assert_eq!(
            plain(&group)[1..3],
            [
                "\u{251c} Failed   missing.txt  : Path not found: missing.txt",
                "\u{2514} Failed src",
            ]
        );
    }

    #[test]
    fn a_refused_file_change_is_named_by_the_tools_word_and_a_refused_command_by_itself() {
        // fx `fx-approvals-40x120-cmd2` and `fx-approvals-big-…-end`.
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&WRITE_FILE, json!({"path": "c.txt", "content": "x"})),
        );
        assert!(group.finish("1", false, "no", Outcome::Denied, None));
        group.start("2".into(), command("echo hi"));
        assert!(group.finish("2", false, "no", Outcome::Denied, None));
        group.start("3".into(), command("echo again"));
        assert!(group.finish("3", true, "", Outcome::Exited(0), None));
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 3 tool calls \u{b7} 2 commands \u{b7} 1 write \u{b7} 2 denied",
                "\u{251c} Denied file",
                "\u{251c} Denied echo hi",
                "\u{2514} Ran echo again",
                "",
            ]
        );
    }

    #[test]
    fn a_command_xfx_stopped_or_a_signal_killed_says_so() {
        let mut group = ToolGroup::default();
        group.start("1".into(), command("sleep 999"));
        assert!(group.finish("1", false, "", Outcome::TimedOut, None));
        group.start("2".into(), command("yes"));
        assert!(group.finish("2", true, "", Outcome::Signalled(9), None));
        assert_eq!(
            plain(&group)[..3],
            [
                "\u{25cf} 2 tool calls \u{b7} 2 commands \u{b7} 1 timed out \u{b7} 1 failed",
                "\u{251c} Timed out sleep 999",
                "\u{2514} Signaled 9 yes",
            ]
        );
    }

    #[test]
    fn collapsed_the_block_is_its_header_and_a_blank_row_and_still_counts_every_call() {
        // `/settings`' `Collapse tool calls` (`formatGroupBlock`'s early
        // return, `tool_group_projection.zig:561-565`): the running command's
        // row goes too (`fx-settings-collapse-40x120-running`).
        let mut group = five_calls_one_running();
        group.set_collapsed(true);
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 5 tool calls \u{b7} 3 read \u{b7} 1 list \u{b7} 1 command",
                "",
            ]
        );
        assert_eq!(
            group.kinds(),
            [DocumentRow::ToolGroup, DocumentRow::Plain],
            "a kind per row, or a theme report recolours the wrong rows"
        );
        // A stopped call keeps its count and loses its own line.
        assert_eq!(group.cancel_running(), ["5".to_string()]);
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 5 tool calls \u{b7} 3 read \u{b7} 1 list \u{b7} 1 command \u{b7} 1 cancelled",
                "",
            ]
        );
        // Drawn again expanded once it is turned off, and the full transcript
        // never collapses.
        assert!(group.redraw(DARK, &[]).is_some());
        group.set_collapsed(false);
        assert_eq!(group.redraw(DARK, &[]).map(|rows| rows.len()), Some(9));
        group.set_collapsed(true);
        assert_eq!(group.expanded(DARK).len(), 6);
        // What is left of a closed group stays collapsed.
        assert!(group.unsettled().collapsed);
    }

    #[test]
    fn a_stopped_turn_cancels_what_was_running_and_says_so_under_the_group() {
        // fx `fx-interrupt-40x120-cmd-cc`: the row, the count, and the line.
        let mut group = ToolGroup::default();
        group.start("1".into(), command("sleep 30"));
        assert_eq!(group.cancel_running(), ["1".to_string()]);
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 1 tool call \u{b7} 1 command \u{b7} 1 cancelled",
                "\u{2514} Cancelled sleep 30",
                "",
                "\u{25a0} Cancelled sleep 30 \u{b7} What can xfx do differently?",
                "",
            ]
        );
        let line = &group.rows(DARK)[3];
        assert!(
            line.starts_with(&format!(
                "{WORD}\u{25a0}{RESET}{HINT} Cancelled{RESET} {TEXT}"
            )),
            "{line:?}"
        );
        // A second stop finds nothing left to cancel.
        assert!(group.cancel_running().is_empty());
        assert!(!group.finish("1", true, "", Outcome::Exited(0), None));
    }

    #[test]
    fn a_file_change_has_no_row_until_it_is_settled() {
        // fx `fx-approvals-40x120-write-ask`: the write waiting on its approval
        // is neither a row nor counted; `after-esc`: stopped there, it is a
        // `Cancelled file`.
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(
                &EDIT_FILE,
                json!({"path": "a.txt", "old_string": "a", "new_string": "b"}),
            ),
        );
        assert!(group.rows(DARK).is_empty());
        assert_eq!(
            group.redraw(DARK, &[]),
            None,
            "a block with nothing in it was drawn"
        );
        group.cancel_running();
        assert_eq!(
            &plain(&group)[..2],
            [
                "\u{25cf} 1 tool call \u{b7} 1 edit \u{b7} 1 cancelled",
                "\u{2514} Cancelled file"
            ]
        );
    }

    #[test]
    fn a_question_is_never_part_of_a_group() {
        let mut group = ToolGroup::default();
        group.start(
            "q".into(),
            shown(&ASK_USER_QUESTION, json!({"questions": []})),
        );
        assert!(group.rows(DARK).is_empty());
        assert!(!group.finish("q", true, "", Outcome::Finished, None));
    }

    #[test]
    fn a_rebuild_that_changed_nothing_is_not_drawn_again() {
        let mut group = ToolGroup::default();
        group.start("1".into(), command("ls"));
        assert!(group.redraw(DARK, &[]).is_some());
        assert!(group.drawn());
        assert_eq!(group.redraw(DARK, &[]), None);
        assert!(group.finish("1", true, "", Outcome::Exited(0), None));
        assert!(group.redraw(DARK, &[]).is_some());
    }

    #[test]
    fn a_closed_group_keeps_only_the_calls_still_waiting_for_their_results() {
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "a.txt"})),
        );
        ok(&mut group, "1");
        group.start("2".into(), command("sleep 5"));
        assert!(group.redraw(DARK, &[]).is_some());
        let mut rest = group.unsettled();
        assert!(!rest.drawn(), "the closed block is the terminal's now");
        assert!(rest.finish("2", true, "", Outcome::Exited(0), None));
        assert_eq!(
            plain(&rest)[..2],
            [
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "\u{2514} Ran sleep 5"
            ]
        );
    }

    #[test]
    fn a_target_is_one_line_and_a_missing_one_is_the_tools_word() {
        let mut group = ToolGroup::default();
        group.start("1".into(), command("echo a\necho b"));
        // `list_files` with no path lists the workspace root, and its row
        // says so in upstream's word for it.
        group.start(
            "2".into(),
            Shown {
                presentation: read::LIST_FILES.presentation(),
                target: None,
            },
        );
        let rows = plain(&group);
        assert_eq!(rows[1], "\u{251c} Running echo a echo b");
        assert_eq!(rows[2], "\u{2514} Listing .");
    }

    #[test]
    fn a_change_that_changed_nothing_is_the_tools_own_sentence() {
        // `tool_presentation.zig:1046-1047`: a no-op file change is shown as
        // the tool's sentence, with no verb and no counts, and it is not a
        // failure (`fx-toolgroup-noop-40x120-end`).
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(
                &EDIT_FILE,
                json!({"path": "a.txt", "old_string": "a", "new_string": "a"}),
            ),
        );
        assert!(group.finish(
            "1",
            true,
            "a.txt is already what was asked for",
            Outcome::Unchanged,
            Some(LineChanges::default())
        ));
        assert_eq!(
            plain(&group)[..2],
            [
                "\u{25cf} 1 tool call \u{b7} 1 edit",
                "\u{2514} No changes to a.txt; it already contains the requested content"
            ]
        );
    }

    #[test]
    fn lines_written_under_an_open_group_are_its_last_rows_and_it_grows_above_them() {
        // `fx-approvals-40x120-cmd2`: the call after `┃ use capitals` is
        // counted in the same header and drawn above it.
        let mut group = ToolGroup::default();
        group.start("1".into(), command("echo hi"));
        assert!(group.finish("1", true, "", Outcome::Exited(0), None));
        let trailer = ["[you] use capitals".to_string()];
        assert!(group.redraw(DARK, &trailer).is_some());
        group.start("2".into(), command("echo again"));
        let rows: Vec<String> = group
            .redraw(DARK, &trailer)
            .expect("the block changed")
            .iter()
            .map(|row| strip(row))
            .collect();
        assert_eq!(
            rows,
            [
                "\u{25cf} 2 tool calls \u{b7} 2 commands",
                "\u{251c} Ran echo hi",
                "\u{2514} Running echo again",
                "",
                "[you] use capitals",
            ]
        );
        // Closing the group takes the trailer with the block.
        assert!(group.unsettled().redraw(DARK, &[]).is_some());
    }

    /// Upstream's two grayscale palettes, as a theme report moves between them.
    const LIGHT: Palette = Palette {
        mode: Mode::Light,
        depth: Depth::Ansi256,
    };

    /// The rows `group` is drawn as in `palette`.
    fn drawn_in(group: &mut ToolGroup, palette: Palette) -> Vec<String> {
        group.drawn.clear();
        group.redraw(palette, &[]).expect("rows")
    }

    /// `rows` placed from row 1 down on a 120-column grid -- each recorded as
    /// what the group says it is ([`ToolGroup::kinds`]), as the frame records
    /// an append's rows -- and a turn's summary (the dim grey, 245 on dark like
    /// the group's own) under them.
    fn on_a_grid(
        group: &ToolGroup,
        rows: &[String],
        palette: Palette,
    ) -> (super::super::grid::Grid, u16) {
        let geometry = crate::tui::layout::solve(40, 120, 1).expect("a band");
        let mut grid = super::super::grid::Grid::blank(geometry.rows, geometry.cols);
        let kinds = group.kinds();
        let mut line = 1u16;
        for (index, row) in rows.iter().enumerate() {
            grid.place_row(line, row, &geometry);
            grid.set_kind(line, kinds.get(index).copied().unwrap_or_default());
            line += 1;
        }
        grid.place_row(
            line,
            &format!("{}  5s (\u{2191}8 \u{2193}240){RESET}", palette.dim()),
            &geometry,
        );
        (grid, line)
    }

    fn same_cells(a: &super::super::grid::Grid, b: &super::super::grid::Grid, last: u16) {
        for line in 1..=last {
            for column in 1..=a.cols() {
                assert!(
                    a.cell(line, column) == b.cell(line, column),
                    "row {line}, column {column}: {:?} against {:?}",
                    a.cell(line, column),
                    b.cell(line, column)
                );
            }
        }
    }

    #[test]
    fn the_groups_kinds_name_each_row_it_draws() {
        // One kind per row of the block, in its order: the header and the
        // call rows are the group's, the blank rows plain, and a cancelled
        // call's own line a stopped one.
        let mut group = ToolGroup::default();
        group.start("1".into(), command("sleep 30"));
        group.start("2".into(), command("sleep 31"));
        group.cancel_running();
        let rows = drawn_in(&mut group, DARK);
        let kinds = group.kinds();
        assert_eq!(rows.len(), kinds.len(), "{rows:?}");
        assert_eq!(
            kinds,
            vec![
                DocumentRow::ToolGroup,
                DocumentRow::ToolGroup,
                DocumentRow::ToolGroup,
                DocumentRow::Plain,
                DocumentRow::ToolStopped,
                DocumentRow::Plain,
                DocumentRow::ToolStopped,
                DocumentRow::Plain,
            ]
        );
        assert!(rows[4].contains('\u{25a0}'), "{rows:?}");
        assert!(ToolGroup::default().kinds().is_empty());
    }

    #[test]
    fn a_theme_report_repaints_a_group_as_the_other_palette_would_draw_it() {
        // The 245 of a group's rows is upstream's statusline grey and goes to
        // 241 on a light screen; the summary under it is the dim grey, also
        // 245 on dark, and goes to 247. A command's words go 252 -> 238, the
        // mark 255 -> 235, and a cancelled call's own line keeps its argument
        // in 245 on either screen (`tool_stdout_style`). Each cell is compared
        // with what drawing the same group in the light palette leaves.
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "a.txt"})),
        );
        ok(&mut group, "1");
        group.start("2".into(), command("echo boom; echo err >&2; exit 3"));
        assert!(group.finish("2", true, "", Outcome::Exited(3), None));
        group.start("3".into(), command("sleep 30"));
        group.cancel_running();

        let dark_rows = drawn_in(&mut group, DARK);
        let (mut repainted, last) = on_a_grid(&group, &dark_rows, DARK);
        let moved = repainted.retint_document(last, &LIGHT);
        assert!(moved > 0, "nothing was repainted");
        let light_rows = drawn_in(&mut group, LIGHT);
        let (fresh, _) = on_a_grid(&group, &light_rows, LIGHT);
        same_cells(&repainted, &fresh, last);

        // And back: a light screen reported dark again is the dark drawing.
        // (Without a quoted string or a number in the command: on light those
        // share 241 with the statusline, and the statusline wins.)
        let mut plain_group = ToolGroup::default();
        plain_group.start("1".into(), command("sleep 30"));
        plain_group.cancel_running();
        let light_rows = drawn_in(&mut plain_group, LIGHT);
        let (mut back, last) = on_a_grid(&plain_group, &light_rows, LIGHT);
        assert!(back.retint_document(last, &DARK) > 0);
        let dark_rows = drawn_in(&mut plain_group, DARK);
        let (dark, _) = on_a_grid(&plain_group, &dark_rows, DARK);
        same_cells(&back, &dark, last);
    }

    #[test]
    fn a_cancelled_calls_argument_is_the_tool_grey_on_either_screen() {
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(
                &EDIT_FILE,
                json!({"path": "a.txt", "old_string": "a", "new_string": "b"}),
            ),
        );
        group.cancel_running();
        let tool = "\u{1b}[38;5;245mfile";
        assert!(drawn_in(&mut group, DARK)[3].contains(tool));
        assert!(drawn_in(&mut group, LIGHT)[3].contains(tool));
    }

    /// A call of an MCP server's own tool, as the runtime shows it.
    fn mcp_call(name: &str) -> Shown {
        mcp::shown(name)
    }

    fn search(query: &str) -> Shown {
        shown(&mcp::CAPABILITY_SEARCH, json!({"query": query}))
    }

    #[test]
    fn one_mcp_call_is_counted_as_an_mcp_call() {
        // fx v0.0.16 `fx-p5-mcp-direct-40x120-turn` rows 5-6 (0.0.13: `1
        // command`).
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_fixture_echo"));
        ok(&mut group, "1");
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 1 tool call \u{b7} 1 MCP call",
                "\u{2514} Ran MCP mcp_fixture_echo",
                "",
            ]
        );
    }

    #[test]
    fn several_mcp_calls_are_mcp_calls_and_their_rows_stay_in_the_rows_grey() {
        // fx v0.0.16 `fx-p5-mcp-turn-40x120-turn` rows 5-8, colours included:
        // `MCP` is no shell word now that the call is not a command's (0.0.13:
        // `2 commands`, and `MCP` in 252).
        let mut group = ToolGroup::default();
        group.start("1".into(), search("echo text through the fixture"));
        ok(&mut group, "1");
        group.start("2".into(), mcp_call("mcp_fixture_echo"));
        ok(&mut group, "2");
        group.start("3".into(), mcp_call("mcp_fixture_echo"));
        ok(&mut group, "3");
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 3 tool calls \u{b7} 2 MCP calls \u{b7} 1 read",
                "\u{251c} Searched capabilities echo text through the fixture",
                "\u{251c} Ran MCP mcp_fixture_echo",
                "\u{2514} Ran MCP mcp_fixture_echo",
                "",
            ]
        );
        let rows = group.rows(DARK);
        assert_eq!(
            rows[0],
            format!(
                "{HINT}\u{25cf}{RESET} {TEXT}3 tool calls \u{b7} 2 MCP calls \u{b7} 1 read{RESET}"
            )
        );
        assert_eq!(
            rows[2],
            format!("{TEXT}\u{251c} Ran MCP mcp_fixture_echo{RESET}")
        );
        assert_eq!(
            rows[3],
            format!("{TEXT}\u{2514} Ran MCP mcp_fixture_echo{RESET}")
        );
    }

    #[test]
    fn a_running_mcp_call_is_counted_and_an_mcp_call_comes_last_in_a_tie() {
        // fx v0.0.16 `fx-p5-mcp-live-40x120-live`: the call still held by its
        // server is counted already, and `1 read` leads `1 MCP call`.
        let mut group = ToolGroup::default();
        group.start("1".into(), search("echo text through the web server"));
        ok(&mut group, "1");
        group.start("2".into(), mcp_call("mcp_web_echo"));
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 2 tool calls \u{b7} 1 read \u{b7} 1 MCP call",
                "\u{251c} Searched capabilities echo text through the web server",
                "\u{2514} Running MCP mcp_web_echo",
                "",
            ]
        );
        // After `command` too (`tool_group_projection.zig:143-156@481104fd`,
        // index 8 against 5); a shell command keeps its own count and words.
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_web_echo"));
        ok(&mut group, "1");
        group.start("2".into(), command("ls"));
        assert!(group.finish("2", true, "", Outcome::Exited(0), None));
        assert_eq!(
            plain(&group)[0],
            "\u{25cf} 2 tool calls \u{b7} 1 command \u{b7} 1 MCP call"
        );
        assert!(group.rows(DARK)[2].contains(&format!("{WORD}ls{FG_OFF}")));
    }

    #[test]
    fn a_failed_stopped_or_refused_mcp_call_is_named_by_its_tool_alone() {
        // fx v0.0.16 `fx-p5-mcp-live-40x120-cancel` rows 15-19: the held call
        // stopped with Ctrl-C. (The line's mark is the transcript's own.)
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_web_echo"));
        // What a stop does to it is unchanged: it settles as a command does.
        assert!(group.running_command());
        assert_eq!(group.cancel_running(), ["1".to_string()]);
        assert_eq!(
            plain(&group),
            [
                "\u{25cf} 1 tool call \u{b7} 1 MCP call \u{b7} 1 cancelled",
                "\u{2514} Cancelled mcp_web_echo",
                "",
                "\u{25a0} Cancelled mcp_web_echo \u{b7} What can xfx do differently?",
                "",
            ]
        );
        // The tool's name in the tool grey, no word of it lifted.
        let line = &group.rows(DARK)[3];
        assert!(
            line.contains(&format!("{RESET} {TEXT}mcp_web_echo{RESET}")),
            "{line:?}"
        );
        // Refused, the same (`formatMissingSpecToolAction`,
        // `app_agent_runtime.zig:1324-1329@481104fd`).
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_web_echo"));
        assert!(group.finish("1", false, "no", Outcome::Denied, None));
        assert_eq!(plain(&group)[1], "\u{2514} Denied mcp_web_echo");
        // Failed, with its reason after it: `fx-p5-mcp-live-40x120-failed`
        // rows 23-25, the server answering with a JSON-RPC error. (The reason
        // is the tool's result, what the model is told.)
        let mut group = ToolGroup::default();
        group.start("1".into(), search("echo text through the web server"));
        ok(&mut group, "1");
        group.start("2".into(), mcp_call("mcp_web_echo"));
        let reason = "MCP protocol error -32603: fixture refuses";
        assert!(group.finish("2", false, reason, Outcome::Finished, None));
        assert_eq!(
            plain(&group)[..3],
            [
                "\u{25cf} 2 tool calls \u{b7} 1 read \u{b7} 1 MCP call \u{b7} 1 failed",
                "\u{251c} Searched capabilities echo text through the web server",
                "\u{2514} Failed mcp_web_echo: MCP protocol error -32603: fixture refuses",
            ]
        );
        assert_eq!(
            group.rows(DARK)[2],
            format!(
                "{TEXT}\u{2514} Failed mcp_web_echo: MCP protocol error -32603: fixture refuses{RESET}"
            )
        );
    }

    #[test]
    fn a_servers_words_in_a_failed_mcp_calls_row_are_written_out_not_obeyed() {
        // A reason that is a server's text: its line breaks one space, an
        // escape and a bidirectional override spelled out, as upstream's
        // `encodeTerminalSafeInline` does, where every other tool's reason
        // only has its controls blanked.
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_web_echo"));
        let reason = "refused\r\n  by \u{1b}[31mthe\u{202e} server";
        assert!(group.finish("1", false, reason, Outcome::Finished, None));
        assert_eq!(
            plain(&group)[1],
            "\u{2514} Failed mcp_web_echo: refused by \\x1b[31mthe\\u{202e} server"
        );
        assert!(!group.rows(DARK)[1].contains('\u{202e}'));
    }
}
