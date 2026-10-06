//! The seam between the UI thread and the runtime: three channels, one
//! cancellation pair, and a mini-executor for the two producers that are
//! synchronous and cannot be made otherwise.
//!
//! The topology is the spec's (`.prd/03-tui-port.md` §"Runtime topology"), and
//! its shape is one rule: **the UI thread never blocks on the runtime and the
//! runtime never touches the terminal.** What crosses between them is data, on
//! channels whose capacities are themselves the policy:
//!
//! | Channel | Direction | Capacity | Full or closed |
//! |---|---|---|---|
//! | [`UiEvent`] | runtime -> UI | [`UI_EVENTS`] | the producer **awaits** a permit -- nothing is dropped while the UI lives |
//! | [`TurnControl`] | UI -> runtime | unbounded | never blocks the UI, consumed mid-turn |
//! | [`TurnWork`] | UI -> runtime | [`super::worker::WORK_LIMIT`] | a full channel is a visible refusal, never a silent drop |
//!
//! Backpressure on the first one is the point rather than an inconvenience: a
//! producer parked in [`send_ui`] is a socket that is not being read, which is
//! what makes a model that streams faster than a terminal can paint slow down
//! instead of filling memory. The UI applies it deliberately as well as by
//! being slow: it stops taking events at all while the pacer's queue is at
//! `super::event_loop::PACED_BACKLOG`, which is what bounds that queue.
//!
//! **Cancellation is a pair**, because half of the code that has to observe it
//! cannot await: the transport polls an atomic ([`CancelToken`], which is
//! `gateway`'s and is not this plan's to change), and a task parked on a full
//! channel or a quiet socket needs a future that *wakes* it
//! ([`CancellationToken`]). [`Cancellation`] keeps the two in one place and
//! fixes the order they are written in -- the mirror first, then the token --
//! so that a waiter the token wakes cannot read a mirror that still says
//! "running".
//!
//! **Two producers are synchronous and stay that way.** `output::EventSink`
//! and `permission::ApprovalPrompter` are `fn`s, not `async fn`s, and both are
//! defined outside this plan's boundary. The tempting bridge --
//! `tokio::sync::mpsc::Sender::blocking_send` -- **panics** when it is called
//! from inside a runtime rather than applying backpressure, and both of these
//! are called from inside one. [`park_on`] polls the real future on the calling
//! thread instead, which keeps every property the awaited producer has.
//!
//! **Nothing foreign reaches the terminal still able to command it.** Every
//! event that crosses to the UI goes through [`send_ui`] or [`send_terminal`],
//! and both make it [`inert`] first: a model's answer, a tool's report, and an
//! approval's excerpt are *text*, and `frame::place` writes a row's bytes
//! through -- it strips `CR` and `LF` and nothing else, so an `ESC [ 2 J` in a
//! transcript row is a screen the model cleared. The strip is here, at the
//! channel, rather than in the renderer, because the renderer is where Task 13
//! deliberately *puts* `SGR` sequences of its own; a blanket strip there would
//! have to be undone the moment colour arrives.

// Task 11's worker is what spawns these channels and what hands the two
// synchronous seams to a turn. They are declared as one module a task early
// because the topology is one design -- capacities, cancellation order, and
// the sync bridge are the same decision -- and splitting it across the commit
// that uses each piece is how the pieces stop agreeing.
#![allow(dead_code)]

use std::borrow::Cow;
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::gateway::CancelToken;
use crate::output::{Event, EventSink};
use crate::permission::{ApprovalAnswer, ApprovalDiff, ApprovalRequest, ApprovalSubject};
use crate::tools::{LineChanges, Outcome, Shown};

/// The most answer text one `UiEvent` may carry.
///
/// **This is what makes the UI's queue finite**, and without it that queue has
/// no bound at all. `super::event_loop::PACED_BACKLOG` stops the loop taking
/// the *next* event, which bounds the backlog to itself plus whatever one event
/// carried -- and nothing on this side of the channel promised that a delta is
/// any particular size. A provider is free to answer in one frame, `gateway/`
/// has no delta-size contract, and a `Shell::apply` cannot refuse an event it
/// has already been handed. So the size is settled *here*, before the text ever
/// becomes an event: a delta longer than this is divided and each piece waits
/// for its own permit, exactly as the table above says every event does. The
/// UI's queue is then never larger than `PACED_BACKLOG + DELTA_SLICE`, which is
/// a number rather than a hope -- for every input but one, and [`slices`] names
/// that one and what is done about it.
///
/// Dividing is free of consequence, which is why it is the lever: a transcript
/// push is invariant under chunking (`super::transcript`, proven at every split
/// of every stream it is given), so what the document ends up holding does not
/// depend on where the pieces were cut. What it *is* sensitive to is cutting
/// inside one of the terminal's own units, and [`slices`] does not.
///
/// Sized so that dividing is the rare case rather than the rule: ordinary
/// deltas are bytes to kilobytes, so nothing a real provider streams is ever
/// cut at all, and the ceiling is only reached by an answer that arrived whole.
pub(crate) const DELTA_SLICE: usize = 64 * 1024;

/// How many `UiEvent`s may be in flight before the runtime is made to wait.
///
/// Deep enough that a burst of deltas does not park the decode for one frame's
/// worth of painting, shallow enough that a UI which has stopped painting stops
/// the turn rather than buffering an answer no one is reading.
pub(crate) const UI_EVENTS: usize = 256;

/// Something the runtime did that the UI has to show.
///
/// Every `String` in here is text the UI will eventually place on a row, and
/// none of it is xfx's own: it is a model's, a tool's, or a panic's. The two
/// sends are what make it [`inert`], so a variant added later inherits the
/// policy by being sent rather than by its author remembering it -- add its
/// text fields to [`UiEvent::made_inert`] and that is all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiEvent {
    /// A turn has begun on the runtime thread.
    ///
    /// **Not** the submission, and the difference is the whole reason this
    /// event exists. A prompt is accepted the moment it is typed and may then
    /// wait behind another turn for a minute; the runtime picking it up is a
    /// fact only the runtime has, and it is the fact the band's activity row
    /// is about ([`super::activity`]). Told rather than inferred: the place a
    /// concluded turn holds is given back *after* its conclusion is sent
    /// (`super::worker`'s `turn_loop`), so a UI counting places would read
    /// either number depending on which thread ran last.
    TurnStarted,
    /// A fragment of the answer, in arrival order.
    Delta(String),
    /// A tool call was admitted and is about to run, and what the transcript
    /// names it by.
    ToolStart {
        call_id: String,
        tool: String,
        shown: Shown,
    },
    /// A tool call finished, correlated to its start by `call_id`, and how it
    /// ended for the person watching ([`Outcome`]) beside how it ended for the
    /// model (`ok`).
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
        detail: String,
        outcome: Outcome,
        changes: Option<LineChanges>,
    },
    /// Something the session wants said that is not part of an answer, as a
    /// notice whose tone and topic the side that knows what happened chose
    /// ([`super::notice`]).
    Notice(super::notice::Notice),
    /// What the user said when they answered an approval, on its way to the
    /// document.
    ///
    /// **Not an echo of the keystroke.** It is produced where the journal frame
    /// and the user message are (`crate::agent::machine`'s `flush_amendments`),
    /// so what the transcript shows is what the model was really told: a
    /// sentence the user typed and then interrupted out of reaches neither, and
    /// a sentence that reached one reached all three.
    ToolFeedback { call_id: String, text: String },
    /// What the session's journal recorded about a call that the document is
    /// never shown -- its arguments, its whole result -- for the full
    /// transcript (`super::full_transcript::Teed`).
    ///
    /// Its own variant rather than more fields on [`Self::ToolStart`] and
    /// [`Self::ToolResult`]: those are the turn machine's events and carry
    /// what the inline transcript draws; this is the journal's record, read
    /// at the moment it is written down, and nothing but the full transcript
    /// reads it.
    ToolDetail {
        call_id: String,
        detail: super::full_transcript::Detail,
    },
    /// A record only the full transcript shows: one provider request the turn
    /// settled (`super::full_transcript::Networked`), as upstream's
    /// `full_detail_record` event carries its `* network:` notice
    /// (`orchestrator.zig:4251-4280`).
    FullDetail(super::notice::Notice),
    /// A question only the person at the terminal can answer (Task 17).
    ///
    /// Carries the id it was asked under
    /// ([`super::approval_readiness::ApprovalId`]) for the reason
    /// [`Self::Question`] carries its batch's: this channel has no other way to
    /// tell an answer to *this* question from a keystroke left over at one the
    /// turn has already given up on, and two identical-looking questions really
    /// are two questions.
    Approval(super::approval::ApprovalAsked),
    /// A batch of multiple-choice questions the **model** asked
    /// (`crate::tools::question`).
    ///
    /// Not an [`Self::Approval`] and not a variant of one: it grants nothing,
    /// refusing it is not a denial, and what comes back is text rather than an
    /// answer from a fixed set. The id is what makes a late answer discardable
    /// -- the request outlives the keystroke that answers it only if the turn is
    /// still asking.
    Question(super::question::QuestionRequest),
    /// A provider switch committed and the configuration was re-read.
    ///
    /// Sent **after** the reload, never after the write: what it carries is what
    /// the configuration now says, not what the writer intended. The two differ
    /// exactly when a layer above the profile outranks it, which is the case an
    /// operator most needs told about and the one an event built from the
    /// report would get wrong.
    ProviderSelected {
        provider: crate::provider::ProviderId,
        model: String,
        /// Whether the **new** provider has nothing to authenticate with.
        ///
        /// Carried rather than recomputed by the UI, because the question is
        /// `crate::provider::resolve_credential_for`'s and it needs the whole
        /// reloaded configuration to answer -- which lives on the runtime
        /// thread. Without it the hint row's leading segment would keep
        /// answering for the provider the session *used* to have: a machine
        /// with no Gateway key that has just switched to a keyless local daemon
        /// would go on being told to run `xfx setup`.
        missing_credential: bool,
    },
    /// What `/model <query>` came back as, from the selector that owns the rule.
    ///
    /// Sent **after** the decision, and it carries the model in force
    /// afterwards -- the id the query resolved to, or the standing one when it
    /// was refused -- for the reason [`Self::ProviderSelected`] carries the
    /// reloaded configuration's: the UI cannot know what a query becomes,
    /// because it is resolved against the **catalog** and the catalog is on the
    /// runtime thread. A band that adopted the query it submitted would put
    /// `opus` on its hint row while the turn after it was held in
    /// `anthropic/claude-opus-5`.
    ///
    /// Structured rather than a sentence: which model is in force is a fact the
    /// hint row reads, and a UI given only prose would have to parse it back
    /// out. The wording is the surface's ([`super::shell`]), exactly as the
    /// line shell's is its own (`crate::interactive`'s `apply_model`).
    ModelAnswered {
        model: String,
        /// The session's reasoning effort afterwards: the one the change chose,
        /// or the one it left standing.
        effort: crate::provider::effort::Effort,
        outcome: ModelAnswer,
        /// Whether a turn was running when the change was asked for, so it
        /// takes effect from the turn after it: upstream's `Next turn will use`
        /// rather than `Switched to` (`announceModelSelection`,
        /// `session_commands.zig:1266-1279`). Decided where the request was
        /// made -- the UI knows whether a turn was running; the runtime only
        /// ever applies a change between turns -- and carried back, so the
        /// answer is worded for the moment it was asked in.
        next_turn: bool,
        /// Whether the band already said so ([`ModelChange::announced`]), in
        /// which case this answer moves the band's own record of the model and
        /// says nothing more about a change that was accepted.
        announced: bool,
    },
    /// The provider's model catalog, bounded to what the UI will render.
    ///
    /// One event per load. The load happens on the runtime thread because it
    /// opens a socket, and the UI thread must never be the thread that waits.
    CatalogLoaded {
        provider: crate::provider::ProviderId,
        entries: Vec<crate::provider::model::CatalogEntry>,
    },
    /// The provider's catalog could not be read.
    ///
    /// No reason rides with it: what `/model`'s menu says is upstream's
    /// `Unable to load models.` -- or, for a failure asking again could
    /// change, its `retry /model.` sentence, which `retry` names
    /// (`crate::provider::model::CatalogRetry`). The selector remembers the
    /// failure, so a `/model <id>` still applies the id as typed, and the next
    /// `/model` asks again.
    CatalogUnread {
        provider: crate::provider::ProviderId,
        retry: Option<crate::provider::model::CatalogRetry>,
    },
    /// What a **completed** turn spent.
    ///
    /// Both halves optional because a provider really may publish neither
    /// (`gateway::protocol::Usage::input_tokens` is an `Option`), and an absent
    /// number must stay absent: a meter that showed nought per cent for a turn
    /// nobody measured would be reporting a measurement that was never taken.
    Usage {
        input: Option<u64>,
        output: Option<u64>,
    },
    /// A request failed in a way the turn replays, and the next attempt goes
    /// after `delay` (`crate::output::EventSink::retrying`).
    ///
    /// Already shaped for the row that says so ([`super::retry`]): the cause is
    /// this crate's own word, and the diagnostic is what the server said --
    /// which is why it is made inert like every other foreign string here.
    Retrying {
        cause: &'static str,
        diagnostic: String,
        delay: std::time::Duration,
    },
    /// The turn took these steers -- prompts the person sent into it while it
    /// ran -- and the model is being told them (`crate::agent::steering`): the
    /// band writes each one's card now, at its delivery, as upstream writes a
    /// steer's (`append_user_feedback`, `worker_runtime.zig:1233-1240`).
    Steered { ids: Vec<u64> },
    /// A retried request got through on `attempt`
    /// (`crate::output::EventSink::recovered`): the row says so for a moment
    /// ([`super::retry::Row::Recovered`]).
    Recovered { attempt: u32 },
    /// The turn is over, whichever way it went. **Terminal.**
    TurnEnded { failure: Option<String> },
    /// The conversation has been dropped: a [`TurnControl::Reset`] was acted
    /// on, and the next prompt opens a new session.
    ///
    /// **The fence a reset waits on**, and the only reason it is sent. The work
    /// a reset cancelled can still be reporting -- the tail of an answer, a tool
    /// row, the turn's conclusion -- and those events belong to the
    /// conversation the reset handed to scrollback, not to the fresh screen.
    /// The runtime takes work in order and this is sent only once everything
    /// before the drop has been sent, so on the UI's side everything that
    /// arrives ahead of it is the old conversation's and everything after it is
    /// the new one's (`super::shell`'s `draining`). Not terminal: no turn ends
    /// here, and the drain at exit does not wait for it.
    ///
    /// `through` is the reset's own count, so that two resets in a row -- with
    /// a prompt submitted between them, which the second one drops -- are told
    /// apart: the UI drains until the fence of the **last** reset it sent.
    /// Two resets that reached the same turn are answered by one fence, the
    /// later count's.
    Fresh { through: usize },
    /// The saved sessions a `/resume` picker lists, or `None` when the store
    /// could not be read (`Unable to load sessions.`).
    ///
    /// Read on the runtime thread because a listing reads one manifest per
    /// session, and the thread holding the terminal reads no file
    /// ([`super::session_picker`]). Not terminal: like a `/model` answer it
    /// ends no turn.
    Sessions(Option<super::session_picker::Listing>),
    /// The session `/resume` asked for is now the runtime's conversation, and
    /// this is what it replays as ([`super::hydration`]).
    ///
    /// Sent **after** the swap, through `send_terminal`, for the reason a
    /// provider switch's answer is: the conversation the next prompt goes to
    /// has already changed, and a cancellation that dropped the one event
    /// saying so would leave the screen showing the session that was.
    Resumed(super::hydration::Hydration),
    /// The session `/resume` asked for could not be opened, and why. The
    /// conversation is the one it was.
    ResumeFailed {
        id: String,
        failure: super::session_picker::ResumeFailure,
    },
    /// `/rename` reached the conversation: the session is called `title` from
    /// here on, and `unsaved` is why the log does not say so, when it does not
    /// (`renamed for this process but not saved`).
    ///
    /// Structured rather than a notice alone, because the window's title is
    /// the shell's to change and it changes only for a rename that happened: a
    /// rename a Ctrl-C dropped from the queue before it ran is not one.
    Renamed {
        title: String,
        unsaved: Option<String>,
    },
    /// What `/credits` found: the account's fields, or why there are none.
    Credits(crate::credits::Answer),
    /// What `/undo` did ([`TurnControl::Undo`]): upstream's `* undo: …`
    /// notice, always neutral (`commandUndoLast`, `app_commands.zig:1286-1290`).
    ///
    /// **Not a [`Self::Notice`]**, and the one difference is the reason: a
    /// notice the old conversation's work sends after a reset is dropped
    /// (`super::shell`'s `drained`), and this one is never dropped, because a
    /// file has changed -- or was refused -- by the time it is sent. An undo
    /// typed before a `/new` that the runtime got to after it is still an undo
    /// that happened, and one nobody was told of is a file that changed
    /// invisibly.
    Undone(super::notice::Notice),
    /// Where a compaction is: begun, sending its notes request, or over
    /// ([`super::compaction`]). The band's row only -- upstream writes
    /// nothing of a compaction into the transcript. Not terminal: an
    /// automatic one ends inside a turn, and a manual one ends no turn.
    Compaction(super::compaction::Update),
    /// The runtime cannot continue. **Terminal.**
    Fatal(String),
}

