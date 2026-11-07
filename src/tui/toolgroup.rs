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
use crate::tools::{Activity, LineChanges, Outcome, Shown};

use super::theme::{DocumentRow, Mode, Palette};

/// What a cancelled call's own line ends with, after its mark and its row
/// (`vercel-labs/fx@c1db9191 src/ui/transcript/runtime.zig:644`), in this
/// product's name.
const CANCELLED_FOLLOW_UP: &str = " \u{b7} What can xfx do differently?";

/// How much of a command fx reads to make its row
/// (`max_run_command_activity_source_bytes`, `vercel-labs/fx@481104fd
/// src/core/tooling/tool_presentation.zig:22`).
const COMMAND_SOURCE_BYTES: usize = 120 * 120;

/// How many bytes of a command's row a call keeps, spelled: the bound fx
/// stores a command's row at so the transcript can cut it at the live width
/// (`max_run_command_reflow_bytes`, `:23`, less the byte
/// `formatRunCommandDetailBounded` keeps for its own check, `:441`).
///
/// Not the row's width: the row is cut at the screen's, with an ellipsis, when
/// it is written (`super::transcript`'s `ellipsized`), and Ctrl-O wraps it. The
/// bound keeps a four-megabyte command from being copied into every rebuild of
/// the block.
const COMMAND_TARGET_BYTES: usize = COMMAND_SOURCE_BYTES - 1;

/// How many bytes of any other call's argument a row keeps, spelled: fx's
/// bound for a file change's path (`diff.max_encoded_label_bytes`,
/// `src/core/output/diff.zig:10@481104fd`, `fileMutationDisplayPath`).
const TARGET_BYTES: usize = 4 * 1024;

/// How many bytes of a failure's reason a row keeps, spelled
/// (`failureStatusDetail`, `src/core/agent/runtime/tool_presentation.zig:
/// 1445-1493@481104fd`).
const REASON_BYTES: usize = 256;

