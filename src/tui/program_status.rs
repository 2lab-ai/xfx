//! OSC 7501 program status: what the session tells the terminal it is doing.
//!
//! Upstream's Program Status Protocol reports (`vercel-labs/fx@481104fd
//! src/ui/terminal/program_status.zig`): idle, working, waiting on the user,
//! finished, or failed, each written as one `ESC ] 7501 ; … ESC \` between
//! frames. A terminal that does not implement the protocol ignores the
//! sequence, so nothing is asked first and nothing is configured (`:1-6`).
//!
//! **The rule is upstream's, line for line.** The settled value, the
//! "report each result only once" dedup (`4ae9eeeb`), the one-line message and
//! its bound are ported from `:41-204`, and its tests below are upstream's own
//! vectors (`:220-361`). Two things differ, each for a reason:
//!
//! * **`app=xfx`, not `app=fx`.** The record names the program that owns the
//!   terminal, and the program is xfx: the window title and the banner say so
//!   (`super::frame`'s title). Upstream writes its own name (`:22`).
//! * **A refused write is offered again** ([`Reporter::refused`]): every
//!   coordinate-free vector the loop writes -- the bells, the theme query -- is
//!   re-armed when the terminal took none of it, where upstream traces the
//!   failure and moves on (`transcript/runtime.zig:10416-10429`). Observable
//!   only on a terminal that refused a write.
//!
//! **What reaches the terminal is ASCII the session spelled.** The only text
//! that comes from outside -- a permission's label, a model's question -- is
//! folded to one line without control characters, bounded at
//! [`MAX_MESSAGE_BYTES`], and then base64-encoded (`:164-204`, `:152-160`): no
//! byte of it reaches the terminal as itself, whatever it held.

/// Removes every record on the terminal, not only this session's: written by
/// every restore, planned or not, and by the stop handler
/// (`app_lifecycle.zig:43-48`). Spelled again inside each of
/// [`super::term`]'s restore constants, which are literals a signal handler
/// writes in one `write(2)`; a `const` assertion there holds each to this at
/// build time, and a test holds the bytes.
pub(crate) const CLEAR: &str = "\x1b]7501;state=clear\x1b\\";

const PREFIX: &str = "\x1b]7501;";
const TERMINATOR: &str = "\x1b\\";
/// The name the record is kept under.
pub(crate) const APP: &str = "xfx";
/// Keeps `msg` to one short line. The protocol allows 2048 decoded bytes
/// (`program_status.zig:23-24`).
pub(crate) const MAX_MESSAGE_BYTES: usize = 256;
/// What base64 makes of [`MAX_MESSAGE_BYTES`], padding included.
pub(crate) const MAX_ENCODED_MESSAGE: usize = MAX_MESSAGE_BYTES.div_ceil(3) * 4;
const MAX_REPORT_BYTES: usize = PREFIX.len()
    + "state=blocked:kind=permission:app=".len()
    + APP.len()
    + ":msg=".len()
    + MAX_ENCODED_MESSAGE
    + TERMINATOR.len();

/// What the user is being asked to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Blocked {
    Permission,
    Question,
}

/// What the session is doing, as the loop sees it on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity<'a> {
    /// Waiting on the user. `message` is untrusted text, borrowed for the call
    /// and reduced to one line without control characters before it is
    /// encoded.
    Blocked {
        kind: Blocked,
        message: &'a str,
    },
    Working,
    /// Nothing is running; the last result decides what is reported.
    Settled,
}

/// How a turn ended, for the report (`types.TurnPresentationOutcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Completed,
    Failed,
    /// Stopped by the user. Upstream reports a cancelled or paused turn as
    /// idle: it waits for the next instruction (`program_status.zig:66-69`).
    Interrupted,
}

/// What one report says, as the output check is told it
/// (`super::check::Intent::ProgramStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Idle,
    Working,
    Done,
    Error,
    Permission,
    Question,
}

/// One report: what it says, and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Report {
    pub(crate) state: State,
    pub(crate) bytes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Settled {
    #[default]
    Idle,
    Done,
    Error,
}

/// Owned by the UI thread. Keeps the last report so unchanged activity is not
/// written to the terminal again on every tick (`program_status.zig:43-56`).
#[derive(Debug, Default)]
pub(crate) struct Reporter {
    settled: Settled,
    /// Set once a settled report carried `settled`. Work that starts no turn
    /// then settles as idle instead of repeating a result the user already
    /// saw (`4ae9eeeb`).
    settled_reported: bool,
    /// The last report written, or `None` when the next one must be written
    /// whatever it says.
    last: Option<String>,
}