/// What the selector made of one `/model <id>`.
///
/// The three answers [`crate::provider::model::ModelOutcome`] gives a caller
/// that asked it to *select* something, carried across the channel without the
/// fields the UI has no use for: `Reported` belongs to a bare `/model`, which
/// this front end answers on its own thread and follows with a catalog load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelAnswer {
    /// Applied, and recorded where a resumed session will read it.
    ///
    /// `unverified` is the reason the catalog could not confirm the id -- a
    /// provider with no catalog to ask, or a load that failed -- and `None`
    /// when it did. It is not a refusal, and the band says nothing of it:
    /// upstream applies an id it could not check without a word.
    Applied { unverified: Option<String> },
    /// Already the model in force. Nothing was written down.
    Unchanged,
    /// Refused, in xfx's own words, with nothing changed and nothing recorded.
    Refused { reason: String },
}

impl UiEvent {
    /// Whether this event ends the turn it belongs to.
    ///
    /// The UI's drain loop leaves on exactly this, so the set has to be the two
    /// events a turn really cannot continue past: its conclusion, and a runtime
    /// that is gone. An event that answered `true` too eagerly would end the
    /// drain while the worker was still publishing its session log.
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::TurnEnded { .. } | Self::Fatal(_))
    }

    /// The same event with every control character the terminal would obey
    /// turned into a space.
    ///
    /// Exhaustive on purpose: a variant added later does not compile until its
    /// text has been given an answer here.
    fn made_inert(self) -> Self {
        match self {
            // No text at all: what it carries is the fact that it arrived.
            Self::TurnStarted => Self::TurnStarted,
            Self::Delta(text) => Self::Delta(inert_owned(text)),
            // `call_id` and `tool` are the registry's and the provider's, and
            // the provider's half is exactly why they are not exempt.
            // The target is the model's own argument -- a path, a pattern, a
            // command -- and is about to be painted into a row, so it is the
            // least exempt string here. The verbs are this crate's `'static`
            // words and carry nothing to make inert.
            Self::ToolStart {
                call_id,
                tool,
                shown,
            } => Self::ToolStart {
                call_id: inert_owned(call_id),
                tool: inert_owned(tool),
                shown: Shown {
                    presentation: shown.presentation,
                    target: shown.target.map(inert_owned),
                },
            },
            Self::ToolResult {
                call_id,
                tool,
                ok,
                detail,
                outcome,
                changes,
            } => Self::ToolResult {
                call_id: inert_owned(call_id),
                tool: inert_owned(tool),
                ok,
                detail: inert_owned(detail),
                outcome,
                changes,
            },
            // The topic is this crate's word and the body may quote a file, a
            // daemon or a settings layer; both are made inert, because a rule
            // that exempted one field would be a rule about which field a
            // future sender puts foreign text in.
            Self::Notice(notice) => Self::Notice(super::notice::Notice {
                tone: notice.tone,
                topic: inert_owned(notice.topic),
                body: inert_owned(notice.body),
            }),
            // The body quotes a path, which is a file's name and therefore
            // anybody's; it arrives spelled out already
            // (`crate::tools::undo::display_path`), and the rule here is about
            // the channel rather than about whoever built the value.
            Self::Undone(notice) => Self::Undone(super::notice::Notice {
                tone: notice.tone,
                topic: inert_owned(notice.topic),
                body: inert_owned(notice.body),
            }),
            // **The most foreign text on this channel.** It is typed at a
            // terminal by a person, so it is the one string here that a user
            // can put an `ESC` into on purpose -- and it is about to be painted
            // into the document. The `call_id` is the registry's and is not
            // exempt for it.
            Self::ToolFeedback { call_id, text } => Self::ToolFeedback {
                call_id: inert_owned(call_id),
                text: inert_owned(text),
            },
            // A call's arguments and a tool's output are the model's and the
            // tool's, and are about to be laid out into rows.
            Self::ToolDetail { call_id, detail } => Self::ToolDetail {
                call_id: inert_owned(call_id),
                detail: detail.made_inert(),
            },
            // The model id is a settings file's or a catalog's, as foreign as
            // the notice's own body.
            Self::FullDetail(notice) => Self::FullDetail(super::notice::Notice {
                tone: notice.tone,
                topic: inert_owned(notice.topic),
                body: inert_owned(notice.body),
            }),
            // The provider is an enum this crate wrote; the model id is a
            // string that came out of a settings file or a daemon's catalog,
            // and is therefore exactly as foreign as a delta.
            Self::ProviderSelected {
                provider,
                model,
                missing_credential,
            } => Self::ProviderSelected {
                provider,
                model: inert_owned(model),
                missing_credential,
            },
            // The model id came off a settings file, a daemon's catalog or the
            // composer, and the reason quotes the id it is about -- so both are
            // as foreign as a delta, and neither is exempt for having been
            // decided by xfx's own selector.
            Self::ModelAnswered {
                model,
                effort,
                outcome,
                next_turn,
                announced,
            } => Self::ModelAnswered {
                next_turn,
                announced,
                model: inert_owned(model),
                // A level's name is limited to a word upstream would parse
                // when it is read, but it comes off a settings file or a
                // catalog all the same, and the rule here is about the
                // channel rather than about whoever built the value.
                effort: match effort {
                    crate::provider::effort::Effort::Named(name) => {
                        crate::provider::effort::Effort::Named(inert_owned(name))
                    }
                    auto => auto,
                },
                outcome: match outcome {
                    ModelAnswer::Applied { unverified } => ModelAnswer::Applied {
                        unverified: unverified.map(inert_owned),
                    },
                    ModelAnswer::Unchanged => ModelAnswer::Unchanged,
                    ModelAnswer::Refused { reason } => ModelAnswer::Refused {
                        reason: inert_owned(reason),
                    },
                },
            },
            // **Every string of every row.** A catalog is a document a daemon
            // on a port serves, so its ids, its display names and its effort
            // labels are all text the terminal would obey if it were let
            // through -- and they are about to be painted, one per row.
            Self::CatalogLoaded { provider, entries } => Self::CatalogLoaded {
                provider,
                entries: entries
                    .into_iter()
                    .map(|entry| crate::provider::model::CatalogEntry {
                        id: inert_owned(entry.id),
                        aliases: entry.aliases.into_iter().map(inert_owned).collect(),
                        name: entry.name.map(inert_owned),
                        efforts: entry.efforts.into_iter().map(inert_owned).collect(),
                        max_context: entry.max_context,
                        facts: crate::provider::model::ModelFacts {
                            owned_by: entry.facts.owned_by.map(inert_owned),
                            ..entry.facts
                        },
                    })
                    .collect(),
            },
            // A provider tag and a category. Nothing here is text.
            Self::CatalogUnread { provider, retry } => Self::CatalogUnread { provider, retry },
            // Two numbers. There is nothing here a terminal can be made to obey.
            Self::Usage { input, output } => Self::Usage { input, output },
            // A count. There is nothing here a terminal can be made to obey.
            Self::Fresh { through } => Self::Fresh { through },
            // What a Gateway answered, and the one string a remote end chose
            // that reaches the screen verbatim. `\n` survives, and it is
            // the field separator only xfx writes: `crate::credits` quotes an
            // error body on one line and reads each field as one value, but a
            // field the Gateway wrote with a line break in it is still its
            // text, and painted as the notice's own continuation row.
            Self::Credits(answer) => Self::Credits(match answer {
                crate::credits::Answer::Failed(reason) => {
                    crate::credits::Answer::Failed(inert_owned(reason))
                }
                crate::credits::Answer::Credits {
                    balance,
                    used,
                    plan,
                } => crate::credits::Answer::Credits {
                    balance: balance.map(inert_owned),
                    used: used.map(inert_owned),
                    plan: plan.map(inert_owned),
                },
            }),
            // `tool` is a `&'static str` this crate wrote. The rest quotes a
            // path and a bounded excerpt of the content a call would change --
            // a file, in other words, which is the most likely place in the
            // whole product for an escape sequence to be sitting.
            //
            // **Both sides of the diff too**, and they are the largest quotation
            // of a file this product carries: 64 KiB each. The permission
            // boundary escapes them where the change is known
            // (`crate::permission::bounded_diff_side`) and this seam escapes
            // them again where they enter the UI, because the property this
            // channel promises is about every event it carries rather than about
            // the producers that happened to build them correctly.
            //
            // What survives both passes is the **line structure**: `inert`
            // exempts `\n` and `\r` deliberately, and a diff's breaks are the
            // change's own lines, which the review screen turns into rows. A
            // seam that flattened them here would take the shape out of the one
            // surface built to show it.
            //
            // The id is a number this crate minted, so there is nothing in it a
            // terminal can be made to obey.
            Self::Approval(asked) => Self::Approval(super::approval::ApprovalAsked {
                id: asked.id,
                request: ApprovalRequest {
                    tool: asked.request.tool,
                    target: inert_owned(asked.request.target),
                    summary: inert_owned(asked.request.summary),
                    always_scope: inert_owned(asked.request.always_scope),
                    // Where the window sits is two numbers this crate counted,
                    // so it crosses as it was.
                    diff: asked.request.diff.map(|diff| ApprovalDiff {
                        before: inert_owned(diff.before),
                        after: inert_owned(diff.after),
                        first_line: diff.first_line,
                        lines_below: diff.lines_below,
                    }),
                    // The path and the command a review lays out on rows of
                    // their own: the same quotation as the summary, so the same
                    // policy.
                    subject: match asked.request.subject {
                        ApprovalSubject::Path(path) => ApprovalSubject::Path(inert_owned(path)),
                        ApprovalSubject::Command { command, cwd } => ApprovalSubject::Command {
                            command: inert_owned(command),
                            cwd: inert_owned(cwd),
                        },
                    },
                },
            }),
            // **A belt on an encoded payload.** `crate::tools::question`'s
            // encoder has already turned every sequence a terminal would obey
            // into a literal escape before the request left the tools layer, so
            // this arm changes nothing on the path a real question travels. It
            // is here because `made_inert` is total by design: a variant that
            // named no text would be an exemption, and the next producer of one
            // of these requests -- a test, a future non-tool asker -- would
            // inherit the exemption rather than the policy.
            Self::Question(request) => Self::Question(super::question::QuestionRequest {
                id: request.id,
                entries: request
                    .entries
                    .into_iter()
                    .map(|entry| crate::tools::question::QuestionEntry {
                        question: inert_owned(entry.question),
                        options: entry
                            .options
                            .into_iter()
                            .map(|option| crate::tools::question::QuestionOption {
                                label: inert_owned(option.label),
                                description: option.description.map(inert_owned),
                            })
                            .collect(),
                    })
                    .collect(),
            }),
            // The cause is this crate's word; the diagnostic quotes a server.
            Self::Retrying {
                cause,
                diagnostic,
                delay,
            } => Self::Retrying {
                cause,
                diagnostic: inert_owned(diagnostic),
                delay,
            },
            // A count. There is nothing here a terminal can be made to obey.
            Self::Recovered { attempt } => Self::Recovered { attempt },
            // Numbers the band gave out.
            Self::Steered { ids } => Self::Steered { ids },
            Self::TurnEnded { failure } => Self::TurnEnded {
                failure: failure.map(inert_owned),
            },
            // Every title and every path a session store holds: a title is a
            // prompt somebody typed, or a `/rename` of one, and a path is a
            // directory's own name.
            Self::Sessions(listing) => {
                Self::Sessions(listing.map(|listing| super::session_picker::Listing {
                    current: listing.current.into_iter().map(inert_session).collect(),
                    all: listing.all.into_iter().map(inert_session).collect(),
                    now_ms: listing.now_ms,
                }))
            }
            // **A whole conversation read back off a disk**: every prompt,
            // every answer, every argument a model gave a tool and every first
            // line a tool answered with. The line breaks survive, as a delta's
            // do -- a prompt's and an answer's rows are made of them.
            Self::Resumed(hydration) => Self::Resumed(super::hydration::Hydration {
                title: hydration.title.map(inert_owned),
                turns: hydration
                    .turns
                    .into_iter()
                    .map(|turn| super::hydration::HydratedTurn {
                        user: inert_owned(turn.user),
                        sent: inert_owned(turn.sent),
                        steps: turn.steps.into_iter().map(inert_step).collect(),
                        ending: match turn.ending {
                            super::hydration::Ending::Interrupted(reason) => {
                                super::hydration::Ending::Interrupted(inert_owned(reason))
                            }
                            ending => ending,
                        },
                        started_at_ms: turn.started_at_ms,
                        concluded_at_ms: turn.concluded_at_ms,
                        details: turn
                            .details
                            .into_iter()
                            .map(|(id, detail)| (inert_owned(id), detail.made_inert()))
                            .collect(),
                    })
                    .collect(),
                model: hydration.model.map(inert_owned),
                // A number.
                next_image: hydration.next_image,
            }),
            // The id is the store's own name for a directory, read back off a
            // listing; the failure is this crate's word.
            Self::ResumeFailed { id, failure } => Self::ResumeFailed {
                id: inert_owned(id),
                failure,
            },
            // The title passed `validate_title` and holds no control
            // character; the reason quotes a path and an I/O error, which are
            // the filesystem's words. Neither is exempt.
            Self::Renamed { title, unsaved } => Self::Renamed {
                title: inert_owned(title),
                unsaved: unsaved.map(inert_owned),
            },
            // No text: phases and outcomes this crate names.
            Self::Compaction(update) => Self::Compaction(update),
            Self::Fatal(text) => Self::Fatal(inert_owned(text)),
        }
    }
}

