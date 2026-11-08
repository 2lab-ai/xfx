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
//! **A theme switch draws the rows again**, open block or closed: each header
//! and call row is handed to the transcript with how it is drawn in either
//! mode ([`ToolGroup::redraws`]), and a report puts that drawing back on the
//! row rather than recolouring it, as upstream projects the group again in
//! the new theme (`vercel-labs/fx@481104fd tool_group_projection.zig:558-640`).
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
use crate::tools::{mcp, mutate, Activity, LineChanges, Outcome, Presentation, Shown};

use super::theme::{DocumentRow, Mode, Palette, Redraw, CANCELLED_MARK};

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

/// How many bytes of a command fx's status phrase keeps, spelled: the phrase
/// the call's row is made from before the transcript puts the stored command
/// in its place (`max_run_command_activity_bytes`, `tool_presentation.zig:21`;
/// `formatRunCommandActivity`, `:393-416`), and the one Ctrl-O looks in for an
/// argument the heading already shows ([`status`]).
const COMMAND_STATUS_BYTES: usize = 120;

/// How many bytes of a file change's path a row keeps, spelled: fx's bound
/// for it (`diff.max_encoded_label_bytes`, `src/core/output/diff.zig:10@481104fd`,
/// `fileMutationDisplayPath`), for `write_file` and `edit_file` alone
/// (`file_mutation_contract.isToolName`,
/// `src/core/tooling/file_mutation_contract.zig:487-490`).
const TARGET_BYTES: usize = 4 * 1024;

/// How many bytes of a failure's reason a row keeps, spelled
/// (`failureStatusDetail`, `src/core/agent/runtime/tool_presentation.zig:
/// 1445-1493@481104fd`).
const REASON_BYTES: usize = 256;

/// What a row's argument or reason ends with when its bound cut it
/// (`encodeTerminalSafe`'s marker, `text_utils.zig:358@481104fd`).
const CUT: &str = "...";

/// How many columns of a call's argument the transcript's own row keeps: one
/// more than the widest screen there is (a screen's width is a `u16`), so the
/// row is still wider than any screen and is cut there with its ellipsis,
/// cell for cell what the whole argument would show (`super::transcript`'s
/// `ellipsized`). Ctrl-O's heading keeps the whole argument, as fx's expanded
/// row is never clipped (`formatExpandedChild`,
/// `tool_group_projection.zig:786-787@481104fd`).
///
/// A read's, a list's or a pattern's argument is the model's at any length,
/// up to the stream's 8 MiB (`crate::gateway::sse::MAX_COMPLETION_BYTES`) and
/// four times that spelled, and the group's block is drawn again whole, in
/// both modes, on every change: kept whole, a redraw took 0.46 s for 8 MiB of
/// `a` and 1.8 s for 4 Mi U+009B, the session 1.25 GiB at its peak
/// (`.prd/tui-phase5/receipts/cancel-marker/round2/`).
///
/// Columns and nothing else: a bound on bytes cut text a screen shows -- a
/// letter under 196 608 combining marks lost the `tail` after it (round 3).
/// A cluster that holds many bytes is kept whole, as the screen keeps it
/// (`super::grid::tokenize_row` joins every mark to its cell), and what it
/// costs a redraw is paid once ([`Call::inline`], and the transcript's own
/// cache of the rows it writes).
const INLINE_CELLS: usize = u16::MAX as usize + 1;

/// `target`, spelled as [`spelled`] spells an argument that is not a command,
/// as the transcript's own row keeps it: whole when it is no wider than
/// [`INLINE_CELLS`], and otherwise up to the last whole unit that fits, ended
/// [`CUT`].
///
/// A unit is one of `target`'s grapheme clusters, or a spelling it already
/// holds (`\xNN`, `\u{N}`, [`spelling_at`]) with whatever joins that
/// spelling's last character -- the channel's argument is spelled
/// (`super::bridge`'s `spelled_shown`), and spelling it again changes nothing
/// -- so the cut never falls inside a cluster or a spelling.
///
/// **`target` is the channel's, spelled**, wherever a session gives one
/// (`super::bridge`'s `made_inert` spells every call's `Shown`, a turn's and
/// a resumed session's): its units are then the clusters of the spelled
/// text, and a raw cluster the channel spelled in pieces -- a ZWJ family,
/// its joiners written out -- may be cut between them. Raw text, which only
/// a test hands it, is spelled here unit by unit and cut between its raw
/// clusters, but a raw cluster that spells wider than `u16::MAX` cells is
/// measured at that (`super::wrap::width` saturates) and kept whole. No
/// assertion stands for this: the raw path is what the row's own spelling
/// tests drive.
fn inline(target: &str) -> String {
    let mut out = String::new();
    let mut cells = 0usize;
    let mut rest = target;
    while !rest.is_empty() {
        let unit = &rest[..unit_at(rest)];
        let unit_spelled = spelled_whole(unit);
        cells += usize::from(super::wrap::width(&unit_spelled));
        if cells > INLINE_CELLS {
            out.push_str(CUT);
            return out;
        }
        out.push_str(&unit_spelled);
        rest = &rest[unit.len()..];
    }
    out
}

/// How many bytes of `rest` its first unit takes ([`inline`]): a spelling and
/// what joins its last character, or one grapheme cluster -- ended before a
/// spelling it took in.
///
/// A character UAX #29 calls Prepend (GB9b: U+0600-0605, U+06DD, U+070F,
/// ...) joins the character after it, a spelling's `\` too, and no spelling
/// names one: so a cluster that took in a `\` starting a spelling ends
/// before it, and the spelling is the next unit.
fn unit_at(rest: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let cluster = |text: &str| text.graphemes(true).next().map_or(text.len(), str::len);
    match spelling_at(rest) {
        // Its last character is `}` or a hex digit, and a mark after it joins
        // it, so the cluster that character starts is the spelling's end.
        Some(len) => len - 1 + cluster(&rest[len - 1..]),
        None => {
            let whole = cluster(rest);
            rest[..whole]
                .char_indices()
                .skip(1)
                .find(|&(at, character)| character == '\\' && spelling_at(&rest[at..]).is_some())
                .map_or(whole, |(at, _)| at)
        }
    }
}