impl Reporter {
    /// A new turn must not report an earlier turn's result once it settles.
    pub(crate) fn turn_started(&mut self) {
        self.settle(Settled::Idle);
    }

    pub(crate) fn turn_finished(&mut self, outcome: Outcome) {
        self.settle(match outcome {
            Outcome::Completed => Settled::Done,
            Outcome::Failed => Settled::Error,
            Outcome::Interrupted => Settled::Idle,
        });
    }

    /// The result of a manual `/compact`, which runs without a turn
    /// (`noteCompaction`, `program_status.zig:80-96`): `None` for one in which
    /// nothing ran, which reports nothing.
    pub(crate) fn compaction(&mut self, outcome: Option<Outcome>) {
        if let Some(outcome) = outcome {
            self.turn_finished(outcome);
        }
    }

    /// Sends the next report even when it matches the last one -- after a
    /// stop, whose restore cleared the record. A result already reported comes
    /// back as idle (`program_status.zig:98-104`).
    pub(crate) fn invalidate(&mut self) {
        self.last = None;
        if self.settled_reported {
            self.settle(Settled::Idle);
        }
    }

    /// The report this tick produced was not taken by the terminal: it is
    /// offered again on the next tick, recomputed rather than replayed.
    pub(crate) fn refused(&mut self) {
        self.last = None;
    }

    /// The report to write when it differs from the last one
    /// (`program_status.zig:106-128`).
    pub(crate) fn update(&mut self, activity: Activity<'_>) -> Option<Report> {
        // A new result is news even when it matches the last report, such as
        // a second failure in a row.
        let mut new_result = false;
        match activity {
            Activity::Settled => {
                new_result = !self.settled_reported && self.settled != Settled::Idle;
                self.settled_reported = true;
            }
            Activity::Working | Activity::Blocked { .. } => {
                if self.settled_reported {
                    self.settle(Settled::Idle);
                }
            }
        }
        let report = encode(activity, self.settled)?;
        if !new_result && self.last.as_deref() == Some(report.bytes.as_str()) {
            return None;
        }
        self.last = Some(report.bytes.clone());
        Some(report)
    }

    fn settle(&mut self, next: Settled) {
        self.settled = next;
        self.settled_reported = false;
    }
}

/// What a permission question is reported as: the request's label, the
/// tool and the one thing it is about (`request.view().label`,
/// `app_worker_runtime.zig:774-778@481104fd`) -- upstream's
/// `formatPermissionLabel` shape (`tool_presentation.zig:723-757`), as
/// `super::approval_view` already names a call. A tool-set difference rides
/// along: upstream's command is `shell.run <command>`, xfx's is
/// `terminal <command>`.
pub(crate) fn permission_label(request: &crate::permission::ApprovalRequest) -> String {
    use crate::permission::ApprovalSubject;
    let argument = match &request.subject {
        ApprovalSubject::Path(argument) | ApprovalSubject::Call(argument) => argument,
        ApprovalSubject::Command { command, .. } => command,
    };
    if argument.is_empty() {
        request.tool.to_string()
    } else {
        format!("{} {argument}", request.tool)
    }
}