/// One listed session with its text made inert.
fn inert_session(
    session: super::session_picker::ListedSession,
) -> super::session_picker::ListedSession {
    super::session_picker::ListedSession {
        id: inert_owned(session.id),
        title: session.title.map(inert_owned),
        workspace: inert_owned(session.workspace),
        updated_at_ms: session.updated_at_ms,
        turns: session.turns,
    }
}

/// One replayed step with its text made inert, as the live event it stands
/// for would have been.
fn inert_step(step: super::hydration::HydratedStep) -> super::hydration::HydratedStep {
    use super::hydration::HydratedStep;
    match step {
        HydratedStep::Text(text) => HydratedStep::Text(inert_owned(text)),
        HydratedStep::ToolStart { id, shown } => HydratedStep::ToolStart {
            id: inert_owned(id),
            shown: Shown {
                presentation: shown.presentation,
                target: shown.target.map(inert_owned),
            },
        },
        HydratedStep::ToolResult {
            id,
            ok,
            detail,
            outcome,
        } => HydratedStep::ToolResult {
            id: inert_owned(id),
            ok,
            detail: inert_owned(detail),
            outcome,
        },
        HydratedStep::Feedback(text) => HydratedStep::Feedback(inert_owned(text)),
        HydratedStep::Steer(text) => HydratedStep::Steer(inert_owned(text)),
    }
}

/// What the UI tells a turn that is already running.
///
/// Unbounded and drained mid-turn: an answer to a question the turn is blocked
/// on cannot be made to wait for the turn to finish, and a UI that blocked
/// while telling the runtime to stop would be a UI that cannot stop it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TurnControl {
    /// The user answered an approval request.
    ///
    /// Carries the id the question was asked under, for the same reason
    /// [`Self::QuestionAnswer`] does: the prompter takes it only when it is its
    /// own, and consumes anything else rather than leaving it for the next
    /// question to inherit as though it had been typed at that one.
    Answer {
        id: super::approval_readiness::ApprovalId,
        answer: ApprovalAnswer,
        /// The amendment typed at the draft for `answer`, if there was one.
        ///
        /// The other side's draft is discarded at submit and never travels, and
        /// neither does the one belonging to a refusal the user bailed out of
        /// with Escape ([`super::approval_amendment::Drafts::submit`]). It is
        /// **context, not authority**: nothing downstream reads it to widen,
        /// narrow or re-key a grant, and the call that runs is the call that
        /// was judged (`crate::permission::ApprovalResponse`).
        feedback: Option<String>,
    },
    /// The user answered every question of one batch, in entry order.
    ///
    /// Carries the id the batch was asked under, because this channel has no
    /// other way to tell an answer to *this* question from one the user typed at
    /// a question the turn has since given up on: the answer is applied only by
    /// a requester that is still waiting for that id, and discarded otherwise
    /// (`super::worker`, Task 5's requester).
    QuestionAnswer {
        id: super::question::QuestionId,
        answers: Vec<String>,
    },
    /// The user refused a batch. **Not a denial of anything**: the tool mints no
    /// authority, and what the model reads back is its own cancellation
    /// sentinel.
    QuestionCancelled { id: super::question::QuestionId },
    /// Stop this turn, and drop what was queued behind it. The session
    /// continues.
    ///
    /// `through` is how many submissions the UI had made when the key was
    /// pressed, and it is what keeps the second half of that sentence honest
    /// across two channels. The UI writes this message and goes on painting;
    /// the runtime may not read it for another turn's worth of time, and in
    /// that window the user can type a **new** prompt onto the work channel. A
    /// cancellation that simply emptied the queue would eat it -- the interrupt
    /// would silently swallow the question the user asked *after* deciding to
    /// stop, which is the one they most certainly meant. Work submitted through
    /// this count is dropped; anything past it is a new intention and runs.
    Cancel { through: usize },
    /// `/model`'s change of model or effort ([`ModelChange`]).
    ///
    /// **A control message rather than work**, for the reason [`Self::Reset`]
    /// is one: as work it took a place in the queue, so a model chosen while a
    /// turn ran and a prompt waited was refused as busy -- and upstream
    /// applies it at once (`setResolvedModel`, `session_commands.zig:1168-1190`)
    /// and says `Next turn will use` while a turn is still streaming. Here it
    /// costs no place and is never refused: between turns it is applied when
    /// it is read, and while work runs it is put aside -- by the turn loop, or
    /// by a prompter parked on a question, which **does not** answer the
    /// question with it (`super::approval::ControlChannel::defer`) -- and
    /// applied the moment that work has said its last word, before anything
    /// queued behind it is taken. An interrupt does not drop it: it is not a
    /// piece of work, and the turn after the interrupt is the one it was for.
    Model(ModelChange),
    /// `/undo` ([`crate::tools::mutate::undo_through`]), answered with
    /// [`UiEvent::Undone`].
    ///
    /// **A control message rather than work**, for the reason [`Self::Model`]
    /// is one: as work it would take a place in the queue and be refused as
    /// busy behind a waiting prompt, and upstream never refuses it
    /// (`commandUndoLast`, `app_commands.zig:1242-1290`, runs from the
    /// command handler whatever the worker is doing). Between turns it is done
    /// when it is read. While work runs it is put aside and done the moment
    /// that work has said its last word (`super::approval::ControlChannel::defer_undo`)
    /// -- **not at once, as upstream's is**: the files a running turn is
    /// changing are the runtime thread's to change, and an undo there,
    /// between one call's commit and the next call's read, is exactly the
    /// interleaving a single thread exists to rule out.
    ///
    /// **What it takes back is decided at the keystroke all the same.**
    /// `through` is the journal's position when the key was pressed
    /// ([`crate::tools::undo::Mark::now`], read on the UI thread), and the
    /// undo takes back the newest change at or before it -- the one upstream's
    /// would have -- whatever the turn has committed since. A later change to
    /// the same file makes it `Could not undo` rather than a restore that
    /// discards that change.
    Undo { through: crate::tools::undo::Mark },
    /// `/clear`, `/new` and `/reset`: everything [`Self::Cancel`] does, and
    /// then the conversation is dropped -- the recorder closed, the tool
    /// context's grants and read proofs with it -- so the next prompt opens a
    /// new session. The runtime says so with [`UiEvent::Fresh`].
    ///
    /// **A control message rather than work**, which is upstream's "cancel
    /// first, then apply once the worker is idle"
    /// (`decideLiveSessionTransition`, `app_session_runtime.zig:137-170`) in
    /// this channel's terms. As work it would take a place in the queue, so a
    /// reset typed behind a turn and a waiting prompt would have nowhere to go
    /// -- and a prompt typed straight after one would be refused while the
    /// cancelled turn was still unwinding. Here it is never refused and costs
    /// no place: the turn it reaches is stopped, the drop is made the moment
    /// that turn has sent its conclusion and before anything else is taken,
    /// and a prompt submitted after it -- past `through` -- runs in the new
    /// conversation.
    Reset { through: usize },
    /// Stop this turn and end the session.
    Shutdown,
}