/// The length of the spelling `rest` starts with, if it starts with one:
/// `\xNN` or `\u{N}`, one to six hex digits, as [`Spelling`] writes them
/// (`crate::tools::question::terminal_safe`). Text the model wrote that reads
/// the same is taken for one too, which only keeps a cut from falling inside
/// it.
fn spelling_at(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'\\') {
        return None;
    }
    match bytes.get(1) {
        Some(b'x')
            if bytes
                .get(2..4)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) =>
        {
            Some(4)
        }
        Some(b'u') if bytes.get(2) == Some(&b'{') => {
            let digits = bytes[3..]
                .iter()
                .take(7)
                .take_while(|byte| byte.is_ascii_hexdigit())
                .count();
            ((1..=6).contains(&digits) && bytes.get(3 + digits) == Some(&b'}'))
                .then_some(4 + digits)
        }
        _ => None,
    }
}

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
/// more than one ([`category_word`]). Until fx v0.0.16 such a call was
/// counted as a command (`fx-p5-mcp-turn-40x120-turn`: `2 commands` on 0.0.13,
/// `2 MCP calls` on 0.0.16).
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
    /// The argument as the transcript's own row shows it ([`inline`]), made
    /// the first time a row needs it and kept: a call's argument never
    /// changes, and the block is drawn again in both modes on every change to
    /// the group. Ctrl-O's heading reads [`Self::shown`].
    inline: std::cell::OnceCell<String>,
    state: State,
    /// The command as the highlighter last coloured it, per mode and run
    /// colour ([`Call::highlighted`]).
    highlights: std::cell::RefCell<Vec<Highlight>>,
}

/// One colouring of a call's command: the command, the mode, the colour
/// between its words, and what the highlighter made of them.
#[derive(Debug)]
struct Highlight {
    command: String,
    light: bool,
    base: String,
    painted: String,
}