/// What a row's argument or reason ends with when its bound cut it
/// (`encodeTerminalSafe`'s marker, `text_utils.zig:358@481104fd`).
const CUT: &str = "...";

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

    /// The argument the row names, spelled on one line ([`spelled`]).
    ///
    /// A file change that was refused or stopped is named by the tool's own
    /// word (`Denied file`, `Cancelled file`), as upstream's is: its row names
    /// the file it was given only once it has been given one
    /// (`tool_presentation.zig` `formatPlainAction`, `display_target`).
    fn target(&self) -> String {
        let default = self.shown.presentation.default_target;
        let refused = matches!(self.state, State::Denied | State::Cancelled);
        match &self.shown.target {
            Some(target) if !(refused && self.changes_a_file()) => {
                spelled(target, self.shown.presentation.activity)
            }
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
            State::Failed(reason) => {
                let reason = flattened(reason);
                if !reason.is_empty() {
                    phrase.push_str(": ");
                    phrase.push_str(&reason);
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
        if self.shown.presentation.activity != Activity::Command {
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
/// (`formatGroupHeader`, `:497-541`).
fn summary(calls: &[&Call]) -> String {
    let total = calls.len();
    let mut out = format!("{total} tool call{}", if total == 1 { "" } else { "s" });
    let mut counted: Vec<(usize, usize, &str)> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(order, (activity, label))| {
            let count = calls
                .iter()
                .filter(|call| call.shown.presentation.activity == *activity)
                .count();
            (count, order, *label)
        })
        .filter(|(count, _, _)| *count > 0)
        .collect();
    // By count, most first; a tie keeps the categories' own order.
    counted.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (count, _, label) in counted {
        let plural = if label == "command" && count != 1 {
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
    let target = if call.shown.presentation.activity == Activity::Command {
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

/// A call's argument as its row shows it -- the transcript's row and Ctrl-O's
/// heading -- by fx v0.0.16's rule for that row:
///
/// * a command is put on one line as fx puts it
///   (`projectRunCommandActivitySource`, `vercel-labs/fx@481104fd
///   src/core/tooling/tool_presentation.zig:256-311`): of its first
///   [`COMMAND_SOURCE_BYTES`], a carriage return or a line feed ends a line,
///   the spaces and tabs that open the next go with it, and one space stands
///   for the break once something came before it and something follows;
///   then [`Spelling`] writes it at [`COMMAND_TARGET_BYTES`]
///   (`formatRunCommandDetailBounded`, `:432-447`, the row's text at
///   `src/core/app/app_callbacks.zig:544-549`);
/// * any other argument is [`Spelling`]'s as it is, at [`TARGET_BYTES`],
///   as fx spells a file change's path (`fileMutationDisplayPath`,
///   `src/core/agent/runtime/tool_presentation.zig:736-753`). fx writes a
///   read's, a list's or a pattern's row with the argument as it came -- the
///   marks and the C1 controls to the terminal as themselves, the other
///   controls dropped, the row ended at its first line break
///   (`formatToolAction`, `src/core/app/app_agent_runtime.zig:1248-1310`,
///   and `formatToolActionValue`, `:1340-1342`; `normalizeCanonicalStatus`,
///   `src/ui/transcript/tool_group_projection.zig:317-347`) -- which is not
///   safe to copy (ledger `call-row-raw-argument`).
///
/// Spaces around an argument stay, upstream's `Failed   missing.txt  :`
/// (`fx-read-failures-40x120-live`). Not fx's: a command's `cd . &&` and its
/// workspace root written `.` (`stripNoopCurrentDirectoryPrefix`,
/// `workspaceRootMatchesAt`), which this row never had.
///
/// Idempotent: what it writes holds no line break, nothing it would spell and
/// nothing past its bound -- so the channel spells the argument as it crosses
/// (`super::bridge`'s `made_inert`) and the row spells it again unchanged.
pub(crate) fn spelled(target: &str, activity: Activity) -> String {
    if activity != Activity::Command {
        let mut spelling = Spelling::new(TARGET_BYTES);
        for character in target.chars() {
            if !spelling.push(character) {
                break;
            }
        }
        return spelling.finish();
    }
    let mut spelling = Spelling::new(COMMAND_TARGET_BYTES);
    let mut line_break = false;
    let mut written = false;
    for (at, character) in target.char_indices() {
        if at >= COMMAND_SOURCE_BYTES {
            break;
        }
        match character {
            '\r' | '\n' => {
                line_break = true;
                continue;
            }
            ' ' | '\t' if line_break => continue,
            _ => {}
        }
        if std::mem::take(&mut line_break) && written && !spelling.token(" ") {
            break;
        }
        let kept = if at + character.len_utf8() > COMMAND_SOURCE_BYTES {
            // fx reads the command's first bytes, so a character the bound
            // splits is read in part; no part is a character, and each of its
            // bytes is `\xNN` (`terminalSafeToken`, `text_utils.zig:652-657`).
            let mut bytes = [0u8; 4];
            character.encode_utf8(&mut bytes);
            bytes[..COMMAND_SOURCE_BYTES - at]
                .iter()
                .all(|byte| spelling.token(&format!("\\x{byte:02x}")))
        } else {
            spelling.push(character)
        };
        if !kept {
            break;
        }
        written = true;
    }
    spelling.finish()
}

/// A failure's reason as the row shows it, as fx's `encodeTerminalSafeInline`
/// writes it (`src/core/shared/text_utils.zig:397-417@481104fd`, for the row
/// at `failureStatusDetail`, `src/core/agent/runtime/tool_presentation.zig:
/// 1445-1493`): every run of ASCII white space -- a space, `\t` to `\r` --
/// one space and none at either end, then [`Spelling`]'s at [`REASON_BYTES`].
/// A reason can quote the argument, so it is spelled as the argument is.
///
/// Idempotent, as [`spelled`] is: the channel spells a reason as it crosses
/// (`super::bridge`'s `made_inert`) and the row spells it again unchanged.
pub(crate) fn flattened(reason: &str) -> String {
    let mut spelling = Spelling::new(REASON_BYTES);
    let mut space = false;
    for character in reason.chars() {
        if character == ' ' || ('\t'..='\r').contains(&character) {
            space = !spelling.out.is_empty();
            continue;
        }
        if std::mem::take(&mut space) && !spelling.token(" ") {
            break;
        }
        if !spelling.push(character) {
            break;
        }
    }
    spelling.finish()
}

/// Text as fx's `encodeTerminalSafe` writes it, bounded as it bounds it
/// (`src/core/shared/text_utils.zig:353-390@481104fd`).
///
/// Each character is its `terminalSafeToken` (`:643-667`) -- a C0 control or
/// `DEL` `\xNN`, a C1 control or a mark `isNonPrintingCodepoint` names
/// (`:703-709`) `\u{NNNN}`, anything else itself -- which is
/// [`crate::tools::question::terminal_safe`]; and the Arabic letter mark
/// (`U+061C`), which fx writes as itself, is `\u{061c}` too: it reorders the
/// row around it as the marks fx names do (`approval_view`'s `reorders`, the
/// full transcript's `detail_spelled`).
///
/// The bound is on what is written, never on what is read: once a token would
/// take the text past `max` bytes, it is cut back to the last whole token that
/// leaves room for [`CUT`], so no cut falls inside an escape.
struct Spelling {
    out: String,
    max: usize,
    /// The length of the text up to the last whole token that leaves room
    /// for the marker.
    kept: usize,
    cut: bool,
}

impl Spelling {
    fn new(max: usize) -> Self {
        Self {
            out: String::new(),
            max,
            kept: 0,
            cut: false,
        }
    }

    /// Writes `character`'s token, or answers `false` once it does not fit.
    fn push(&mut self, character: char) -> bool {
        if character == '\u{061c}' {
            return self.token("\\u{061c}");
        }
        let mut bytes = [0u8; 4];
        let token = crate::tools::question::terminal_safe(character.encode_utf8(&mut bytes));
        self.token(&token)
    }

    /// Writes one token whole, or answers `false` once it does not fit.
    fn token(&mut self, token: &str) -> bool {
        if self.out.len() + token.len() > self.max {
            self.cut = true;
            return false;
        }
        self.out.push_str(token);
        if self.out.len() + CUT.len().min(self.max) <= self.max {
            self.kept = self.out.len();
        }
        true
    }

    fn finish(mut self) -> String {
        if self.cut {
            self.out.truncate(self.kept);
            self.out.push_str(&CUT[..CUT.len().min(self.max)]);
        }
        self.out
    }
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

    // -- CALLROW-SAFE: a call's own row over what a terminal would act on ----

    /// `p5-call-row-safe`'s step: a command, a glob pattern and a read path,
    /// each carrying a right-to-left override, a left-to-right isolate, the
    /// Arabic letter mark, `ESC[2J`, a bare carriage return, a tab and the C1
    /// `U+009B`. The read fails, and its reason quotes the path.
    const UNSAFE_COMMAND: &str =
        "printf '%s' 'A\u{202e}B\u{2066}C\u{061c}D\u{1b}[2JE\rF\tG\u{9b}H' >/dev/null";
    const UNSAFE_PATTERN: &str = "n\u{202e}o\u{2066}p\u{061c}q\u{1b}7r\ts\u{9b}t\ru*.md";
    const UNSAFE_PATH: &str = "m\u{202e}n\u{2066}o\u{061c}p\u{1b}[2Jq\tr\u{9b}s\rt.txt";

    fn unsafe_calls() -> ToolGroup {
        let mut group = ToolGroup::default();
        group.start("c".into(), command(UNSAFE_COMMAND));
        group.start(
            "g".into(),
            shown(&read::GLOB_FILES, json!({ "pattern": UNSAFE_PATTERN })),
        );
        group.start(
            "r".into(),
            shown(&read::READ_FILE, json!({ "path": UNSAFE_PATH })),
        );
        assert!(group.finish("c", true, "", Outcome::Exited(0), None));
        ok(&mut group, "g");
        let reason = format!("Path not found: {UNSAFE_PATH}");
        assert!(group.finish("r", false, &reason, Outcome::Finished, None));
        group
    }

    /// A character no row may hand the terminal as itself: a control other
    /// than the escape (which [`only_colours`] reads), or a mark fx names
    /// (`isNonPrintingCodepoint`, `text_utils.zig:703-709@481104fd`) or xfx
    /// does (`U+061C`).
    fn obeyed_or_hidden(character: char) -> bool {
        let code = u32::from(character);
        (code < 0x20 && character != '\u{1b}')
            || code == 0x7f
            || (0x80..=0x9f).contains(&code)
            || (0x200b..=0x200f).contains(&code)
            || (0x2028..=0x202e).contains(&code)
            || (0x2060..=0x206f).contains(&code)
            || code == 0xfeff
            || code == 0x061c
    }

    /// Whether every escape in `row` is a colour (`ESC[…m`), the only ones
    /// a row writes.
    fn only_colours(row: &str) -> bool {
        let mut rest = row;
        while let Some(at) = rest.find('\u{1b}') {
            rest = &rest[at..];
            match super::super::pacer::escape_at(rest) {
                Some(len) if rest[..len].starts_with("\u{1b}[") && rest[..len].ends_with('m') => {
                    rest = &rest[len..];
                }
                _ => return false,
            }
        }
        true
    }

    #[test]
    fn a_calls_own_row_spells_its_argument_as_fx_spells_a_commands() {
        // `p5-call-row-safe` (fx v0.0.16, `-end` and `-full`): the command
        // through fx's projection -- the bare carriage return one space --
        // and `encodeTerminalSafe` (`tool_presentation.zig:432-447@481104fd`,
        // `text_utils.zig:353-390`): `ESC` and the tab `\xNN`, the marks
        // and the C1 `\u{NNNN}`. The pattern and the path the same way
        // without the projection, as fx spells a file change's
        // (`fileMutationDisplayPath`, `src/core/agent/runtime/
        // tool_presentation.zig:736-753@481104fd`); fx hands those two to
        // the terminal as they are (ledger `call-row-raw-argument`). The
        // Arabic letter mark is named on every row. The reason as
        // `encodeTerminalSafeInline` spells it: white space one space, the
        // rest the same encoding.
        let group = unsafe_calls();
        let expected = [
            r"├ Ran printf '%s' 'A\u{202e}B\u{2066}C\u{061c}D\x1b[2JE F\x09G\u{009b}H' >/dev/null",
            r"├ Matched n\u{202e}o\u{2066}p\u{061c}q\x1b7r\x09s\u{009b}t\x0du*.md",
            r"└ Failed m\u{202e}n\u{2066}o\u{061c}p\x1b[2Jq\x09r\u{009b}s\x0dt.txt: Path not found: m\u{202e}n\u{2066}o\u{061c}p\x1b[2Jq r\u{009b}s t.txt",
        ];
        assert_eq!(plain(&group)[1..4], expected);
        let expanded: Vec<String> = group.expanded(DARK).iter().map(|row| strip(row)).collect();
        assert_eq!(expanded[1..4], expected);
    }

    #[test]
    fn no_calls_row_hands_the_terminal_a_control_or_a_mark_at_any_width() {
        let group = unsafe_calls();
        for cols in [10u16, 24, 40, 80, 120] {
            for row in group.rows(DARK).iter().chain(&group.expanded(DARK)) {
                let cut = super::super::transcript::ellipsized(row, cols);
                assert!(super::super::wrap::width(&cut) <= cols, "{cols}: {cut:?}");
                assert!(only_colours(&cut), "{cols}: {cut:?}");
                assert!(
                    !strip(&cut).chars().any(obeyed_or_hidden),
                    "{cols}: {cut:?}"
                );
            }
        }
    }

    #[test]
    fn a_commands_line_breaks_are_one_space_as_fxs_projection_makes_them() {
        // `projectRunCommandActivitySource` (`tool_presentation.zig:256-311
        // @481104fd`): a carriage return or a line feed ends a line, the
        // spaces and tabs that open the next one go with it, and one space
        // stands for the break -- once something came before it and only
        // when something follows. Another call's argument has its breaks
        // spelled.
        for (command_text, row) in [
            (
                "echo a\n  echo b\r\n\techo c\n",
                "Running echo a echo b echo c",
            ),
            ("\n\t ls", "Running ls"),
            ("  \nls", "Running    ls"),
            ("a\tb\r", r"Running a\x09b"),
        ] {
            let mut group = ToolGroup::default();
            group.start("1".into(), command(command_text));
            assert_eq!(
                plain(&group)[1],
                format!("\u{2514} {row}"),
                "{command_text:?}"
            );
        }
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": "a\nb\rc"})),
        );
        assert_eq!(plain(&group)[1], r"└ Reading a\x0ab\x0dc");
    }

    #[test]
    fn a_long_argument_is_bounded_after_it_is_spelled_and_never_inside_an_escape() {
        // `encodeTerminalSafe` bounds what it writes, not what it reads, and
        // cuts back to the last whole token that leaves room for `...`
        // (`text_utils.zig:353-390@481104fd`): a command at the transcript's
        // reflow bound, 14 399 bytes (`max_run_command_reflow_bytes`,
        // `tool_presentation.zig:21-23`, `formatRunCommandDetailBounded`
        // `:441`), a file change's path at 4 096 (`diff.max_encoded_label_bytes`,
        // `src/core/output/diff.zig:10`), and so every other argument here.
        let pair = "A\u{202e}";
        let spelled_pair = r"A\u{202e}";
        let mut group = ToolGroup::default();
        group.start("1".into(), command(&pair.repeat(5_000)));
        let row = &plain(&group)[1];
        let expected = format!("\u{2514} Running {}A...", spelled_pair.repeat(1_599));
        assert_eq!(row.len(), expected.len());
        assert!(*row == expected, "{}", &row[row.len() - 40..]);
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": pair.repeat(5_000)})),
        );
        let row = &plain(&group)[1];
        let expected = format!("\u{2514} Reading {}A...", spelled_pair.repeat(454));
        assert!(*row == expected, "{}", &row[row.len() - 40..]);
    }
}