/// One `/model` change, as the band asks for it ([`TurnControl::Model`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelChange {
    /// The model: the id itself when [`Self::announced`] is set -- a row the
    /// picker or the menu named, an id the explicit form spelled, a query the
    /// band resolved against the rows it holds
    /// (`crate::provider::model::resolve_in_order`) -- and otherwise the query
    /// as typed, which the selector resolves against the catalog it holds or
    /// loads (`super::worker`'s `change_model`).
    pub(crate) name: String,
    /// The reasoning effort chosen with it, by the effort column or the
    /// explicit form, for a model the catalog lists levels for. `None` leaves
    /// the session's effort as it is -- upstream's router form changes only
    /// the model (`setResolvedModel`), and a model with no levels takes none
    /// (`selectModelFromPicker`, `session_commands.zig:886-915`).
    pub(crate) effort: Option<crate::provider::effort::Effort>,
    /// Chosen in a picker -- the column, the menu or the explicit form --
    /// rather than typed as a query: what upstream's settings line names the
    /// change (`model picker` against `model`, `session_commands.zig:914`, `:1187`).
    pub(crate) picker: bool,
    /// The band has already said `Switched to` or `Next turn will use` for
    /// it, because it knew the id. Upstream says so the moment the change is
    /// made; a change the band could not resolve is announced by the runtime's
    /// answer instead.
    pub(crate) announced: bool,
    /// A turn was running, or waiting to, when it was asked for.
    pub(crate) next_turn: bool,
    /// The effort alone: `/settings`' `Reasoning effort` row, which keeps the
    /// model and leaves the runtime nothing to select, load or write
    /// (`selectEffortFromSettings`, `session_commands.zig:882-884`) -- the band
    /// has written `effort` itself and said so. Carried as a model change so
    /// it waits behind a running turn exactly as one does, and keeps its place
    /// among them.
    pub(crate) effort_only: bool,
}

/// What the UI asks the runtime to do next.
///
/// Capacity [`super::worker::WORK_LIMIT`] -- two -- because that is what the
/// session holds: the turn in flight, and one prompt waiting behind it. The
/// channel is that deep rather than one deep so the two numbers cannot
/// disagree: a permit is freed the instant the runtime **takes** an item, which
/// is where a turn begins rather than where it ends
/// (`super::worker::WorkHandle::outstanding`), so a one-deep channel would
/// accept or refuse a second prompt depending on whether the runtime had got
/// round to picking the first one up. Past those two the submission is refused
/// where the user can read it. The waiting one is never a surprise, because the
/// band says `queued 1` for as long as it is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TurnWork {
    /// Run a turn on this prompt.
    Submit(String),
    /// Run a turn on this prompt and the images its `[Image #N]` name, in the
    /// order they were attached.
    ///
    /// A variant of its own rather than a field on [`Self::Submit`]: a prompt
    /// with nothing attached is the case every other surface and test sends,
    /// and it stays the one-word value it always was.
    Attached {
        prompt: String,
        images: Vec<crate::image::Image>,
    },
    /// Switch to this provider: prepare, commit, reload, swap.
    ///
    /// A piece of *work* rather than a control message like `/model`'s
    /// ([`TurnControl::Model`]): it performs network I/O, it writes a file, and it replaces the
    /// configuration, the bundle and the conversation together. None of that may
    /// happen under a running turn, and none of it may happen on the thread
    /// holding the terminal.
    Setup(crate::provider::ProviderId),
    /// Load the configured provider's model catalog.
    ///
    /// Work rather than a control message because it opens a socket. The UI
    /// thread sits in `pselect(2)` holding the terminal and may not wait for a
    /// daemon that is not answering.
    Catalog,
    /// List the saved sessions a `/resume` picker offers
    /// ([`UiEvent::Sessions`]). Work because it reads the store, which the
    /// thread holding the terminal does not.
    Sessions,
    /// Make the saved session with this id the conversation
    /// ([`UiEvent::Resumed`], or [`UiEvent::ResumeFailed`]). Work rather than
    /// control because it must happen *between* turns: the picker that asks
    /// for it is refused while one runs, and the conversation it replaces is
    /// one a turn would be writing into.
    Resume(String),
    /// Give the conversation's session this title, already validated
    /// (`crate::session::validate_title`), opening the session first when no
    /// prompt has. Answered with a notice: renamed, or renamed here and not
    /// saved.
    Rename(String),
    /// Ask the configured provider what its account has left
    /// ([`crate::credits`]), answered with [`UiEvent::Credits`].
    ///
    /// Work rather than a control message because it opens a socket, which
    /// the thread holding the terminal may not wait on.
    Credits,
    /// `/compact`: compact the conversation, between turns. Work because it
    /// asks the model for notes and writes the session's log; answered with
    /// [`UiEvent::Compaction`] updates and nothing in the transcript.
    Compact,
}

/// The session's cancellation: an awaitable token and the atomic the transport
/// polls, written in a fixed order.
///
/// Cloning shares both -- there is one cancellation per session, and a clone is
/// another handle on it rather than another cancellation.
#[derive(Debug, Clone)]
pub(crate) struct Cancellation {
    /// `gateway`'s flag, handed to each turn's `TurnRequest`. Not awaitable,
    /// which is the entire reason the token below exists beside it.
    mirror: CancelToken,
    /// The session's root. Each turn awaits a child of it, so cancelling the
    /// session cancels whatever turn is running with it.
    root: CancellationToken,
}

impl Cancellation {
    pub(crate) fn new(mirror: CancelToken) -> Self {
        Self {
            mirror,
            root: CancellationToken::new(),
        }
    }

    /// A handle on the session's root, for a producer that has to await it.
    pub(crate) fn token(&self) -> CancellationToken {
        self.root.clone()
    }

    /// `gateway`'s flag, for a conversation bound before any turn has begun.
    ///
    /// The same flag every [`Cancellation::turn`] hands its turn, so a tool
    /// context made from it is stopped by the same keystroke as one
    /// `worker`'s `ready` made from a turn's.
    pub(crate) fn mirror(&self) -> CancelToken {
        self.mirror.clone()
    }

    /// Ends the session's work: the mirror first, then the token.
    pub(crate) fn cancel(&self) {
        self.cancel_between(|| {});
    }

    /// [`cancel`](Self::cancel), with [`stop`]'s seam exposed.
    fn cancel_between(&self, between: impl FnOnce()) {
        stop(&self.mirror, &self.root, between);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.root.is_cancelled()
    }

    /// The cancellation for one turn: a fresh child token, and the mirror reset.
    ///
    /// The mirror is reset before the child is taken, and the child is then
    /// asked whether it was born cancelled -- which is what a turn beginning
    /// after a shutdown is. Nothing here clears the *session's* cancellation:
    /// a cancelled root stays cancelled, and every child of it is cancelled
    /// from birth.
    pub(crate) fn turn(&self) -> TurnCancel {
        self.mirror.reset();
        let token = self.root.child_token();
        if token.is_cancelled() {
            self.mirror.cancel();
        }
        TurnCancel {
            mirror: self.mirror.clone(),
            token,
        }
    }
}

/// One turn's half of the session's cancellation.
#[derive(Debug, Clone)]
pub(crate) struct TurnCancel {
    /// Handed to the turn's `TurnRequest`, which is what the provider and the
    /// SSE decode poll.
    pub(crate) mirror: CancelToken,
    /// What this turn's producers await. A child, so the session's own
    /// cancellation reaches it, and only this turn's so that cancelling a turn
    /// does not end the session.
    pub(crate) token: CancellationToken,
}

impl TurnCancel {
    /// Stops this turn and nothing else, in the session's own order.
    pub(crate) fn cancel(&self) {
        self.cancel_between(|| {});
    }

    /// [`cancel`](Self::cancel), with [`stop`]'s seam exposed.
    fn cancel_between(&self, between: impl FnOnce()) {
        stop(&self.mirror, &self.token, between);
    }
}

/// Ends one cancellation pair, in the order the topology fixes.
///
/// **The mirror first, then the token.** A task parked in [`send_ui`] is woken
/// *by* the token and the first thing it does is stop; anything it reads on the
/// way out -- including `gateway`'s own flag, which the transport polls between
/// reads -- must already agree that this work is over.
///
/// **And the mirror again, after the token.** The leading write leaves a window
/// open: [`Cancellation::turn`] *resets* the mirror, so a turn that starts
/// inside this call -- after the mirror was set, before the token was -- would
/// take a child that is not yet cancelled, reset the flag on its way past, and
/// leave a cancelled session with a flag that says it is running. The transport
/// would keep streaming until the next event's send failed. Writing the flag
/// again after the token means every interleaving ends with the two agreeing.
///
/// `between` is how that interleaving is *demonstrated* rather than argued
/// about; production passes a closure that does nothing.
fn stop(mirror: &CancelToken, token: &CancellationToken, between: impl FnOnce()) {
    mirror.cancel();
    between();
    token.cancel();
    mirror.cancel();
}

/// Why a producer stopped.
///
/// Two reasons rather than one flag, told apart where they happen: reading the
/// token afterwards to decide would be a second look at a value that can change
/// between the two, and the caller turns them into two different `io::Error`s.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stopped {
    /// The turn was cancelled.
    Cancelled,
    /// The UI is gone -- the receiver was dropped.
    UiGone,
}

/// Hands one event to the UI, waiting for a permit, unless the turn ends first.
///
/// `biased`, so a cancelled turn stops even when there is room: the alternative
/// is a race in which a cancellation and a free permit arrive together and the
/// event wins half the time, which is a test that passes half the time.
pub(crate) async fn send_ui(
    events: &Sender<UiEvent>,
    cancel: &CancellationToken,
    event: UiEvent,
) -> Result<(), Stopped> {
    let event = event.made_inert();
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(Stopped::Cancelled),
        result = events.send(event) => result.map_err(|_| Stopped::UiGone),
    }
}