/// How many colourings a call keeps: each mode, under the row's grey and
/// under a stopped call's own line's, and one over.
const HIGHLIGHTS_KEPT: usize = 5;

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

    /// The argument the row names, spelled on one line ([`spelled`]).
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
    ///
    /// `whole`: Ctrl-O's heading, which shows all of it; otherwise the
    /// transcript's row, which keeps what a screen can show ([`inline`]) --
    /// but a command's and a file change's, which have bounds of their own,
    /// under that one.
    fn target(&self, whole: bool) -> String {
        let default = self.shown.presentation.default_target;
        let refused = matches!(self.state, State::Denied | State::Cancelled);
        let ran = matches!(self.state, State::Running | State::Done(_));
        match self.shown.target.as_deref() {
            Some(target) if self.calls_mcp() => {
                let named = if ran {
                    target
                } else {
                    target.strip_prefix("MCP ").unwrap_or(target)
                };
                crate::mcp::terminal_safe(named)
            }
            Some(target) if !(refused && self.changes_a_file()) => {
                let own_bound = self.shown.presentation.activity == Activity::Command
                    || names_a_changed_file(self.shown.presentation);
                if whole || own_bound {
                    spelled(target, self.shown.presentation)
                } else {
                    self.inline.get_or_init(|| inline(target)).clone()
                }
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
    fn phrase(&self, whole: bool) -> String {
        let mut phrase = format!("{} {}", self.label(), self.target(whole));
        match &self.state {
            // A reason with nothing left once it is spelled is no reason, as
            // fx's is none (`failureStatusDetail`, `src/core/agent/runtime/
            // tool_presentation.zig:1490-1493@481104fd`, and `:1283`).
            State::Failed(reason) if self.calls_mcp() => {
                let reason = server_spelled(reason);
                if !reason.is_empty() {
                    phrase.push_str(": ");
                    phrase.push_str(&reason);
                }
            }
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
    fn painted(&self, base: &str, palette: Palette, whole: bool) -> String {
        if !self.runs_a_command() {
            return self.phrase(whole);
        }
        format!("{} {}", self.label(), self.highlighted(base, palette))
    }

    /// [`highlighted`] for this call's command, from the colourings it
    /// already made when one is for the same command, mode and run colour.
    ///
    /// A group's block is drawn again whole on every change, in both modes
    /// ([`ToolGroup::redraws`]), so a group of N calls coloured each command
    /// twice per change -- and a resumed session replays every change of
    /// every group. The highlighter is a function of exactly those three
    /// things, so what it gave once is what it gives again.
    fn highlighted(&self, base: &str, palette: Palette) -> String {
        // A command is held to its own bound, under the row's.
        let command = self.target(true);
        let light = palette.mode == Mode::Light;
        if let Some(kept) = self
            .highlights
            .borrow()
            .iter()
            .find(|kept| kept.light == light && kept.base == base && kept.command == command)
        {
            return kept.painted.clone();
        }
        let painted = highlighted(&command, base, palette);
        let mut kept = self.highlights.borrow_mut();
        if kept.len() == HIGHLIGHTS_KEPT {
            kept.remove(0);
        }
        kept.push(Highlight {
            command,
            light,
            base: base.to_string(),
            painted: painted.clone(),
        });
        painted
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
            inline: std::cell::OnceCell::new(),
            state: State::Running,
            highlights: std::cell::RefCell::default(),
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

    /// How each row of [`Self::rows`] is drawn in each mode, in the same
    /// order, for the rows a theme report draws again
    /// rather than recolours ([`Redraw`]): the header and the call rows --
    /// upstream projects the group again in the theme then in force
    /// (`formatGroupBlock`, `tool_group_projection.zig:558-640@481104fd`, the
    /// command through `highlightCommandPhrase` with `shared_theme.current()`,
    /// `:742-784`). Nothing for the blank rows, and nothing for a cancelled
    /// call's own line: upstream does not project that entry again but draws
    /// its own bytes (`:649-664`), which a switch rewrites through its token
    /// table (`store.zig:3745-3750`), and so does a report here
    /// (`super::theme::Palette::document_retint_in`).
    ///
    /// Beside [`Self::rows`], as [`Self::kinds`] is. `drawn` is what
    /// [`Self::redraw`] just gave in `palette` -- the rows in that mode, so
    /// only the other mode is drawn here -- with whatever it carried under
    /// them.
    pub(crate) fn redraws(&self, palette: Palette, drawn: &[String]) -> Vec<Option<Redraw>> {
        let other = self.rows(Palette {
            mode: match palette.mode {
                Mode::Dark => Mode::Light,
                Mode::Light => Mode::Dark,
            },
        });
        self.kinds()
            .into_iter()
            .zip(drawn.iter().zip(other))
            .map(|(kind, (drawn, other))| {
                (kind == DocumentRow::ToolGroup).then(|| match palette.mode {
                    Mode::Dark => Redraw::new(drawn.clone(), other),
                    Mode::Light => Redraw::new(other, drawn.clone()),
                })
            })
            .collect()
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
        let mut rows = self.header_and_calls(palette, palette.statusline(), false);
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
        self.header_and_calls(palette, "", true)
    }

    /// The header, then each visible call's row with `text` around it: the
    /// whole argument for Ctrl-O, the transcript's own row's otherwise
    /// ([`Call::target`]).
    fn header_and_calls(&self, palette: Palette, text: &str, whole: bool) -> Vec<String> {
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
                call.painted(text, palette, whole)
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
        out.push_str(&format!(" \u{b7} {count} {}", category_word(label, count)));
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

/// The word a kind of work is counted in for `count` calls: upstream's table
/// (`formatGroupSummary`, `vercel-labs/fx@481104fd
/// src/ui/transcript/tool_group_projection.zig:539-544`). A count of 1 takes
/// the label as written; any other takes the label's plural where the table
/// has one -- `commands`, `subagents`, `MCP calls` -- and the label itself
/// where it has none (`2 read`, `3 edit`). `subagent` is upstream's word for a
/// tool xfx does not have (`docs/parity.md`, `subagent`): the table keeps it
/// so the header says what fx's would the day the tool is ported, and no call
/// of xfx's is counted as one.
fn category_word(label: &str, count: usize) -> &str {
    const PLURALS: [(&str, &str); 3] = [
        ("command", "commands"),
        ("subagent", "subagents"),
        (MCP_CALL, "MCP calls"),
    ];
    if count == 1 {
        return label;
    }
    PLURALS
        .iter()
        .find(|(singular, _)| *singular == label)
        .map_or(label, |(_, plural)| plural)
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
        call.highlighted(base, palette)
    } else {
        call.target(false)
    };
    format!(
        "{}{CANCELLED_MARK}{reset}{} Cancelled{reset} {base}{target}{reset}{CANCELLED_FOLLOW_UP}",
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
///   for the break once something came before it and something follows --
///   and where fx says nothing of a command it stopped reading with more left
///   to show, the row ends `...` (ledger `call-row-source-cut`);
///   then [`Spelling`] writes it at [`COMMAND_TARGET_BYTES`]
///   (`formatRunCommandDetailBounded`, `:432-447`, the row's text at
///   `src/core/app/app_callbacks.zig:544-549`);
/// * a file change's path (`write_file`, `edit_file`) is [`Spelling`]'s as
///   it is, at [`TARGET_BYTES`], as fx spells it (`fileMutationDisplayPath`,
///   `src/core/agent/runtime/tool_presentation.zig:736-753`);
/// * any other argument is [`Spelling`]'s as it is, whole. fx writes a
///   read's, a list's, a pattern's or a new folder's row with the argument as
///   it came, at any length -- the marks and the C1 controls to the terminal as
///   themselves, the other controls dropped, the row ended at its first line
///   break (`formatToolAction`, `src/core/app/app_agent_runtime.zig:1248-1310`,
///   `presentationLabelValue`, `src/core/tooling/tool_dispatch.zig:626-647`,
///   and `formatToolActionValue`, `:1340-1342`; `normalizeCanonicalStatus`,
///   `src/ui/transcript/tool_group_projection.zig:317-347`) -- which is not
///   safe to copy (ledger `call-row-raw-argument`); its length is, and Ctrl-O
///   shows the heading whole as fx's does (`formatExpandedChild`, `:786-787`:
///   an expanded row is never clipped).
///
/// Spaces around an argument stay, upstream's `Failed   missing.txt  :`
/// (`fx-read-failures-40x120-live`). Not fx's: a command's `cd . &&` and its
/// workspace root written `.` (`stripNoopCurrentDirectoryPrefix`,
/// `workspaceRootMatchesAt`), which this row never had.
///
/// Idempotent: what it writes holds no line break, nothing it would spell and
/// nothing past its bound -- so the channel spells the argument as it crosses
/// (`super::bridge`'s `made_inert`) and the row spells it again unchanged.
pub(crate) fn spelled(target: &str, presentation: Presentation) -> String {
    if presentation.activity == Activity::Command {
        return command_spelled(target, COMMAND_TARGET_BYTES, true);
    }
    if !names_a_changed_file(presentation) {
        return spelled_whole(target);
    }
    let mut spelling = Spelling::new(TARGET_BYTES);
    for character in target.chars() {
        if !spelling.push(character) {
            break;
        }
    }
    spelling.finish()
}

/// [`Spelling`]'s of `text`, unbounded, in one pass over it rather than a
/// token a character: [`Spelling::push`]'s token is
/// [`crate::tools::question::terminal_safe`]'s for every character but the
/// Arabic letter mark, so without one the whole text's `terminal_safe` is the
/// same text -- and `text` as it is when it holds nothing to spell, which is
/// what the channel hands a row (`super::bridge`'s `spelled_shown`).
fn spelled_whole(text: &str) -> String {
    if !text.contains('\u{061c}') {
        return crate::tools::question::terminal_safe(text).into_owned();
    }
    let mut spelling = Spelling::new(usize::MAX);
    for character in text.chars() {
        spelling.push(character);
    }
    spelling.finish()
}

/// Whether `presentation` is `write_file`'s or `edit_file`'s, whose path fx
/// spells at a file change's bound ([`TARGET_BYTES`]; `file_mutation_contract.
/// isToolName`, `src/core/tooling/file_mutation_contract.zig:487-490@481104fd`).
fn names_a_changed_file(presentation: Presentation) -> bool {
    [mutate::WRITE_FILE, mutate::EDIT_FILE]
        .iter()
        .any(|spec| spec.presentation() == presentation)
}

/// fx's status phrase for a call with this argument as the model wrote it,
/// or `None` when its status is the whole argument -- the text Ctrl-O looks in
/// for an argument the heading already shows (`argumentVisibleInToolHeading`,
/// `src/ui/full_transcript_screen.zig:6594-6619@481104fd`, reads the status
/// entry's own bytes, not the command the heading puts in its place).
///
/// A command's is its first [`COMMAND_STATUS_BYTES`] spelled -- and, as fx's,
/// with no `...` for a command read only in part, which the row's own spelling
/// has (ledger `call-row-source-cut`) -- so a command past them keeps its
/// argument row even where the heading shows it whole. A
/// command call with no command has an empty one, which holds nothing. Any
/// other call's status is its whole argument, which its row shows.
///
/// `shown` must be the **raw** call -- the tool's own decode of the model's
/// arguments, before the channel spells them (`super::bridge`'s
/// `spelled_shown`): a bound applied to the spelled text could fall inside an
/// escape's spelling, where fx's, on the raw command, falls before the whole
/// escape, and a value ending in `\` would be found where fx finds none.
pub(crate) fn status(shown: &Shown) -> Option<String> {
    (shown.presentation.activity == Activity::Command).then(|| {
        shown
            .target
            .as_deref()
            .map(|target| command_spelled(target, COMMAND_STATUS_BYTES, false))
            .unwrap_or_default()
    })
}

/// A command as [`spelled`] spells it, at `max` bytes -- and, with
/// `mark_unread`, ended `...` where it was read only in part.
///
/// The status is fx's phrase (but for the unported `cd . &&` and workspace-root
/// `.`, `docs/parity.md:238`): it says nothing of a command fx stopped reading
/// (`tool_presentation.zig:441-445@481104fd` marks only what its encoder cuts),
/// and a marker there would put in a `.` fx's has not and hide `cwd: .`.
fn command_spelled(target: &str, max: usize, mark_unread: bool) -> String {
    let mut spelling = Spelling::new(max);
    let mut line_break = false;
    let mut written = false;
    for (at, character) in target.char_indices() {
        if at >= COMMAND_SOURCE_BYTES {
            // fx stops reading here and says nothing of what it left
            // (`tool_presentation.zig:268`, its marker only the encoder's,
            // `:445`): a row that hides its own tail without a word is not
            // safe to copy, so the row is marked cut when anything it would
            // show is left (ledger `call-row-source-cut`).
            if target[at..]
                .chars()
                .any(|left| !matches!(left, ' ' | '\t' | '\r' | '\n'))
            {
                spelling.cut |= mark_unread;
            }
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
            // The rest of it is left unread, so the row is marked cut too.
            let mut bytes = [0u8; 4];
            character.encode_utf8(&mut bytes);
            spelling.cut |= mark_unread;
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

/// An MCP call's failure reason as the row shows it. Its words can be the
/// server's own (an `isError` result's text), so it is spelled as every text
/// a server supplies is: upstream's `encodeTerminalSafeInline`
/// (`vercel-labs/fx@481104fd src/core/shared/text_utils.zig:397-417`), ASCII
/// white space flattened to one space and then [`crate::mcp::terminal_safe`]'s
/// tokens -- every other control and every invisible or bidirectional code
/// point written out rather than dropped, and the length bounded. Since fx
/// v0.0.16 such a row carries its reason, where a command's row did not
/// (`fx-p5-mcp-live-40x120-failed` row 25).
///
/// The reason has already crossed the channel spelled as [`flattened`]
/// spells every call's (`super::bridge`'s `made_inert`), which leaves raw
/// some of what this names (U+FE0F, U+00AD, ...): so the bound is
/// [`crate::mcp::terminal_safe_respelled`]'s, which keeps each escape the
/// channel wrote whole and cuts between tokens, and the row is what one pass
/// over the server's words makes of them.
fn server_spelled(reason: &str) -> String {
    let words: Vec<&str> = reason
        .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
        .filter(|word| !word.is_empty())
        .collect();
    crate::mcp::terminal_safe_respelled(&words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tools::mutate::{EDIT_FILE, WRITE_FILE};
    use crate::tools::question::ASK_USER_QUESTION;
    use crate::tools::ToolSpec;
    use crate::tools::{read, terminal};
    use serde_json::json;

    const DARK: Palette = Palette { mode: Mode::Dark };

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
                "\u{25aa} Cancelled sleep 30 \u{b7} What can xfx do differently?",
                "",
            ]
        );
        let line = &group.rows(DARK)[3];
        assert!(
            line.starts_with(&format!(
                "{WORD}\u{25aa}{RESET}{HINT} Cancelled{RESET} {TEXT}"
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
    const LIGHT: Palette = Palette { mode: Mode::Light };

    /// The rows `group` is drawn as in `palette`.
    fn drawn_in(group: &mut ToolGroup, palette: Palette) -> Vec<String> {
        group.drawn.clear();
        group.redraw(palette, &[]).expect("rows")
    }

    /// The 40x120 screen these cases put a group on.
    fn screen() -> crate::tui::layout::Geometry {
        crate::tui::layout::solve(40, 120, 1).expect("a band")
    }

    /// `rows` placed from row 1 down on a 120-column grid -- each recorded as
    /// what the group says it is and how it draws it in each mode
    /// ([`ToolGroup::kinds`], [`ToolGroup::redraws`]), as the frame records an
    /// append's rows -- and a turn's summary (the dim grey, 245 on dark like
    /// the group's own) under them.
    fn on_a_grid(
        group: &ToolGroup,
        rows: &[String],
        palette: Palette,
    ) -> (super::super::grid::Grid, u16) {
        let geometry = screen();
        let mut grid = super::super::grid::Grid::blank(geometry.rows, geometry.cols);
        let kinds = group.kinds();
        let redraws = group.redraws(palette, rows);
        let mut line = 1u16;
        for (index, row) in rows.iter().enumerate() {
            grid.place_row(line, row, &geometry);
            grid.record(
                line,
                kinds.get(index).copied().unwrap_or_default(),
                redraws.get(index).cloned().flatten(),
            );
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
        assert!(rows[4].contains('\u{25aa}'), "{rows:?}");
        assert!(ToolGroup::default().kinds().is_empty());
    }

    #[test]
    fn the_groups_header_and_calls_carry_their_drawings_in_both_modes_and_nothing_else_does() {
        // Upstream projects the header and the calls again in the new theme
        // (`formatGroupBlock`, `tool_group_projection.zig:558-640@481104fd`)
        // and draws a cancelled call's own line from its bytes (`:649-664`),
        // so the header and each call row carry what they are in each mode --
        // exactly the rows the group draws in that mode -- and the blank rows
        // and a cancelled call's line carry nothing. Collapsed, the header
        // alone.
        let at = |mode| Palette { mode };
        let mut group = ToolGroup::default();
        group.start("1".into(), command("echo \"hi\" 42 # note"));
        assert!(group.finish("1", true, "", Outcome::Exited(0), None));
        group.start("2".into(), command("sleep 31"));
        group.cancel_running();
        let dark = drawn_in(&mut group, at(Mode::Dark));
        let light = drawn_in(&mut group, at(Mode::Light));
        let redraws = group.redraws(at(Mode::Dark), &dark);
        assert_eq!(redraws, group.redraws(at(Mode::Light), &light));
        assert_eq!(redraws.len(), group.kinds().len());
        for (index, (redraw, kind)) in redraws.iter().zip(group.kinds()).enumerate() {
            match redraw {
                Some(redraw) => {
                    assert_eq!(kind, DocumentRow::ToolGroup, "row {index}");
                    assert_eq!(redraw.drawn(Mode::Dark), dark[index]);
                    assert_eq!(redraw.drawn(Mode::Light), light[index]);
                }
                None => assert_ne!(kind, DocumentRow::ToolGroup, "row {index}"),
            }
        }
        assert_eq!(
            redraws.iter().filter(|redraw| redraw.is_some()).count(),
            3,
            "the header and two calls"
        );
        group.set_collapsed(true);
        let collapsed_rows = drawn_in(&mut group, at(Mode::Dark));
        let collapsed = group.redraws(at(Mode::Dark), &collapsed_rows);
        assert_eq!(collapsed.len(), 2);
        assert!(collapsed[0].is_some() && collapsed[1].is_none());
        assert!(ToolGroup::default().redraws(DARK, &[]).is_empty());
    }

    #[test]
    fn a_theme_report_repaints_a_group_as_the_other_palette_would_draw_it() {
        // The 245 of a group's rows is upstream's statusline grey and goes to
        // 241 on a light screen; the summary under it is the dim grey, also
        // 245 on dark, and goes to 247. A command's words go 252 -> 238 and
        // the mark 255 -> 235. Each cell of the group's own rows is compared
        // with what drawing the same group in the light palette leaves --
        // upstream projects them again (`tool_group_projection.zig:558-640`).
        // A cancelled call's own line is not projected again but rewritten
        // through upstream's token table (`:649-664`, `store.zig:3784-3800`),
        // so its argument, drawn 245 on either screen, is 247 after the
        // switch where a fresh light drawing has 245 (`p5-toolstop-retint`).
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
        let moved = repainted.retint_document(last, &LIGHT, &screen());
        assert!(moved > 0, "nothing was repainted");
        let light_rows = drawn_in(&mut group, LIGHT);
        let (fresh, _) = on_a_grid(&group, &light_rows, LIGHT);
        let stopped = (1..=last)
            .find(|&line| repainted.kind(line) == DocumentRow::ToolStopped)
            .expect("a cancelled call's own line");
        same_cells(&repainted, &fresh, stopped - 1);
        let argument = (1..=repainted.cols())
            .find(|&column| {
                matches!(repainted.cell(stopped, column),
                    Some(super::super::grid::Cell::Lead { grapheme, .. }) if grapheme == "3")
            })
            .expect("the argument's `30`");
        let Some(super::super::grid::Cell::Lead { sgr, .. }) = repainted.cell(stopped, argument)
        else {
            unreachable!()
        };
        assert_eq!(
            sgr.color().ok(),
            Some(crate::tui::check::Color::Indexed(247)),
            "a cancelled call's argument did not move as upstream's token table moves it"
        );

        // And back: a light screen reported dark again is the dark drawing --
        // a quoted string and a number too, which on light share 241 with the
        // statusline, and a comment, which on dark shares its 245
        // (`p5-toolgroup-retint-light`).
        let mut back_group = ToolGroup::default();
        back_group.start("1".into(), command("echo \"hi\" 42 | grep hi # note"));
        assert!(back_group.finish("1", true, "", Outcome::Exited(0), None));
        let light_rows = drawn_in(&mut back_group, LIGHT);
        let (mut back, last) = on_a_grid(&back_group, &light_rows, LIGHT);
        assert!(back.retint_document(last, &DARK, &screen()) > 0);
        let dark_rows = drawn_in(&mut back_group, DARK);
        let (dark, _) = on_a_grid(&back_group, &dark_rows, DARK);
        same_cells(&back, &dark, last - 1);
        // And light again, from the drawing rather than from what the dark
        // one left on the screen.
        assert!(back.retint_document(last, &LIGHT, &screen()) > 0);
        let (light, _) = on_a_grid(&back_group, &light_rows, LIGHT);
        same_cells(&back, &light, last - 1);
    }

    #[test]
    fn a_calls_command_is_coloured_as_a_fresh_colouring_would_and_kept_per_mode_and_grey() {
        // The colourings a call keeps are what the highlighter gives fresh,
        // in each mode and run colour, in any order of switches: a
        // group drawn again and again is the group drawn once.
        let calls = |group: &mut ToolGroup| {
            group.start("1".into(), command("echo \"hi\" 42 | grep hi # note"));
            group.start("2".into(), command("sleep 30 # wait"));
            group.cancel_running();
        };
        let mut kept = ToolGroup::default();
        calls(&mut kept);
        for palette in [DARK, LIGHT, DARK, LIGHT, LIGHT, DARK, DARK] {
            let mut fresh = ToolGroup::default();
            calls(&mut fresh);
            let (kept_rows, fresh_rows) =
                (drawn_in(&mut kept, palette), drawn_in(&mut fresh, palette));
            assert_eq!(kept_rows, fresh_rows, "{palette:?}");
            assert_eq!(
                kept.redraws(palette, &kept_rows),
                fresh.redraws(palette, &fresh_rows),
                "{palette:?}"
            );
            for call in &kept.calls {
                assert!(call.highlights.borrow().len() <= HIGHLIGHTS_KEPT);
            }
        }
        // Each mode, under the row's grey and the stopped
        // line's -- three, since on a dark screen the two are one grey (245).
        let mut one = ToolGroup::default();
        calls(&mut one);
        for palette in [DARK, LIGHT, DARK, LIGHT] {
            let rows = drawn_in(&mut one, palette);
            one.redraws(palette, &rows);
        }
        assert_eq!(one.calls[0].highlights.borrow().len(), 3);
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
                "\u{25aa} Cancelled mcp_web_echo \u{b7} What can xfx do differently?",
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
    fn an_mcp_reason_spelled_by_the_channel_is_cut_as_one_pass_cuts_the_raw_one() {
        // Train 2's seam, at every cut point: the channel's spelling
        // ([`flattened`]) and then the MCP row's ([`server_spelled`]) give
        // what the row's spelling gives the server's words in one pass, for
        // a reason whose escapes, marks and wider-set characters the 256-byte
        // bound falls among, at every length from well under to well over it.
        fn one_pass(reason: &str) -> String {
            let words: Vec<&str> = reason
                .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
                .filter(|word| !word.is_empty())
                .collect();
            crate::mcp::terminal_safe(&words.join(" "))
        }
        let tails = [
            "\u{fe0f}\u{fe0f}\u{1b}\u{1b}\u{1b}x",
            "\u{ad}\\\u{202e}\u{9b}\u{7f}\u{ad}y",
            "\u{61c}\u{feff}\u{fe0f}\t\r\n\u{200b}\u{b}\u{3164}w",
        ];
        for tail in tails {
            for n in 150..300 {
                let raw = format!("lead \\ {}{tail}{tail}", "A".repeat(n));
                let crossed = flattened(&raw);
                let row = server_spelled(&crossed);
                assert_eq!(row, one_pass(&raw), "{n} {tail:?}");
                // Spelled twice it is the same: the row's text is a fixed
                // point of its own spelling.
                assert_eq!(server_spelled(&row), row);
            }
        }
        // Text that only looks like an escape -- a server's literal `\x1b`
        // -- is printable characters, which the channel's bound may cut as
        // it cuts any text, and reads the same as an escape once spelled: the
        // row keeps what reaches it within the bound, and spells it once.
        for n in 150..300 {
            let raw = format!(
                "lead {}\\x1b\\u{{202e}}\u{fe0f}\u{1b}\\x7f\\u{{9b}}z",
                "A".repeat(n)
            );
            let row = server_spelled(&flattened(&raw));
            assert!(row.len() <= crate::mcp::TERMINAL_SAFE_MAX_BYTES, "{n}");
            assert_eq!(server_spelled(&row), row);
        }
        // A backslash that begins no escape is a character like any other.
        assert_eq!(
            crate::mcp::terminal_safe_respelled("a\\xzz\\u{}b\\"),
            "a\\xzz\\u{}b\\"
        );
    }

    #[test]
    fn escape_shaped_server_text_at_the_bound_is_kept_whole_and_the_row_ends_before_it() {
        // The one place the row is not what one pass over the server's words
        // gives: text shaped like an escape (a literal `\x1b`) that
        // straddles the cut, byte 253, is kept whole as the channel's own
        // escapes are, so the row ends before it where one pass ends inside
        // it.
        fn one_pass(reason: &str) -> String {
            let words: Vec<&str> = reason
                .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
                .filter(|word| !word.is_empty())
                .collect();
            crate::mcp::terminal_safe(&words.join(" "))
        }
        let failed_row = |reason: &str| {
            let mut group = ToolGroup::default();
            group.start("1".into(), mcp_call("mcp_web_echo"));
            assert!(group.finish("1", false, reason, Outcome::Finished, None));
            plain(&group)[1].clone()
        };
        // Straight to the row: 252 `A`, a literal `\x1b`, `Z`.
        let raw = format!("{}\\x1bZ", "A".repeat(252));
        assert_eq!(one_pass(&raw), format!("{}\\...", "A".repeat(252)));
        assert_eq!(
            failed_row(&raw),
            format!("\u{2514} Failed mcp_web_echo: {}...", "A".repeat(252))
        );
        // Through the channel: 244 `A`, U+00AD, a literal `\x1b`, `Z`. The
        // channel leaves U+00AD raw (2 bytes, 251 in all, under its bound);
        // the row names it in 8, which carries the literal across the cut.
        let raw = format!("{}\u{ad}\\x1bZ", "A".repeat(244));
        let crossed = flattened(&raw);
        assert_eq!(crossed, raw);
        assert_eq!(
            one_pass(&raw),
            format!("{}\\u{{00ad}}\\...", "A".repeat(244))
        );
        assert_eq!(
            failed_row(&crossed),
            format!(
                "\u{2514} Failed mcp_web_echo: {}\\u{{00ad}}...",
                "A".repeat(244)
            )
        );
    }

    #[test]
    fn an_mcp_reason_with_nothing_left_once_spelled_is_no_reason() {
        // The MCP arm tests the reason after spelling it, as fx's
        // `failureStatusDetail` returns none for an empty encoding: a reason
        // of white space alone, given to the row raw, leaves no `: `.
        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call("mcp_web_echo"));
        assert!(group.finish("1", false, "\u{b}", Outcome::Finished, None));
        assert_eq!(plain(&group)[1], "\u{2514} Failed mcp_web_echo");
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

    /// Whether every escape in a drawn row is one of the palette's own
    /// (a 256-colour foreground, its reset, or the full reset) and no other
    /// control or bidirectional code point is in it: what a row must be
    /// before any strip, for a name a server chose.
    fn inert(row: &str) -> bool {
        let palette_only = row.match_indices('\u{1b}').all(|(at, _)| {
            let rest = &row[at + 1..];
            rest.starts_with("[38;5;") || rest.starts_with("[39m") || rest.starts_with("[0m")
        });
        palette_only
            && !row
                .chars()
                .any(|c| (c.is_control() && c != '\u{1b}') || c == '\u{202e}')
    }

    #[test]
    fn a_hostile_tool_name_is_written_out_on_every_row_it_reaches() {
        // A server's tool name with an escape and a bidirectional override in
        // it, through every row an MCP call can have, the cancelled line and
        // the full transcript's rows (`expanded`): spelled out by
        // `crate::mcp::terminal_safe`, never sent to the terminal.
        const NAME: &str = "mcp_evil\u{1b}[31m\u{202e}x";
        const SPELLED: &str = "mcp_evil\\x1b[31m\\u{202e}x";
        let check = |group: &ToolGroup, row: usize, want: String| {
            let drawn = group.rows(DARK);
            assert!(inert(&drawn[row]), "{:?}", drawn[row]);
            assert_eq!(strip(&drawn[row]), want);
            for line in group.expanded(DARK) {
                assert!(inert(&line), "{line:?}");
            }
        };

        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call(NAME));
        check(&group, 1, format!("\u{2514} Running MCP {SPELLED}"));
        ok(&mut group, "1");
        check(&group, 1, format!("\u{2514} Ran MCP {SPELLED}"));

        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call(NAME));
        assert!(group.finish("1", false, "no", Outcome::Denied, None));
        check(&group, 1, format!("\u{2514} Denied {SPELLED}"));

        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call(NAME));
        assert!(group.finish("1", false, "boom", Outcome::Finished, None));
        check(&group, 1, format!("\u{2514} Failed {SPELLED}: boom"));

        let mut group = ToolGroup::default();
        group.start("1".into(), mcp_call(NAME));
        assert_eq!(group.cancel_running(), ["1".to_string()]);
        check(&group, 1, format!("\u{2514} Cancelled {SPELLED}"));
        check(
            &group,
            3,
            format!("\u{25aa} Cancelled {SPELLED} \u{b7} What can xfx do differently?"),
        );
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
        // `src/core/output/diff.zig:10`). Every other argument fx writes as it
        // came, at any length, and so here, spelled.
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
            shown(
                &WRITE_FILE,
                json!({"path": pair.repeat(5_000), "content": "x"}),
            ),
        );
        assert!(group.finish("1", true, "", Outcome::Finished, None));
        let row = &plain(&group)[1];
        let expected = format!("\u{2514} Wrote {}A...", spelled_pair.repeat(454));
        assert!(*row == expected, "{}", &row[row.len() - 40..]);
        let mut group = ToolGroup::default();
        group.start(
            "1".into(),
            shown(&read::READ_FILE, json!({"path": pair.repeat(5_000)})),
        );
        let row = &plain(&group)[1];
        let expected = format!("\u{2514} Reading {}", spelled_pair.repeat(5_000));
        assert!(*row == expected, "{}", &row[row.len() - 40..]);
    }

    #[test]
    fn a_command_read_only_in_part_says_so_where_fx_says_nothing() {
        // fx reads a command's first 14 400 bytes and marks nothing when what
        // it read fits (`projectRunCommandActivitySource`, `tool_presentation.
        // zig:268@481104fd`; the marker only `encodeTerminalSafe`'s, `:445`):
        // one that is mostly line breaks shows its head alone, its tail hidden
        // without a word. Here the row says it was cut -- in the transcript
        // and on Ctrl-O, where a row that hides its own tail is not safe to
        // copy (ledger `call-row-source-cut`) -- but only when something the
        // row would show is left.
        let breaks = "\n".repeat(14_400);
        for (command_text, row) in [
            (
                format!("echo shown{breaks}echo hidden"),
                "Running echo shown...",
            ),
            (format!("echo shown{breaks} \t\r\n"), "Running echo shown"),
            // A character the bound splits: the byte read is `\xNN`, as fx
            // writes it, and the rest is marked -- with more after it, and
            // when it is the command's last.
            (
                format!("echo shown{}\u{202e}z", "\n".repeat(14_389)),
                r"Running echo shown \xe2...",
            ),
            (
                format!("echo shown{}\u{202e}", "\n".repeat(14_389)),
                r"Running echo shown \xe2...",
            ),
        ] {
            let mut group = ToolGroup::default();
            group.start("1".into(), command(&command_text));
            assert_eq!(plain(&group)[1], format!("\u{2514} {row}"));
            let expanded = strip(&group.expanded(DARK)[1]);
            assert_eq!(expanded, format!("\u{2514} {row}"));
            // And spelled again, as the row spells what the channel spelled.
            let once = spelled(&command_text, terminal::TERMINAL.presentation());
            assert_eq!(spelled(&once, terminal::TERMINAL.presentation()), once);
        }
    }

    #[test]
    fn a_kinds_word_for_more_than_one_call_is_upstreams_table() {
        // `formatGroupSummary` (`tool_group_projection.zig:520-545@481104fd`):
        // a count of 1 takes the label as written; any other takes
        // `commands`, `subagents` or `MCP calls`, and every other label stays
        // as it is (`2 read`, `3 edit`). The labels are upstream's
        // `category_labels` (`:143-153`); `open`, `subagent` and `browser`
        // name tools xfx does not have, so only this table knows them.
        for label in [
            "read", "list", "write", "edit", "open", "command", "subagent", "browser", "MCP call",
        ] {
            assert_eq!(category_word(label, 1), label);
        }
        for (label, plural) in [
            ("read", "read"),
            ("list", "list"),
            ("write", "write"),
            ("edit", "edit"),
            ("open", "open"),
            ("command", "commands"),
            ("subagent", "subagents"),
            ("browser", "browser"),
            ("MCP call", "MCP calls"),
        ] {
            for count in [2, 3, 12] {
                assert_eq!(category_word(label, count), plural, "{label} x{count}");
            }
        }
        // Upstream's own case (`group summary uses singular and plural
        // subagent labels`, `:1509-1524`), as the header joins the words.
        let words = |counts: &[(usize, &str)]| -> String {
            counts
                .iter()
                .map(|(count, label)| format!(" \u{b7} {count} {}", category_word(label, *count)))
                .collect()
        };
        assert_eq!(
            words(&[(1, "command"), (1, "subagent")]),
            " \u{b7} 1 command \u{b7} 1 subagent"
        );
        assert_eq!(
            words(&[(3, "subagent"), (2, "command")]),
            " \u{b7} 3 subagents \u{b7} 2 commands"
        );
    }

    /// Where each of `raw`'s grapheme clusters ends once spelled: the places
    /// a cut of the spelled argument may fall without cutting one in two.
    fn cluster_ends(raw: &str) -> Vec<usize> {
        use unicode_segmentation::UnicodeSegmentation;
        let presentation = read::GLOB_FILES.presentation();
        let mut at = 0;
        let mut ends = vec![0];
        for cluster in raw.graphemes(true) {
            at += spelled(cluster, presentation).len();
            ends.push(at);
        }
        ends
    }

    /// The transcript's row `row` and the row the whole argument makes, as a
    /// screen `cols` wide shows each (`super::super::transcript`'s
    /// `ellipsized`, which is how the transcript writes a block's rows).
    fn shown_at(row: &str, whole: &str, cols: u16) -> (String, String) {
        (
            super::super::transcript::ellipsized(row, cols),
            super::super::transcript::ellipsized(whole, cols),
        )
    }

    /// The widths a screen is tested at: the scenarios', a wide one, and the
    /// widest whose cut `wrap::width`, saturating at `u16::MAX`, still sees.
    const WIDTHS: [u16; 4] = [40, 120, 1000, u16::MAX - 1];

    #[test]
    fn the_transcripts_row_shows_at_every_width_what_the_whole_row_does_and_ctrl_o_the_whole() {
        // T-U4 rounds 2 and 3: a read's, a list's or a pattern's argument is
        // the model's at any length on fx's Ctrl-O (`formatExpandedChild`,
        // never clipped) -- up to the stream's 8 MiB here, four times that
        // spelled -- and every redraw of the open group drew it whole in both
        // modes (0.46 s a redraw for 8 MiB of `a`, receipts/cancel-marker/
        // round2). The transcript's own row shows at most a screen's width,
        // so it keeps what the widest screen shows and Ctrl-O's heading keeps
        // it all -- and at every width a screen shows exactly what it shows
        // of the whole row, with no cut inside one of the argument's
        // clusters, however many bytes one holds.
        let wide = usize::from(u16::MAX) + 1;
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}";
        let heavy = format!("e{}", "\u{301}".repeat(5000));
        let cases = [
            // Past any screen's width.
            "a".repeat(3 * wide),
            // Spelled eight times as wide as it came.
            "\u{9b}".repeat(wide),
            // One cluster of 196 609 code points that paints one cell, and
            // what follows it: on every screen.
            format!("a{}tail", "\u{301}".repeat(3 * wide)),
            // A run of ZWJ families, each one cluster: its joiners are
            // spelled, so each is 32 cells on the row.
            family.repeat(wide / 16),
            // A heavy cluster in the last column the row keeps, and one just
            // past it.
            format!("{}{heavy}bcd", "a".repeat(wide - 1)),
            format!("{}{heavy}bcd", "a".repeat(wide)),
        ];
        for pattern in cases {
            let spelled_whole = spelled(&pattern, read::GLOB_FILES.presentation());
            let mut group = ToolGroup::default();
            group.start(
                "1".into(),
                shown(&read::GLOB_FILES, json!({"pattern": pattern})),
            );
            let rows = group.rows(DARK);
            let whole = format!("{TEXT}\u{2514} Matching {spelled_whole}{RESET}");
            for cols in WIDTHS {
                let (row, all) = shown_at(&rows[1], &whole, cols);
                assert!(
                    row == all,
                    "{cols} columns: {} bytes, {} whole",
                    row.len(),
                    all.len()
                );
            }
            // What it keeps is the argument up to the end of one of its
            // clusters, and no further than the widest screen shows. Its
            // *raw* clusters: these arguments reach the group unspelled, as
            // only a test hands one; through the channel the units are the
            // spelled text's (the next test).
            let kept = strip(&rows[1]);
            let kept = kept.strip_prefix("\u{2514} Matching ").expect("the row");
            let (kept, cut) = match kept.strip_suffix("...") {
                Some(kept) if kept.len() < spelled_whole.len() => (kept, true),
                _ => (kept, false),
            };
            assert!(spelled_whole.starts_with(kept));
            assert!(
                cluster_ends(&pattern).contains(&kept.len()),
                "cut at byte {} of {}, inside a cluster",
                kept.len(),
                spelled_whole.len()
            );
            let cells = |text: &str| -> usize {
                super::super::wrap::painting(text)
                    .map(|painted| usize::from(super::super::wrap::cluster_width(painted.cluster)))
                    .sum()
            };
            assert!(
                cells(kept) <= wide,
                "the transcript's row keeps {} cells",
                cells(kept)
            );
            // Cut only when the whole is wider than that.
            assert_eq!(cut, cells(&spelled_whole) > wide);
            // Ctrl-O's heading is the whole argument.
            let heading = strip(&group.expanded(DARK)[1]);
            assert!(heading == format!("\u{2514} Matching {spelled_whole}"));
        }
    }

    #[test]
    fn the_transcripts_row_cuts_a_spelled_argument_between_its_spellings() {
        // T-U4 round 3: the channel spells a call's argument as it crosses
        // (`super::super::bridge`'s `spelled_shown`), so the row is given
        // `\u{009b}` and `\x01` as text. Wherever the row's bound falls, it
        // falls between two of them, never inside one -- whatever runs
        // before them. T-U4-F: an Arabic number sign (U+0600), which no
        // spelling names, joins the `\` after it into one cluster (UAX #29
        // GB9b), so it runs before a spelling too, as the lead and between
        // spellings.
        let wide = usize::from(u16::MAX) + 1;
        let whole_spellings = |mut text: &str| {
            while !text.is_empty() {
                match ["\\u{009b}", "\\x01", "\u{600}"]
                    .iter()
                    .find(|token| text.starts_with(**token))
                {
                    Some(token) => text = &text[token.len()..],
                    None => return false,
                }
            }
            true
        };
        let leads = ["", "a", "ab", "abc", "abcd", "abcde", "abcdef", "abcdefg"];
        let prepended = ["\u{600}", "a\u{600}", "ab\u{600}", "\u{600}\u{600}"];
        let cases = leads
            .iter()
            .chain(&prepended)
            .map(|lead| (*lead, "\u{9b}\u{1}"))
            .chain(leads.iter().map(|lead| (*lead, "\u{600}\u{1}")))
            .chain(
                leads
                    .iter()
                    .map(|lead| (*lead, "\u{600}\u{9b}\u{600}\u{1}")),
            );
        for (lead, body) in cases {
            // Every body at least four cells a repeat: past any screen.
            let pattern = format!("{lead}{}", body.repeat(wide / 3));
            let event = super::super::bridge::UiEvent::ToolStart {
                call_id: "1".into(),
                tool: "glob_files".into(),
                shown: shown(&read::GLOB_FILES, json!({"pattern": pattern})),
            }
            .made_inert();
            let super::super::bridge::UiEvent::ToolStart {
                shown: spelled_shown,
                ..
            } = event
            else {
                panic!("a start");
            };
            let spelled_whole = spelled_shown.target.clone().expect("a pattern");
            let mut group = ToolGroup::default();
            group.start("1".into(), spelled_shown);
            let rows = group.rows(DARK);
            let kept = strip(&rows[1]);
            let kept = kept
                .strip_prefix("\u{2514} Matching ")
                .and_then(|kept| kept.strip_suffix("..."))
                .unwrap_or_else(|| panic!("{lead:?} {body:?}: not cut"));
            let tokens = kept.strip_prefix(lead).expect("the lead");
            assert!(
                whole_spellings(tokens),
                "{lead:?} {body:?}: cut inside a spelling, ...{:?}",
                &tokens[tokens.len() - 12..]
            );
            let whole = format!("{TEXT}\u{2514} Matching {spelled_whole}{RESET}");
            for cols in WIDTHS {
                let (row, all) = shown_at(&rows[1], &whole, cols);
                assert!(row == all, "{lead:?} {body:?}, {cols} columns");
            }
        }
    }

    #[test]
    fn a_unit_of_the_row_is_a_cluster_or_a_spelling_with_what_joins_its_last_character() {
        // `inline`'s units: a cut falls between two of them. A mark after a
        // spelling joins the spelling's last character on the screen, so it
        // is the spelling's; a skin tone after one is too (an extend).
        for (text, unit) in [
            ("ab", "a"),
            ("e\u{301}\u{301}x", "e\u{301}\u{301}"),
            ("\\u{009b}x", "\\u{009b}"),
            ("\\x01\u{301}y", "\\x01\u{301}"),
            ("\\u{009b}\u{1f3fb}x", "\\u{009b}\u{1f3fb}"),
            ("\\u{10ffff}", "\\u{10ffff}"),
            // Not spellings: a bare backslash, too few or too many digits.
            ("\\xq1", "\\"),
            ("\\x0", "\\"),
            ("\\u{}", "\\"),
            ("\\u{1234567}", "\\"),
            ("\\u{12", "\\"),
        ] {
            assert_eq!(&text[..unit_at(text)], unit, "{text:?}");
        }
    }

    #[test]
    fn a_whole_argument_is_spelled_as_its_characters_are_one_by_one() {
        // `spelled_whole`'s one pass is `Spelling`'s token a character, the
        // Arabic letter mark's included, and text with nothing to spell comes
        // back as it is.
        for text in [
            "plain text",
            "a\u{9b}b\u{1}c\u{7f}d\u{202e}e\u{200d}f\u{feff}g\u{301}",
            "h\u{61c}i\u{9b}j",
            "\\u{009b} already spelled \\x01",
            "",
        ] {
            let mut spelling = Spelling::new(usize::MAX);
            for character in text.chars() {
                assert!(spelling.push(character));
            }
            assert_eq!(spelled_whole(text), spelling.finish(), "{text:?}");
        }
    }

    #[test]
    fn a_unit_ends_before_a_spelling_whatever_joins_its_backslash() {
        // T-U4-F: a character UAX #29 calls Prepend (GB9b: U+0600-0605,
        // U+06DD, U+070F, ...) joins the character after it into one cluster,
        // a spelling's `\` included, and the channel leaves every one of them
        // as it is. Every character that joins a following `\` and is not
        // spelled is its own unit, and the spelling after it is one.
        use unicode_segmentation::UnicodeSegmentation;
        let mut joiners = Vec::new();
        for character in (0..=0x10_ffff).filter_map(char::from_u32) {
            let text = format!("{character}\\x01z");
            let joins = text.graphemes(true).next().map(str::len) != Some(character.len_utf8());
            let unspelled = matches!(
                crate::tools::question::terminal_safe(&text[..character.len_utf8()]),
                std::borrow::Cow::Borrowed(_)
            ) && character != '\u{061c}';
            if joins && unspelled {
                joiners.push(character);
                assert_eq!(
                    unit_at(&text),
                    character.len_utf8(),
                    "U+{:04X} took the spelling's `\\` into its unit",
                    u32::from(character)
                );
                assert_eq!(unit_at(&text[character.len_utf8()..]), 4, "the spelling");
            }
        }
        assert!(
            joiners.contains(&'\u{600}') && joiners.contains(&'\u{70f}'),
            "{joiners:?}"
        );
        // A `\` that starts no spelling stays in the cluster that took it.
        assert_eq!(unit_at("\u{600}\\q"), "\u{600}\\".len());
    }
}