/// What a rule change's confirmation is reported as: upstream's label for it
/// (`app_commands.zig:681-684`, `:735`), which its band asks as the question
/// (`approvalQuestion`, `approval_ui.zig:1853-1862`).
pub(crate) fn rule_label(question: &'static str) -> &'static str {
    if question == crate::permission::slash::REVOKE_QUESTION {
        "Revoke saved-session permission rule"
    } else {
        question.strip_suffix('?').unwrap_or(question)
    }
}

/// What a manual `/compact` came to, for the report: upstream's outcome for
/// each ending (`noteCompaction`, `program_status.zig:89-95`), where every
/// failure -- one that never started, one too large, one whose checkpoint may
/// not have landed (`compaction_activity.zig:198`) -- is `failed`. `None` for
/// one in which nothing ran.
pub(crate) fn of_compaction(feedback: super::compaction::Feedback) -> Option<Outcome> {
    use super::compaction::Feedback;
    match feedback {
        Feedback::Succeeded => Some(Outcome::Completed),
        Feedback::Cancelled => Some(Outcome::Interrupted),
        Feedback::NotStarted | Feedback::Failed | Feedback::TooLarge | Feedback::Unconfirmed => {
            Some(Outcome::Failed)
        }
        Feedback::NoOp | Feedback::Busy => None,
    }
}

/// The report, or `None` when it would not fit [`MAX_REPORT_BYTES`] -- which
/// the message's own bound makes unreachable, and which upstream drops rather
/// than writes cut (`program_status.zig:120-123`, `:136-141`).
fn encode(activity: Activity<'_>, settled: Settled) -> Option<Report> {
    let (state, word) = match activity {
        Activity::Blocked {
            kind: Blocked::Permission,
            ..
        } => (State::Permission, "blocked:kind=permission"),
        Activity::Blocked {
            kind: Blocked::Question,
            ..
        } => (State::Question, "blocked:kind=question"),
        Activity::Working => (State::Working, "working"),
        Activity::Settled => match settled {
            Settled::Idle => (State::Idle, "idle"),
            Settled::Done => (State::Done, "done"),
            Settled::Error => (State::Error, "error"),
        },
    };
    // Each report replaces its record, so every report repeats `app`.
    let mut bytes = format!("{PREFIX}state={word}:app={APP}");
    if let Activity::Blocked { message, .. } = activity {
        let message = one_line(message.as_bytes());
        if !message.is_empty() {
            bytes.push_str(":msg=");
            bytes.push_str(&crate::image::base64(&message));
        }
    }
    bytes.push_str(TERMINATOR);
    (bytes.len() <= MAX_REPORT_BYTES).then_some(Report { state, bytes })
}

/// Terminals discard a report whose message holds a control character, so
/// whitespace and control runs collapse to one space and invalid UTF-8 is
/// dropped; long text ends with `...` within [`MAX_MESSAGE_BYTES`]
/// (`oneLineMessage`, `program_status.zig:164-204`).
///
/// Over bytes rather than a `&str` so upstream's own vector -- which carries
/// invalid UTF-8 -- runs here unchanged. Its decoding steps are upstream's:
/// an invalid lead byte is skipped alone and sets no space, a well-led
/// sequence that does not decode is skipped one byte at a time, and a
/// sequence cut off by the end of the text ends it.
pub(crate) fn one_line(raw: &[u8]) -> Vec<u8> {
    const MARKER: &[u8] = b"...";
    let mut out: Vec<u8> = Vec::with_capacity(MAX_MESSAGE_BYTES.min(raw.len()));
    let mut space_pending = false;
    let mut index = 0;
    while index < raw.len() {
        let Some(len) = sequence_len(raw[index]) else {
            index += 1;
            continue;
        };
        if raw.len() - index < len {
            break;
        }
        let sequence = &raw[index..index + len];
        let Some(codepoint) = std::str::from_utf8(sequence)
            .ok()
            .and_then(|text| text.chars().next())
        else {
            index += 1;
            continue;
        };
        index += len;
        let codepoint = u32::from(codepoint);
        if codepoint <= 0x20 || (0x7f..=0x9f).contains(&codepoint) {
            space_pending = !out.is_empty();
            continue;
        }
        let separator = usize::from(space_pending);
        if out.len() + separator + len > MAX_MESSAGE_BYTES {
            let keep = backward_boundary(&out, MAX_MESSAGE_BYTES - MARKER.len());
            out.truncate(keep);
            out.extend_from_slice(MARKER);
            break;
        }
        if space_pending {
            out.push(b' ');
            space_pending = false;
        }
        out.extend_from_slice(sequence);
    }
    out
}

/// The length a UTF-8 sequence led by `lead` claims, or `None` for a byte no
/// sequence starts with (`std.unicode.utf8ByteSequenceLength`).
fn sequence_len(lead: u8) -> Option<usize> {
    match lead {
        0x00..=0x7f => Some(1),
        0xc0..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf7 => Some(4),
        _ => None,
    }
}

/// The last character boundary of `text` at or before `index`
/// (`text_utils.utf8BackwardBoundary`).
fn backward_boundary(text: &[u8], index: usize) -> usize {
    let mut len = index.min(text.len());
    while len > 0 && len < text.len() && text[len] & 0b1100_0000 == 0b1000_0000 {
        len -= 1;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: &str = "\x1b]7501;state=idle:app=xfx\x1b\\";
    const WORKING: &str = "\x1b]7501;state=working:app=xfx\x1b\\";
    const DONE: &str = "\x1b]7501;state=done:app=xfx\x1b\\";
    const ERROR: &str = "\x1b]7501;state=error:app=xfx\x1b\\";

    fn permission(message: &str) -> Activity<'_> {
        Activity::Blocked {
            kind: Blocked::Permission,
            message,
        }
    }

    fn question(message: &str) -> Activity<'_> {
        Activity::Blocked {
            kind: Blocked::Question,
            message,
        }
    }

    #[track_caller]
    fn expect_report(reporter: &mut Reporter, activity: Activity<'_>, expected: &str) {
        let report = reporter.update(activity).expect("a report");
        assert_eq!(report.bytes, expected);
    }

    /// The message a report carries, decoded by a decoder of this test's own.
    fn decoded_message(report: &str) -> Vec<u8> {
        let start = report.find(":msg=").expect("a message") + ":msg=".len();
        let encoded = &report.as_bytes()[start..report.len() - TERMINATOR.len()];
        let value = |byte: u8| -> u32 {
            match byte {
                b'A'..=b'Z' => u32::from(byte - b'A'),
                b'a'..=b'z' => u32::from(byte - b'a') + 26,
                b'0'..=b'9' => u32::from(byte - b'0') + 52,
                b'+' => 62,
                b'/' => 63,
                other => panic!("{other:#x} is not base64"),
            }
        };
        let mut out = Vec::new();
        for chunk in encoded.chunks(4) {
            let pad = chunk.iter().filter(|byte| **byte == b'=').count();
            let mut n = 0u32;
            for byte in chunk {
                n = (n << 6) | if *byte == b'=' { 0 } else { value(*byte) };
            }
            let three = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            out.extend_from_slice(&three[..3 - pad]);
        }
        out
    }

    #[test]
    fn program_status_follows_a_turn_from_idle_to_its_outcome() {
        // `program_status.zig:220-256`.
        let mut reporter = Reporter::default();
        expect_report(&mut reporter, Activity::Settled, IDLE);
        reporter.turn_started();
        expect_report(&mut reporter, Activity::Working, WORKING);
        expect_report(
            &mut reporter,
            permission("shell.run git push"),
            "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=c2hlbGwucnVuIGdpdCBwdXNo\x1b\\",
        );
        expect_report(&mut reporter, Activity::Working, WORKING);
        reporter.turn_finished(Outcome::Completed);
        expect_report(&mut reporter, Activity::Settled, DONE);

        reporter.turn_started();
        expect_report(
            &mut reporter,
            question(""),
            "\x1b]7501;state=blocked:kind=question:app=xfx\x1b\\",
        );
        reporter.turn_finished(Outcome::Interrupted);
        expect_report(&mut reporter, Activity::Settled, IDLE);

        // Upstream's `.paused` is reported as its `.interrupted` is; xfx's
        // stopped turn is the one outcome for both.
        reporter.turn_started();
        expect_report(&mut reporter, Activity::Working, WORKING);
        reporter.turn_finished(Outcome::Interrupted);
        expect_report(&mut reporter, Activity::Settled, IDLE);

        reporter.turn_started();
        reporter.turn_finished(Outcome::Failed);
        expect_report(&mut reporter, Activity::Settled, ERROR);
        // A second failure in a row is reported again.
        reporter.turn_started();
        reporter.turn_finished(Outcome::Failed);
        expect_report(&mut reporter, Activity::Settled, ERROR);
        assert_eq!(reporter.update(Activity::Settled), None);
        reporter.turn_started();
        expect_report(&mut reporter, Activity::Settled, IDLE);
        reporter.turn_finished(Outcome::Failed);
        expect_report(&mut reporter, Activity::Settled, ERROR);
    }

    #[test]
    fn program_status_writes_only_changed_reports_until_invalidated() {
        // `program_status.zig:258-269`.
        let mut reporter = Reporter::default();
        assert!(reporter.update(Activity::Working).is_some());
        assert_eq!(reporter.update(Activity::Working), None);
        reporter.invalidate();
        expect_report(&mut reporter, Activity::Working, WORKING);

        assert!(reporter.update(question("First?")).is_some());
        assert_eq!(reporter.update(question("First?")), None);
        assert!(reporter.update(question("Second?")).is_some());
    }

    #[test]
    fn program_status_does_not_repeat_a_result_the_user_already_saw() {
        // `program_status.zig:271-296`.
        let mut reporter = Reporter::default();
        reporter.turn_started();
        reporter.turn_finished(Outcome::Failed);
        expect_report(&mut reporter, Activity::Settled, ERROR);
        // `/compact` runs without starting a turn.
        expect_report(&mut reporter, Activity::Working, WORKING);
        expect_report(&mut reporter, Activity::Settled, IDLE);

        reporter.turn_started();
        reporter.turn_finished(Outcome::Completed);
        expect_report(&mut reporter, Activity::Settled, DONE);
        // `/permissions remember` asks for confirmation outside a turn.
        expect_report(
            &mut reporter,
            permission(""),
            "\x1b]7501;state=blocked:kind=permission:app=xfx\x1b\\",
        );
        expect_report(&mut reporter, Activity::Settled, IDLE);

        reporter.turn_started();
        reporter.turn_finished(Outcome::Completed);
        expect_report(&mut reporter, Activity::Settled, DONE);
        reporter.invalidate();
        expect_report(&mut reporter, Activity::Settled, IDLE);
    }

    #[test]
    fn program_status_keeps_an_outcome_noted_before_the_turn_was_seen_running() {
        // `program_status.zig:298-306`: a turn can start and fail within one
        // drain of events.
        let mut reporter = Reporter::default();
        expect_report(&mut reporter, Activity::Settled, IDLE);
        reporter.turn_started();
        reporter.turn_finished(Outcome::Failed);
        expect_report(&mut reporter, Activity::Working, WORKING);
        expect_report(&mut reporter, Activity::Settled, ERROR);
    }

    #[test]
    fn program_status_reports_each_manual_compaction_result() {
        // `program_status.zig:308-337`, with the once-per-operation guard on
        // the shell's side (`Shell::compaction_update`), which is where xfx
        // learns an operation ended.
        let mut reporter = Reporter::default();
        expect_report(&mut reporter, Activity::Working, WORKING);
        reporter.compaction(Some(Outcome::Failed));
        expect_report(&mut reporter, Activity::Settled, ERROR);
        expect_report(&mut reporter, Activity::Working, WORKING);
        expect_report(&mut reporter, Activity::Settled, IDLE);

        expect_report(&mut reporter, Activity::Working, WORKING);
        reporter.compaction(Some(Outcome::Completed));
        expect_report(&mut reporter, Activity::Settled, DONE);

        // Nothing ran, so there is no result to report.
        reporter.compaction(None);
        assert_eq!(reporter.update(Activity::Settled), None);
    }

    #[test]
    fn program_status_messages_are_one_bounded_line_without_control_characters() {
        // `program_status.zig:339-357`, the hostile vector byte for byte.
        let hostile: &[u8] = b"  Run\n\ttests?\x1b]2;owned\x07 \xc2\x9b now\xff\xe2 ok  ";
        assert_eq!(one_line(hostile), b"Run tests? ]2;owned now ok");
        // The same through a report, from text the session holds: only the
        // OSC introducer and the string terminator carry ESC.
        let mut reporter = Reporter::default();
        let report = reporter
            .update(question("  Run\n\ttests?\x1b]2;owned\x07 \u{9b} now ok  "))
            .expect("a report");
        assert_eq!(
            decoded_message(&report.bytes),
            b"Run tests? ]2;owned now ok"
        );
        assert_eq!(report.bytes.matches('\x1b').count(), 2);
        assert!(report.bytes.ends_with(TERMINATOR));

        let long = "é".repeat(200);
        let long_report = reporter.update(permission(&long)).expect("a report");
        let message = decoded_message(&long_report.bytes);
        assert!(message.len() <= MAX_MESSAGE_BYTES);
        assert!(message.ends_with(b"..."));
        assert!(std::str::from_utf8(&message).is_ok());
        assert!(long_report.bytes.len() <= MAX_REPORT_BYTES);
    }

    #[test]
    fn a_message_reaches_the_terminal_as_base64_and_nothing_else() {
        // The security claim, over every shape of foreign text a permission's
        // label or a model's question can carry: controls (C0, DEL, C1, an
        // OSC, a CSI), bidi overrides and isolates, zero-width marks, and a
        // text long enough to be cut. The bytes after `:msg=` are the base64
        // alphabet and nothing else, and what they decode to holds no control
        // character -- a bidi mark is kept, inside the encoding, as upstream
        // keeps it.
        let cases = [
            "rm -rf / \u{202e}gnp.exe\u{202c}",
            "\u{2066}isolated\u{2069} \u{200f}rtl\u{200e}",
            "a\x1b[2J\x1b]52;c;aGVsbG8=\x07b",
            "c\u{7f}d\u{85}e\u{9b}1m\u{9d}f",
            "zero\u{200b}width\u{feff}",
            &"\u{202e}x".repeat(300),
        ];
        let mut reporter = Reporter::default();
        for (n, text) in cases.iter().enumerate() {
            let kind = if n % 2 == 0 {
                permission(text)
            } else {
                question(text)
            };
            let report = reporter.update(kind).expect("a report");
            let start = report.bytes.find(":msg=").expect("a message") + ":msg=".len();
            let encoded = &report.bytes[start..report.bytes.len() - TERMINATOR.len()];
            assert!(
                encoded
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')),
                "{text:?} reached the wire as {encoded:?}"
            );
            assert!(encoded.len() <= MAX_ENCODED_MESSAGE);
            let message = String::from_utf8(decoded_message(&report.bytes)).expect("UTF-8");
            assert!(
                !message.chars().any(char::is_control),
                "{text:?} decoded to {message:?}"
            );
            assert!(message.len() <= MAX_MESSAGE_BYTES);
            // The report itself: one OSC, its two ESCs, and printable ASCII.
            assert_eq!(report.bytes.matches('\x1b').count(), 2, "{text:?}");
            assert!(report.bytes[1..report.bytes.len() - 2]
                .bytes()
                .all(|byte| (0x20..0x7f).contains(&byte)));
        }
    }

    #[test]
    fn a_refused_report_is_offered_again_and_says_what_is_true_now() {
        let mut reporter = Reporter::default();
        expect_report(&mut reporter, Activity::Settled, IDLE);
        reporter.turn_started();
        reporter.turn_finished(Outcome::Completed);
        expect_report(&mut reporter, Activity::Settled, DONE);
        reporter.refused();
        expect_report(&mut reporter, Activity::Settled, DONE);
        assert_eq!(reporter.update(Activity::Settled), None);
        // Refused, and work began before the next tick: that is what is
        // offered.
        reporter.refused();
        expect_report(&mut reporter, Activity::Working, WORKING);
    }

    #[test]
    fn a_rule_confirmation_is_reported_under_upstreams_label() {
        use crate::permission::session_rules::Decision;
        use crate::permission::slash;
        assert_eq!(
            rule_label(slash::remember_question(Decision::Allow)),
            "Remember allow for this saved session"
        );
        assert_eq!(
            rule_label(slash::remember_question(Decision::Deny)),
            "Remember deny for this saved session"
        );
        assert_eq!(
            rule_label(slash::REVOKE_QUESTION),
            "Revoke saved-session permission rule"
        );
    }

    #[test]
    fn a_compaction_reports_upstreams_outcome_for_each_ending() {
        use super::super::compaction::Feedback;
        assert_eq!(of_compaction(Feedback::Succeeded), Some(Outcome::Completed));
        assert_eq!(
            of_compaction(Feedback::Cancelled),
            Some(Outcome::Interrupted)
        );
        for failed in [
            Feedback::NotStarted,
            Feedback::Failed,
            Feedback::TooLarge,
            Feedback::Unconfirmed,
        ] {
            assert_eq!(of_compaction(failed), Some(Outcome::Failed), "{failed:?}");
        }
        assert_eq!(of_compaction(Feedback::NoOp), None);
        assert_eq!(of_compaction(Feedback::Busy), None);
    }

    #[test]
    fn program_status_clear_removes_every_record() {
        // `program_status.zig:359-361`.
        assert_eq!(CLEAR, "\x1b]7501;state=clear\x1b\\");
    }

    #[test]
    fn a_report_names_xfx_and_the_bound_is_upstreams() {
        assert!(IDLE.contains(":app=xfx"));
        assert_eq!(MAX_ENCODED_MESSAGE, 344);
        assert_eq!(MAX_REPORT_BYTES, 7 + 34 + 3 + 5 + 344 + 2);
    }
}