/// Hands the UI the event that ends a turn. **Not cancellable, on purpose.**
///
/// This is the acknowledgement the UI's drain loop is waiting for, and
/// cancellation is usually the reason it is being sent -- selecting it against
/// the token would abandon the send exactly when it matters and leave the UI
/// draining until its deadline. It is bounded instead by that deadline: the UI
/// drops the receiver when it expires, which resolves this send as an error
/// that there is nothing left to do about.
pub(crate) async fn send_terminal(events: &Sender<UiEvent>, event: UiEvent) {
    let _ = events.send(event.made_inert()).await;
}

/// Drives `future` to completion on the calling thread.
///
/// The two synchronous seams the turn machine owns -- `output::EventSink` and
/// `permission::ApprovalPrompter`, both defined outside this module -- have to
/// reach an async channel. `tokio::sync::mpsc::Sender::blocking_send` is not
/// the answer: called from inside a runtime it **panics** rather than applying
/// backpressure. Polling the real future here instead keeps every property the
/// awaited producer has -- cancellation wins by wakeup, order is preserved,
/// nothing is dropped, and the socket is not polled while the thread is parked,
/// which is exactly the backpressure the decode should feel.
pub(crate) fn park_on<F: Future>(future: F) -> F::Output {
    struct Unpark {
        thread: std::thread::Thread,
        /// Whether a wakeup has arrived and not yet been acted on.
        ///
        /// **Not** what saves a wakeup that lands before the park.
        /// `Thread::unpark` leaves a *token* on the thread and the next `park`
        /// consumes it and returns at once, so that case is the standard
        /// library's guarantee and holds with or without this flag -- replacing
        /// the loop below with one bare `park()` fails no test, and was run as
        /// such. What the flag is for is the other direction: `park` may return
        /// **spuriously**, having been given no token at all, and the flag is
        /// how the loop tells that apart from a real wakeup and re-parks
        /// instead of re-polling a future nothing has moved.
        woken: AtomicBool,
    }
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.woken.store(true, Ordering::Release);
            self.thread.unpark();
        }
    }

    let mut future = std::pin::pin!(future);
    let unpark = Arc::new(Unpark {
        thread: std::thread::current(),
        woken: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&unpark));
    let mut context = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        // `park` may return without a wakeup at all, so the flag rather than
        // the return is what says the future is worth polling again.
        while !unpark.woken.swap(false, Ordering::AcqRel) {
            std::thread::park();
        }
    }
}

/// `text` with every control character the terminal would obey turned into a
/// space.
///
/// `CR` and `LF` survive because they are the transcript's own vocabulary --
/// they are how a line ends, `transcript::push` splits on them, and
/// `frame::row_text` removes them from a row before it is placed. Everything
/// else in the class goes: `ESC`, which can clear the screen, retitle the
/// window, or move the cursor out of the band; `BEL`; `TAB`, whose width the
/// clip and the wrap do not agree on; and the C1 range, where a single scalar
/// is a `CSI` on a terminal that decodes it.
///
/// A space rather than a deletion, and rather than a `^[` escape, because it is
/// what `output::safe_one_line` already does everywhere else xfx quotes
/// something it did not write, and because it preserves the column count the
/// wrap measured.
pub(crate) fn inert(text: &str) -> Cow<'_, str> {
    if !text.chars().any(obeyed) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|character| if obeyed(character) { ' ' } else { character })
            .collect(),
    )
}

/// [`inert`], keeping the allocation the caller already made when there is
/// nothing to change -- which is every delta of a normal answer.
fn inert_owned(text: String) -> String {
    match inert(&text) {
        Cow::Borrowed(_) => text,
        Cow::Owned(cleaned) => cleaned,
    }
}

/// Whether the terminal would act on this character rather than draw it.
fn obeyed(character: char) -> bool {
    character.is_control() && character != '\n' && character != '\r'
}

/// `text` in pieces, none of them longer than [`DELTA_SLICE`].
///
/// Cut only where a terminal can be cut -- never inside a grapheme cluster,
/// never inside an escape sequence -- so a piece is a thing that can be written
/// on its own. A sequence that would have straddled a boundary is pushed whole
/// into the next piece, which makes that piece's predecessor shorter than the
/// ceiling rather than making anything longer than it.
///
/// One piece, borrowed, for everything that already fits: dividing is the rare
/// case and it should cost the common one nothing.
/// # The one unit that is bigger than a slice
///
/// A grapheme cluster and an escape sequence are **indivisible and have no
/// maximum size** (`super::pacer::unit_at` says why, and says that
/// `super::input`'s 32-byte cap is the keyboard's rather than the provider's).
/// So a text can begin with a single unit larger than [`DELTA_SLICE`], and
/// three things could be done about it: split it, hand it over whole as an
/// oversized piece, or make no progress at all.
///
/// **Ruled: it is handed over whole**, and the bound for that text becomes
/// `PACED_BACKLOG + max(DELTA_SLICE, that unit)`. Splitting is what this used
/// to do and it is the worst of the three: the halves of a cut sequence are two
/// fragments, and the second one is printable text nobody wrote -- the exact
/// defect the render layer was fixed for, reintroduced at the ingress. Making
/// no progress is a hang. Atomicity and progress are the two things worth more
/// than the bound here, and the corner is degenerate by construction: sixty-four
/// kilobytes of parameter bytes is not a colour any terminal will honour, and
/// nothing that is really one glyph is that long. The bound is exact for every
/// input that is not that.
pub(crate) fn slices(text: &str) -> Vec<&str> {
    if text.len() <= DELTA_SLICE {
        return vec![text];
    }
    let mut pieces = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let fits = super::pacer::cut(rest, DELTA_SLICE, super::pacer::Unfinished::Take);
        // Zero means the very first unit is longer than a whole slice. It goes
        // whole, per the ruling above -- and `unit_at` answers at least one
        // byte for any non-empty text, so the loop cannot fail to advance.
        let end = if fits == 0 {
            super::pacer::unit_at(rest, super::pacer::Unfinished::Take)
        } else {
            fits
        };
        let (piece, tail) = rest.split_at(end.clamp(1, rest.len()));
        pieces.push(piece);
        rest = tail;
    }
    pieces
}

/// The turn machine's event sink, writing to the UI's channel.
///
/// The turn calls this from a `fn`, inside the runtime, on the worker thread;
/// [`park_on`] is what makes that legal.
pub(crate) struct UiEventSink {
    events: Sender<UiEvent>,
    /// This turn's token, not the session's: a cancelled turn stops streaming
    /// into a session that is still alive.
    cancel: CancellationToken,
    /// The retry the next attempt is answering, for the records of that
    /// attempt and of the recovery ([`super::retry::Retried`]).
    retried: Option<super::retry::Retried>,
}

impl UiEventSink {
    pub(crate) fn new(events: Sender<UiEvent>, cancel: CancellationToken) -> Self {
        Self {
            events,
            cancel,
            retried: None,
        }
    }
}

/// The `UiEvent` a turn event becomes, or `None` for the ones the UI must not
/// be told twice.
///
/// `Final` and `Error` are the turn's conclusion, emitted here by
/// `agent::machine` (`machine.rs:334-345`) *and* returned from the same call as
/// a `Result` the worker turns into exactly one `UiEvent::TurnEnded`. Sending
/// one from here as well would put two conclusions on the channel, and the
/// drain loop stops at the first -- before the worker has published its session
/// log. `Final`'s text is the deltas already streamed, concatenated, so nothing
/// is lost by dropping it; `Error`'s message is `err.to_string()`, which is the
/// same string the worker's `TurnEnded { failure }` carries.
fn translate(event: &Event) -> Option<UiEvent> {
    match event {
        // An empty delta is a permit and a frame spent on nothing.
        Event::AssistantDelta { text } if text.is_empty() => None,
        Event::AssistantDelta { text } => Some(UiEvent::Delta(text.clone())),
        Event::ToolStart {
            call_id,
            tool,
            shown,
        } => Some(UiEvent::ToolStart {
            call_id: call_id.clone(),
            tool: tool.clone(),
            shown: shown.clone(),
        }),
        Event::ToolResult {
            call_id,
            tool,
            ok,
            detail,
            outcome,
            changes,
        } => Some(UiEvent::ToolResult {
            call_id: call_id.clone(),
            tool: tool.clone(),
            ok: *ok,
            detail: detail.clone(),
            outcome: *outcome,
            changes: *changes,
        }),
        // Carried through, unlike `Final` and `Error`: nothing else on this
        // channel says it, and the worker's `TurnEnded` is about the turn
        // rather than about what the user said inside it.
        Event::ToolFeedback { call_id, text } => Some(UiEvent::ToolFeedback {
            call_id: call_id.clone(),
            text: text.clone(),
        }),
        Event::Final { .. } | Event::Error { .. } => None,
    }
}

impl UiEventSink {
    /// Hands one event over, waiting for a permit.
    fn send(&mut self, event: UiEvent) -> io::Result<()> {
        match park_on(send_ui(&self.events, &self.cancel, event)) {
            Ok(()) => Ok(()),
            // The two the turn machine already knows how to report: it takes
            // the sink's error as the turn's own (`machine.rs:641-648`), so the
            // kind is what a reader will be shown. An interrupted turn was
            // stopped on purpose; a broken pipe is a session that ended under
            // it.
            Err(Stopped::Cancelled) => Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "the turn was cancelled",
            )),
            Err(Stopped::UiGone) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the terminal session is gone",
            )),
        }
    }
}

impl EventSink for UiEventSink {
    fn emit(&mut self, event: &Event) -> io::Result<()> {
        let Some(event) = translate(event) else {
            return Ok(());
        };
        match event {
            // The one event whose text is a provider's to size, and therefore
            // the one that gets a size here ([`DELTA_SLICE`]). Each piece waits
            // for its own permit, so a provider that answers in a single frame
            // feels the same backpressure as one that streams -- and the UI's
            // queue stays inside a number rather than inside a hope.
            UiEvent::Delta(text) if text.len() > DELTA_SLICE => {
                for piece in slices(&text) {
                    self.send(UiEvent::Delta(piece.to_string()))?;
                }
                Ok(())
            }
            event => self.send(event),
        }
    }

    /// The row that counts down to the next attempt, told as the event it is
    /// drawn from.
    ///
    /// Through the same permit every other event waits for, and its failure
    /// dropped: a turn that was cancelled or a UI that is gone is told so by
    /// the wait and the next emit, and this hook changes nothing about either
    /// ([`EventSink::retrying`]).
    ///
    /// And the full transcript's `* recovery:` record of it, through the same
    /// path as every other full-detail record ([`UiEvent::FullDetail`]), as
    /// upstream records each recovery it shows (`recordRouteRecoveryNotice`,
    /// `app_worker_runtime.zig:92-133`).
    fn retrying(
        &mut self,
        error: &crate::gateway::ProviderError,
        attempt: u32,
        limit: u32,
        delay: std::time::Duration,
    ) {
        let _ = self.send(UiEvent::Retrying {
            cause: super::retry::cause(error),
            diagnostic: super::retry::diagnostic(error),
            delay,
        });
        let retried = super::retry::Retried::of(error, delay);
        let _ = self.send(UiEvent::FullDetail(retried.record(attempt, limit)));
        self.retried = Some(retried);
    }

    /// The record of the attempt the retry above was waiting for, as it
    /// starts: upstream's second `! recovery:` for the same retry.
    fn retry_started(&mut self, attempt: u32, limit: u32) {
        if let Some(retried) = &self.retried {
            let record = retried.record(attempt, limit);
            let _ = self.send(UiEvent::FullDetail(record));
        }
    }

    /// Which of the band's steers the turn took.
    fn steered(&mut self, steers: &[crate::agent::Steer]) {
        let _ = self.send(UiEvent::Steered {
            ids: steers.iter().map(|steer| steer.id).collect(),
        });
    }

    /// The row that says an attempt got through after a retry, and its
    /// `✓ recovery:` record, in upstream's order: the status is set, then
    /// recorded (`app_worker_runtime.zig:1121-1129`).
    fn recovered(&mut self, attempt: u32, limit: u32) {
        if self.retried.take().is_some() {
            let _ = self.send(UiEvent::Recovered { attempt });
            let _ = self.send(UiEvent::FullDetail(super::retry::recovered(attempt, limit)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io;
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    use tokio::sync::mpsc::Receiver;
    use tokio_util::sync::CancellationToken;

    /// The next event, or a failure. Never a suite that does not end: a change
    /// that stops sending must be a red test rather than a hang, which is what
    /// an unbounded `recv().await` on a channel whose sender is still alive
    /// would be.
    async fn next(events: &mut Receiver<UiEvent>) -> Option<UiEvent> {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("the event never arrived")
    }

    use crate::gateway::CancelToken;
    use crate::output::{Event, EventSink};
    use crate::tui::frame::Band;
    use crate::tui::layout;
    use crate::tui::transcript::Transcript;

    #[test]
    fn park_on_returns_a_ready_value_without_a_runtime() {
        assert_eq!(park_on(async { 7 }), 7);
    }

    #[test]
    fn park_on_wakes_when_another_thread_completes_the_future() {
        let (tx, rx) = tokio::sync::oneshot::channel::<u8>();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            tx.send(9).expect("the receiver is parked");
        });
        assert_eq!(park_on(rx).expect("the sender ran"), 9);
        sender.join().expect("the sender thread");
    }

    #[test]
    fn park_on_does_not_sleep_through_a_wakeup_that_arrived_before_the_park() {
        // The wake happens *inside* the poll, on this very thread, before the
        // park is reached -- the hardest ordering for the executor and the one
        // a wrongly wired waker loses. It is `Thread::unpark`'s token that
        // carries it, not the `woken` flag (deleting the flag's loop fails
        // nothing; handing `park_on` a `Waker::noop` fails this), so what is
        // pinned here is that the future's own waker really is the one this
        // thread parks on. Run on a thread of its own with a bounded wait, so
        // the failure is a failure rather than a suite that never ends.
        struct WakesItselfOnce {
            polled: bool,
        }
        impl Future for WakesItselfOnce {
            type Output = u8;
            fn poll(mut self: std::pin::Pin<&mut Self>, context: &mut Context<'_>) -> Poll<u8> {
                if self.polled {
                    return Poll::Ready(5);
                }
                self.polled = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
        }

        let (done, parked) = std_mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(park_on(WakesItselfOnce { polled: false }));
        });
        assert_eq!(
            parked.recv_timeout(Duration::from_secs(5)),
            Ok(5),
            "the wakeup that arrived before the park was slept through"
        );
    }

    #[test]
    fn a_retry_is_the_rows_event_and_then_the_full_transcripts_recovery_record() {
        // The row counts down on the band; the record is what the full
        // transcript keeps of it (`recordRouteRecoveryNotice`,
        // `app_worker_runtime.zig:92-133`), sent through the same path as F5's
        // other records.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        let error = crate::gateway::ProviderError::Status {
            subject: crate::gateway::GATEWAY_SUBJECT,
            status: 503,
            body: String::new(),
            retryable: true,
            retry_after: None,
        };
        sink.retrying(&error, 1, 3, Duration::from_secs(1));
        drop(sink);
        assert!(matches!(
            rx.blocking_recv(),
            Some(UiEvent::Retrying {
                cause: "Provider unavailable",
                ..
            })
        ));
        let Some(UiEvent::FullDetail(record)) = rx.blocking_recv() else {
            panic!("no recovery record followed the row");
        };
        assert_eq!(record.topic, "recovery");
        assert_eq!(
            record.body,
            "\u{26a0} Provider unavailable \u{b7} HTTP 503 \u{b7} retrying request in 1s\n\
             kind: auto_retry \u{b7} cause: provider_unavailable \u{b7} attempt: 1/3 \u{b7} \
             retry delay: 1s \u{b7} diagnostic: HTTP 503"
        );
        assert_eq!(rx.blocking_recv(), None);
    }

    #[test]
    fn the_attempt_after_a_retry_and_its_recovery_are_recorded_too() {
        // Upstream's sequence (`fx-interrupt-recovery-80x120-full`): the
        // retry's record, the next attempt's as it starts -- same cause, delay
        // and diagnostic, its own number -- then `✓ recovery:`. A turn that
        // never retried records neither.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        sink.retry_started(2, 3);
        sink.recovered(2, 3);
        let error = crate::gateway::ProviderError::Status {
            subject: crate::gateway::GATEWAY_SUBJECT,
            status: 500,
            body: String::new(),
            retryable: true,
            retry_after: None,
        };
        sink.retrying(&error, 1, 3, Duration::from_millis(250));
        sink.retry_started(2, 3);
        sink.recovered(2, 3);
        drop(sink);
        let mut records = Vec::new();
        let mut recovered = Vec::new();
        while let Some(event) = rx.blocking_recv() {
            match event {
                UiEvent::FullDetail(record) => records.push(record.body),
                // The row's own event, once, for the retried step only.
                UiEvent::Recovered { attempt } => recovered.push(attempt),
                _ => {}
            }
        }
        assert_eq!(recovered, [2]);
        assert_eq!(
            records,
            [
                "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} retrying request\n\
                 kind: auto_retry \u{b7} cause: provider_unavailable \u{b7} attempt: 1/3 \u{b7} \
                 diagnostic: HTTP 500",
                "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} retrying request\n\
                 kind: auto_retry \u{b7} cause: provider_unavailable \u{b7} attempt: 2/3 \u{b7} \
                 diagnostic: HTTP 500",
                "\u{2713} recovered \u{b7} succeeded on attempt 2\nkind: auto_recovered \u{b7} \
                 attempt: 2/3",
            ]
        );
    }

    #[test]
    fn an_empty_delta_costs_the_ui_nothing() {
        // A permit and a frame spent on no text. The turn machine emits one
        // whenever a step decoded nothing.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        sink.emit(&Event::AssistantDelta {
            text: String::new(),
        })
        .expect("accepted");
        drop(sink);
        assert_eq!(rx.blocking_recv(), None, "an empty delta reached the UI");
    }

    #[tokio::test]
    async fn a_cancelled_turn_stops_sending_even_when_there_is_room() {
        // `biased` makes cancellation win a tie.
        let (tx, _rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(send_ui(&tx, &cancel, UiEvent::Delta("x".into()))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_full_channel_parks_the_producer_and_drops_nothing() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let cancel = CancellationToken::new();
        send_ui(&tx, &cancel, UiEvent::Delta("a".into()))
            .await
            .expect("room");
        send_ui(&tx, &cancel, UiEvent::Delta("b".into()))
            .await
            .expect("room");
        let parked = tokio::spawn({
            let tx = tx.clone();
            let cancel = cancel.clone();
            async move { send_ui(&tx, &cancel, UiEvent::Delta("c".into())).await }
        });
        assert_eq!(next(&mut rx).await, Some(UiEvent::Delta("a".into())));
        parked
            .await
            .expect("the task")
            .expect("the send completed once a permit freed");
        assert_eq!(next(&mut rx).await, Some(UiEvent::Delta("b".into())));
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::Delta("c".into())),
            "order was not preserved"
        );
    }

    #[tokio::test]
    async fn the_terminal_event_is_sent_even_though_the_turn_was_cancelled() {
        // The acknowledgement the UI's drain loop waits for. Selecting it
        // against the token would be circular: cancellation is usually the
        // reason it is being sent.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let cancel = CancellationToken::new();
        cancel.cancel();
        send_terminal(&tx, UiEvent::TurnEnded { failure: None }).await;
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::TurnEnded { failure: None })
        );
    }

    #[test]
    fn cancellation_sets_the_mirror_before_the_token() {
        // A waiter woken by the token immediately reads the mirror, so the
        // flag must already say "cancelled" when the wakeup lands.
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());
        let root = cancellation.token();
        let (seen, observed) = std_mpsc::channel();
        let watcher = std::thread::spawn(move || {
            park_on(root.cancelled());
            seen.send(mirror.is_cancelled())
                .expect("the test is listening");
        });
        cancellation.cancel();
        assert!(
            observed.recv().expect("the watcher woke"),
            "the mirror still said running"
        );
        watcher.join().expect("the watcher thread");
    }

    #[test]
    fn the_sink_sends_a_delta_from_a_synchronous_callback() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        sink.emit(&Event::AssistantDelta { text: "hi".into() })
            .expect("sent");
        assert_eq!(rx.blocking_recv(), Some(UiEvent::Delta("hi".into())));
    }

    #[test]
    fn an_answer_that_arrived_whole_is_handed_over_in_pieces_that_each_wait() {
        // The finite bound, at the door it has to be made finite at. The UI
        // refuses to take the *next* event past its mark, which bounds its
        // queue to that mark plus whatever one event carried -- so a provider
        // that answers in a single frame would put the whole answer in the UI's
        // hands in one go and the mark would mean nothing. The size is settled
        // here instead, and each piece waits for its own permit like every
        // other event.
        //
        // A channel one deep is what makes "waits" a fact rather than a claim:
        // three pieces cannot all be in flight, so the only way all three
        // arrive is one permit at a time.
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let text = "y".repeat(DELTA_SLICE * 2 + 7);
        let expected = text.clone();
        let sender = std::thread::spawn(move || {
            let mut sink = UiEventSink::new(tx, CancellationToken::new());
            sink.emit(&Event::AssistantDelta { text }).expect("sent");
        });

        let mut seen = String::new();
        let mut pieces = 0usize;
        while let Some(event) = rx.blocking_recv() {
            let UiEvent::Delta(piece) = event else {
                panic!("a delta arrived as something else");
            };
            assert!(
                piece.len() <= DELTA_SLICE,
                "a piece was {} bytes, past the ceiling",
                piece.len()
            );
            seen.push_str(&piece);
            pieces += 1;
        }
        sender.join().expect("the sending thread");

        assert_eq!(pieces, 3, "the answer was not divided as expected");
        assert_eq!(seen, expected, "dividing the answer changed it");
    }

    #[test]
    fn a_sequence_that_would_straddle_a_boundary_goes_whole_into_the_next_piece() {
        // Dividing may not cut one of the terminal's own units in half. A
        // sequence split across two events is two fragments: the first ends a
        // piece with half a `CSI` in it and the second begins with the
        // printable tail of one. The boundary moves rather than the sequence.
        let colour = "\u{1b}[38;5;200m";
        // Put the sequence across the ceiling: two bytes of it before, the rest
        // after.
        let head = "a".repeat(DELTA_SLICE - 2);
        let text = format!("{head}{colour}{}", "b".repeat(16));
        let pieces = slices(&text);

        assert!(pieces.len() > 1, "the case did not divide anything");
        assert_eq!(pieces.concat(), text, "dividing changed the text");
        for piece in &pieces {
            assert!(piece.len() <= DELTA_SLICE, "a piece was past the ceiling");
        }
        assert_eq!(
            pieces.iter().filter(|piece| piece.contains(colour)).count(),
            1,
            "the colour is not whole inside exactly one piece: {pieces:?}"
        );
        // and it is the *second* one that has it, because the first stopped
        // short rather than cutting it
        assert_eq!(pieces[0], head);
    }

    #[test]
    fn a_glyph_is_never_divided_either() {
        // The same rule for the other unit. A cut inside a cluster is a
        // different glyph or none, and a cut inside a scalar is not text at
        // all.
        let text = "\u{c548}".repeat(DELTA_SLICE);
        let pieces = slices(&text);
        assert!(pieces.len() > 1, "the case did not divide anything");
        assert_eq!(pieces.concat(), text);
        for piece in &pieces {
            assert!(piece.len() <= DELTA_SLICE);
            assert_eq!(
                piece.len() % 3,
                0,
                "a three-byte glyph was cut: {} bytes",
                piece.len()
            );
        }
    }

    #[test]
    fn a_single_unit_bigger_than_a_slice_goes_whole_rather_than_being_cut() {
        // The corner the bound cannot cover, ruled rather than left to
        // whichever branch happened to run. Neither a grapheme cluster nor an
        // escape sequence has a maximum size, so a text can begin with one
        // indivisible unit larger than the whole ceiling. Splitting it is the
        // worst answer -- the second half of a cut sequence is printable text
        // nobody wrote, which is the defect the render layer exists to prevent,
        // reintroduced here -- and making no progress is a hang. It goes whole.
        //
        // Both kinds, because "indivisible" has two meanings here.
        let escape = format!("\u{1b}[{}m", "1;".repeat(DELTA_SLICE));
        let cluster = format!("a{}", "\u{301}".repeat(DELTA_SLICE));
        for (what, unit) in [("an escape sequence", escape), ("a cluster", cluster)] {
            let tail = "b".repeat(64);
            let text = format!("{unit}{tail}");
            assert!(unit.len() > DELTA_SLICE, "{what} was not oversized");

            let pieces = slices(&text);

            assert_eq!(pieces.concat(), text, "{what} was changed by dividing");
            assert_eq!(pieces[0], unit, "{what} was cut in half");
            assert_eq!(
                pieces.len(),
                2,
                "{what} did not leave the rest in one piece: {} pieces",
                pieces.len()
            );
            // and the documented bound still holds for everything that is not
            // the oversized unit itself
            for piece in &pieces[1..] {
                assert!(piece.len() <= DELTA_SLICE, "{what}: a later piece is over");
            }
        }
    }

    #[test]
    fn dividing_always_makes_progress() {
        // The hang the ruling above rules out, as a property rather than as two
        // examples: whatever the text, every piece has something in it and the
        // pieces put the text back together. A loop that could answer "nothing
        // fits" would spin here instead of failing.
        let big = DELTA_SLICE + 1;
        for text in [
            "\u{1b}[".to_string() + &"9".repeat(big),
            "\u{1b}]0;".to_string() + &"t".repeat(big),
            "\u{c548}".repeat(big),
            "x".repeat(big),
            format!("{}\u{1b}[31m{}", "a".repeat(big), "b".repeat(big)),
        ] {
            let pieces = slices(&text);
            assert!(!pieces.is_empty());
            assert!(pieces.iter().all(|piece| !piece.is_empty()));
            assert_eq!(pieces.concat(), text);
        }
    }

    #[test]
    fn an_answer_that_already_fits_is_not_divided_or_copied() {
        // The common case pays nothing: one piece, borrowed from the text.
        let text = "an ordinary delta";
        let pieces = slices(text);
        assert_eq!(pieces, vec![text]);
        assert!(std::ptr::eq(pieces[0], text));
    }

    #[test]
    fn the_sink_reports_a_cancelled_turn_as_an_interruption() {
        let (tx, _rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut sink = UiEventSink::new(tx, cancel);
        let err = sink
            .emit(&Event::AssistantDelta { text: "hi".into() })
            .expect_err("a cancelled turn kept streaming");
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn the_sink_reports_a_gone_ui_as_a_broken_pipe() {
        // The other half of the same seam: a turn that is still live writing
        // to a session that is not. `machine.rs:646` reports the sink's error
        // as the turn's own, so the two have to be told apart at the source.
        let (tx, rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        drop(rx);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        let err = sink
            .emit(&Event::AssistantDelta { text: "hi".into() })
            .expect_err("the sink wrote into a closed channel");
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn the_turn_machines_own_terminal_events_are_not_ui_terminal_events() {
        // `Final`/`Error` are the turn's conclusion emitted through the sink
        // (`machine.rs:334-345`), and the worker sends exactly one `TurnEnded`
        // derived from the very same `Result`. Translating them here would put
        // a second conclusion on the channel, and the UI's drain loop stops at
        // the first terminal event -- before the worker published its log.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        sink.emit(&Event::Final {
            output: "the answer".into(),
        })
        .expect("accepted");
        sink.emit(&Event::Error {
            message: "it failed".into(),
        })
        .expect("accepted");
        drop(sink);
        assert_eq!(
            rx.blocking_recv(),
            None,
            "a conclusion reached the UI twice"
        );
    }

    #[test]
    fn only_the_two_terminal_events_end_a_turn() {
        assert!(UiEvent::TurnEnded { failure: None }.is_terminal());
        assert!(UiEvent::Fatal("gone".into()).is_terminal());
        for event in [
            UiEvent::Delta("d".into()),
            UiEvent::ToolStart {
                call_id: "1".into(),
                tool: "read".into(),
                shown: crate::tools::read::READ_FILE.shown(&serde_json::json!({})),
            },
            UiEvent::ToolResult {
                call_id: "1".into(),
                tool: "read".into(),
                ok: true,
                detail: "done".into(),
                outcome: Outcome::Finished,
                changes: None,
            },
            UiEvent::Notice(super::super::notice::Notice::neutral("", "n")),
            // A turn beginning is the *opposite* of one ending, and a drain
            // loop that stopped at it would leave before the answer.
            UiEvent::TurnStarted,
        ] {
            assert!(!event.is_terminal(), "{event:?} ended the turn");
        }
    }

    #[test]
    fn a_new_turn_gets_a_reset_mirror_and_a_child_that_dies_with_the_root() {
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());

        let first = cancellation.turn();
        first.cancel();
        assert!(mirror.is_cancelled(), "the turn's mirror is the session's");
        assert!(first.token.is_cancelled());
        assert!(
            !cancellation.is_cancelled(),
            "cancelling one turn ended the session"
        );

        let second = cancellation.turn();
        assert!(
            !second.mirror.is_cancelled(),
            "the new turn started cancelled"
        );
        assert!(!second.token.is_cancelled());

        cancellation.cancel();
        assert!(second.token.is_cancelled(), "the child outlived its root");
        assert!(second.mirror.is_cancelled());
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn the_mirror_is_already_set_when_the_token_is_cancelled() {
        // The same order, held still. The watcher above observes it from the
        // outside and can only see the window if it is scheduled inside it;
        // this one stands in the window.
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());
        let root = cancellation.token();
        let mut inside = None;
        cancellation.cancel_between(|| inside = Some((mirror.is_cancelled(), root.is_cancelled())));
        assert_eq!(
            inside,
            Some((true, false)),
            "the token was cancelled before the flag it is read with"
        );

        // A turn's own cancellation is the same pair and the same order: Task
        // 12's Ctrl-C stops a turn while the session keeps running.
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());
        let turn = cancellation.turn();
        let mut inside = None;
        turn.cancel_between(|| {
            inside = Some((turn.mirror.is_cancelled(), turn.token.is_cancelled()))
        });
        assert_eq!(
            inside,
            Some((true, false)),
            "a turn cancelled its token before its flag"
        );
        assert!(turn.mirror.is_cancelled());
        assert!(turn.token.is_cancelled());
    }

    #[test]
    fn a_turn_that_starts_while_the_session_is_ending_does_not_clear_the_mirror() {
        // The interleaving `cancel` is written around: the mirror is set, the
        // turn begins and resets it, and only then is the token cancelled. Held
        // still by the seam instead of being hoped for on a busy machine.
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());
        let mut turn = None;
        cancellation.cancel_between(|| turn = Some(cancellation.turn()));

        let turn = turn.expect("the turn started");
        assert!(
            mirror.is_cancelled(),
            "the session was cancelled and its flag said running"
        );
        assert!(turn.mirror.is_cancelled());
        assert!(turn.token.is_cancelled(), "the child outlived its root");
    }

    #[test]
    fn a_turn_taken_after_the_session_ended_is_born_cancelled() {
        let mirror = CancelToken::new();
        let cancellation = Cancellation::new(mirror.clone());
        cancellation.cancel();

        let turn = cancellation.turn();
        assert!(turn.token.is_cancelled());
        assert!(
            turn.mirror.is_cancelled(),
            "the reset outlived the cancellation"
        );
    }

    #[test]
    fn controls_the_terminal_would_obey_never_reach_the_ui() {
        // The policy, at the one seam every byte of foreign text crosses.
        // Line breaks survive because they are the transcript's own vocabulary
        // and `frame::row_text` strips them from a row; everything else --
        // `ESC`, `TAB`, the C1 range -- becomes a space.
        assert_eq!(inert("plain"), "plain");
        assert_eq!(inert("\x1b[2Jgone"), " [2Jgone");
        assert_eq!(inert("a\tb"), "a b");
        assert_eq!(inert("a\u{9b}[2Jb"), "a [2Jb", "C1 CSI survived");
        assert_eq!(inert("one\r\ntwo\n"), "one\r\ntwo\n");
    }

    #[tokio::test]
    async fn the_send_makes_every_event_inert_whoever_built_it() {
        // The guarantee is structural rather than a rule producers remember:
        // the two sends are the only ways an event reaches the UI, and every
        // variant that carries text is checked here so that a variant added
        // later cannot quietly skip the policy.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let cancel = CancellationToken::new();

        for event in [
            UiEvent::Delta("\x1b[2Jd".into()),
            UiEvent::ToolStart {
                call_id: "\x1b[2Jc".into(),
                tool: "\x1b[2Jt".into(),
                // The model's own argument, which is what the row will quote.
                shown: crate::tools::terminal::TERMINAL
                    .shown(&serde_json::json!({"action": "exec", "command": "\x1b[2Jls"})),
            },
            UiEvent::ToolResult {
                call_id: "\x1b[2Jc".into(),
                tool: "\x1b[2Jt".into(),
                ok: false,
                detail: "\x1b[2Jd".into(),
                outcome: Outcome::Denied,
                changes: None,
            },
            UiEvent::Notice(super::super::notice::Notice::warning(
                "\x1b[2Jtopic",
                "\x1b]0;retitled\x07",
            )),
            UiEvent::Approval(super::super::approval::ApprovalAsked {
                id: super::super::approval_readiness::ApprovalId(4),
                request: ApprovalRequest {
                    tool: "write",
                    target: "\x1b[2Jsrc/main.rs".into(),
                    summary: "write \x1b[2Jsomething".into(),
                    always_scope: "\x1b[2Jsrc".into(),
                    // Both sides of the diff, because they are the largest
                    // quotation of a file this product ever carries and the
                    // policy has to hold for a payload built by a caller that
                    // did not escape it -- the permission boundary's own
                    // bounding is the first seam, and this is the second.
                    diff: Some(ApprovalDiff {
                        before: "\x1b[2Jold".into(),
                        after: "\x1b]0;new\x07".into(),
                        first_line: 1,
                        lines_below: 0,
                    }),
                    subject: ApprovalSubject::Command {
                        command: "\x1b[2Jrm".into(),
                        cwd: "\x1b]0;.\x07".into(),
                    },
                },
            }),
        ] {
            send_ui(&tx, &cancel, event).await.expect("room");
        }

        assert_eq!(next(&mut rx).await, Some(UiEvent::Delta(" [2Jd".into())));
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::ToolStart {
                call_id: " [2Jc".into(),
                tool: " [2Jt".into(),
                shown: Shown {
                    presentation: crate::tools::terminal::TERMINAL.presentation(),
                    target: Some(" [2Jls".into()),
                },
            })
        );
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::ToolResult {
                call_id: " [2Jc".into(),
                tool: " [2Jt".into(),
                ok: false,
                detail: " [2Jd".into(),
                outcome: Outcome::Denied,
                changes: None,
            })
        );
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::Notice(super::super::notice::Notice::warning(
                " [2Jtopic",
                " ]0;retitled ",
            )))
        );
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::Approval(super::super::approval::ApprovalAsked {
                // The id is a number this crate minted, so it crosses the seam
                // as it was: there is nothing in it a terminal can be made to
                // obey, and an id changed in flight would address the answer to
                // a question nobody asked.
                id: super::super::approval_readiness::ApprovalId(4),
                request: ApprovalRequest {
                    tool: "write",
                    target: " [2Jsrc/main.rs".into(),
                    summary: "write  [2Jsomething".into(),
                    always_scope: " [2Jsrc".into(),
                    diff: Some(ApprovalDiff {
                        before: " [2Jold".into(),
                        after: " ]0;new ".into(),
                        first_line: 1,
                        lines_below: 0,
                    }),
                    subject: ApprovalSubject::Command {
                        command: " [2Jrm".into(),
                        cwd: " ]0;. ".into(),
                    },
                },
            })),
            "an approval quotes a file, which is where an escape would be"
        );

        // The uncancellable send is held to the same policy, and it is the one
        // that carries a failure message xfx did not write.
        send_terminal(
            &tx,
            UiEvent::TurnEnded {
                failure: Some("\x1b[2Jbroke".into()),
            },
        )
        .await;
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::TurnEnded {
                failure: Some(" [2Jbroke".into())
            })
        );
        send_terminal(&tx, UiEvent::Fatal("\x1b[2Jpanicked".into())).await;
        assert_eq!(
            next(&mut rx).await,
            Some(UiEvent::Fatal(" [2Jpanicked".into()))
        );
    }

    #[test]
    fn an_injected_escape_sequence_reaches_the_document_inert() {
        // The composed proof the control policy is really about: answer text
        // carrying `ESC [ 2 J` travels the whole path a delta travels -- sink,
        // channel, transcript, band -- and what would be written to the
        // terminal contains no clear-screen.
        let (tx, mut rx) = tokio::sync::mpsc::channel(UI_EVENTS);
        let mut sink = UiEventSink::new(tx, CancellationToken::new());
        sink.emit(&Event::AssistantDelta {
            text: "\x1b[2Jgone".into(),
        })
        .expect("sent");
        let Some(UiEvent::Delta(text)) = rx.blocking_recv() else {
            panic!("the delta never arrived");
        };

        let geometry = layout::solve(24, 80, 1).expect("a band");
        let mut transcript = Transcript::new(geometry.cols);
        let append = transcript.push(&text);
        let mut band = Band::new();
        let mut out = Vec::new();
        band.append_document(&mut out, append.scroll, &append.rows, &geometry)
            .expect("a vector accepts every write");

        assert!(
            !out.windows(4).any(|window| window == b"\x1b[2J"),
            "a clear-screen reached the terminal"
        );
        assert!(
            out.windows(8).any(|window| window == b" [2Jgone"),
            "the text itself did not land: {}",
            String::from_utf8_lossy(&out)
        );
    }
    /// A catalog row carrying an escape sequence in every string it has.
    fn hostile_entry() -> crate::provider::model::CatalogEntry {
        crate::provider::model::CatalogEntry {
            id: "id\u{1b}[2J".to_string(),
            aliases: vec!["alias\u{1b}]0;pwned\u{7}".to_string()],
            name: Some("name\u{1b}[H".to_string()),
            efforts: vec!["high\u{1b}[3J".to_string()],
            max_context: Some(200_000),
            facts: crate::provider::model::ModelFacts {
                owned_by: Some("vendor\u{1b}[2J".to_string()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn catalog_rows_are_made_inert_in_every_string_they_carry() {
        // A catalog is a document a daemon on a port serves. Its ids, its
        // display names and its effort labels are all about to become rows on a
        // terminal, so all of them are exactly as foreign as a delta -- and the
        // policy is applied at the channel rather than at the painter, so a
        // later reader of this event inherits it.
        let event = UiEvent::CatalogLoaded {
            provider: crate::provider::ProviderId::Llmux,
            entries: vec![hostile_entry()],
        }
        .made_inert();
        let UiEvent::CatalogLoaded { entries, .. } = event else {
            panic!("the variant changed");
        };
        let entry = &entries[0];
        for text in std::iter::once(&entry.id)
            .chain(entry.aliases.iter())
            .chain(entry.name.iter())
            .chain(entry.efforts.iter())
            .chain(entry.facts.owned_by.iter())
        {
            assert!(!text.contains('\u{1b}'), "{text:?} still carries an escape");
            assert!(
                !text.chars().any(char::is_control),
                "{text:?} still carries a control character"
            );
        }
        assert_eq!(entry.max_context, Some(200_000), "a number is not text");
    }

    #[test]
    fn every_string_a_question_batch_carries_is_made_inert() {
        // The payload really is encoded before it gets here
        // (`crate::tools::question::terminal_safe`, run inside `parse`), so this
        // arm is a belt rather than the braces. It is asserted anyway because
        // `made_inert` is **total**: this type is constructible anywhere in the
        // crate, and a variant that named no text would exempt every later
        // producer of one from a policy the channel is supposed to guarantee.
        // Every string of every option, because a row is what each of them
        // becomes.
        let hostile = "a\u{1b}[2Jb".to_string();
        let event = UiEvent::Question(crate::tui::question::QuestionRequest {
            id: crate::tui::question::QuestionId(7),
            entries: vec![crate::tools::question::QuestionEntry {
                question: hostile.clone(),
                options: vec![
                    crate::tools::question::QuestionOption {
                        label: hostile.clone(),
                        description: Some("d\u{1b}]0;pwned\u{7}".to_string()),
                    },
                    crate::tools::question::QuestionOption {
                        label: "plain".to_string(),
                        description: None,
                    },
                ],
            }],
        })
        .made_inert();
        let UiEvent::Question(request) = event else {
            panic!("the variant changed");
        };
        assert_eq!(
            request.id,
            crate::tui::question::QuestionId(7),
            "the id is a number, not text"
        );
        let entry = &request.entries[0];
        for text in std::iter::once(&entry.question)
            .chain(entry.options.iter().map(|option| &option.label))
            .chain(
                entry
                    .options
                    .iter()
                    .filter_map(|option| option.description.as_ref()),
            )
        {
            assert!(!text.contains('\u{1b}'), "{text:?} still carries an escape");
            assert!(
                !text.chars().any(char::is_control),
                "{text:?} still carries a control character"
            );
        }
        assert_eq!(
            entry.options[1].description, None,
            "a missing half stayed missing"
        );
    }

    #[test]
    fn a_question_batch_is_not_a_terminal_event() {
        // A question is asked *inside* a turn, and the drain leaves on exactly
        // the two events a turn cannot continue past. One that ended the drain
        // would leave while the turn was still waiting to be answered.
        assert!(!UiEvent::Question(crate::tui::question::QuestionRequest {
            id: crate::tui::question::QuestionId(1),
            entries: Vec::new(),
        })
        .is_terminal());
    }

    #[test]
    fn a_selected_providers_model_is_made_inert_and_its_two_facts_are_not_text() {
        let event = UiEvent::ProviderSelected {
            provider: crate::provider::ProviderId::Llmux,
            model: "model\u{1b}[2J".to_string(),
            missing_credential: true,
        }
        .made_inert();
        let UiEvent::ProviderSelected {
            model,
            missing_credential,
            provider,
        } = event
        else {
            panic!("the variant changed");
        };
        assert!(!model.contains('\u{1b}'), "{model:?}");
        assert!(missing_credential, "the fact survived the sanitizing");
        assert_eq!(provider, crate::provider::ProviderId::Llmux);
    }

    #[test]
    fn every_string_a_model_answer_carries_is_made_inert() {
        // A model id comes off a settings file, a daemon's catalog or the
        // composer, and both of the sentences beside it quote it back -- so an
        // answer that let one through would paint an escape sequence onto a
        // document row, having been built by xfx's own selector rather than by
        // a model, which is exactly the producer a reader stops suspecting.
        let hostile = "model\u{1b}[2J".to_string();
        let UiEvent::ModelAnswered {
            model,
            effort,
            outcome,
            next_turn,
            announced,
        } = (UiEvent::ModelAnswered {
            model: hostile.clone(),
            effort: crate::provider::effort::Effort::Named(hostile.clone()),
            outcome: ModelAnswer::Applied {
                unverified: Some(hostile.clone()),
            },
            next_turn: true,
            announced: true,
        })
        .made_inert()
        else {
            panic!("the variant changed");
        };
        assert!(!model.contains('\u{1b}'), "{model:?}");
        assert!(!effort.display().contains('\u{1b}'), "{effort:?}");
        assert!(next_turn, "a fact, not text, and carried through");
        assert!(announced, "a fact, not text, and carried through");
        let ModelAnswer::Applied { unverified } = outcome else {
            panic!("the answer changed");
        };
        let unverified = unverified.expect("the caveat survived");
        assert!(!unverified.contains('\u{1b}'), "{unverified:?}");

        // And the refusal, which is the arm that quotes the id back by name.
        let UiEvent::ModelAnswered { outcome, .. } = (UiEvent::ModelAnswered {
            model: "plain".to_string(),
            effort: crate::provider::effort::Effort::Auto,
            outcome: ModelAnswer::Refused {
                reason: hostile.clone(),
            },
            next_turn: false,
            announced: false,
        })
        .made_inert() else {
            panic!("the variant changed");
        };
        let ModelAnswer::Refused { reason } = outcome else {
            panic!("the answer changed");
        };
        assert!(!reason.contains('\u{1b}'), "{reason:?}");

        // The answer with nothing to say is carried through unchanged.
        let UiEvent::ModelAnswered { outcome, .. } = (UiEvent::ModelAnswered {
            model: "plain".to_string(),
            effort: crate::provider::effort::Effort::Auto,
            outcome: ModelAnswer::Unchanged,
            next_turn: false,
            announced: false,
        })
        .made_inert() else {
            panic!("the variant changed");
        };
        assert_eq!(outcome, ModelAnswer::Unchanged);
    }

    #[test]
    fn a_model_answer_is_not_a_terminal_event() {
        // `/model` is not a turn and owes no conclusion; a drain that left on
        // one would end while the worker was still publishing.
        assert!(!UiEvent::ModelAnswered {
            model: "m-1".to_string(),
            effort: crate::provider::effort::Effort::Auto,
            outcome: ModelAnswer::Unchanged,
            next_turn: false,
            announced: false,
        }
        .is_terminal());
    }

    #[test]
    fn neither_new_work_item_nor_usage_is_a_terminal_event() {
        // The drain leaves on exactly the two events a turn cannot continue
        // past. A catalog or a usage number that answered `true` would end the
        // drain while the worker still owed its conclusion.
        assert!(!UiEvent::Usage {
            input: Some(1),
            output: Some(2)
        }
        .is_terminal());
        assert!(!UiEvent::CatalogLoaded {
            provider: crate::provider::ProviderId::Gateway,
            entries: Vec::new()
        }
        .is_terminal());
        assert!(!UiEvent::CatalogUnread {
            provider: crate::provider::ProviderId::Gateway,
            retry: None,
        }
        .is_terminal());
        assert!(!UiEvent::ProviderSelected {
            provider: crate::provider::ProviderId::Gateway,
            model: "m".to_string(),
            missing_credential: false
        }
        .is_terminal());
        assert!(UiEvent::TurnEnded { failure: None }.is_terminal());
    }

    #[test]
    fn an_amendment_reaches_the_band_as_the_users_sentence_and_reaches_it_inert() {
        // The one string on this channel a **person** typed on purpose, and the
        // band is about to paint it into the document. `made_inert` is
        // exhaustive precisely so that a variant carrying text cannot be added
        // without answering this.
        let translated = translate(&Event::ToolFeedback {
            call_id: "c1".to_string(),
            text: "keep the header".to_string(),
        });
        assert_eq!(
            translated,
            Some(UiEvent::ToolFeedback {
                call_id: "c1".to_string(),
                text: "keep the header".to_string(),
            }),
            "the amendment never reached the UI at all"
        );

        let UiEvent::ToolFeedback { call_id, text } = UiEvent::ToolFeedback {
            call_id: "c\u{1b}[2J1".to_string(),
            text: "keep\nthe\u{1b}[2Jheader".to_string(),
        }
        .made_inert() else {
            panic!("made_inert changed the variant");
        };
        assert!(
            !text.contains('\u{1b}'),
            "a sentence typed at a terminal reached the band able to paint it: {text:?}"
        );
        assert!(!call_id.contains('\u{1b}'), "{call_id:?}");
        // The newline survives here **on purpose**, and this is the case that
        // says so rather than leaving it to be discovered: [`obeyed`] exempts
        // `\n` because a streamed answer's line breaks are the rows the
        // transcript exists to make. An amendment is one row, and it is
        // `super::shell` that makes it one -- with the same `safe_one_line` it
        // already flattens a tool's detail with.
        assert!(
            text.contains('\n'),
            "the channel started flattening text the transcript needs whole: {text:?}"
        );
    }
}
