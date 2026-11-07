//! `/copy` on this surface: the clipboard write, off the thread that holds the
//! terminal.
//!
//! What is copied and how a clipboard tool is run are [`crate::clipboard`]'s;
//! what this module adds is **where the wait happens**. Upstream waits for its
//! clipboard tool on the thread that called it (`copyToClipboard`,
//! `native.zig:15-52`), and so does the line shell. Here that thread is the UI
//! thread, which sits in `pselect(2)` owning the terminal -- and a tool that
//! never exits, or one that reads slowly, would stop the band being painted.
//! So each copy runs on a thread of its own and says how it went on a channel
//! the UI reads once a turn of its loop ([`Copies::finished`]), the way it reads
//! the clock.
//!
//! **And none of those threads outlives the session** ([`Errands`]): when the
//! shell is dropped -- inside `super::hold`, ahead of `super::signals::release`
//! and the restore's last steps -- a copy still running is given a moment to
//! land, then its tool is killed, and its thread is joined, so no tool of the
//! session is still writing a clipboard once the session is over. Each thread
//! is started with every owned signal blocked
//! ([`super::signals::spawn_unsignalled`]), so no handler ever runs on it.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::clipboard::{Clipboard, Stop};

/// The name each copy's thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-clipboard";

/// Where a shell built by a test looks for its clipboard tool: nowhere.
///
/// A test that wants a copy to happen gives the shell a directory with a fake
/// tool in it ([`Copies::new`]); one that does not cannot reach the user's
/// real clipboard by typing `/copy`.
#[cfg(test)]
pub(crate) const NOWHERE: &str = "/nonexistent/xfx-test-has-no-clipboard";

/// How long a session that is ending lets clipboard work still running finish
/// on its own before its tool is killed.
///
/// Long enough for a tool that answers -- `pbcopy` takes milliseconds, a
/// warmed `osascript` a few hundred -- and short enough that a tool that never
/// will costs the exit one second rather than its whole deadline. Upstream's
/// exit cannot be waiting on one: it copies on the thread the keystroke
/// arrived on, so a copy is over before the next key is read.
pub(crate) const GRACE: Duration = Duration::from_secs(1);

/// The threads clipboard work runs on, and the end the session gives them.
///
/// Each piece of work is handed a [`Stop`] of its own. Dropping this waits up
/// to [`GRACE`] for every thread still running, then stops what is left -- a
/// tool still running is killed, one not started is never started -- and
/// joins every thread. What a thread does besides waiting on a tool (a
/// report's write, `super::trace`) is not stopped: it is joined whole.
#[derive(Debug, Default)]
pub(crate) struct Errands {
    running: Mutex<Vec<(JoinHandle<()>, Stop)>>,
}

impl Errands {
    /// Runs `work` on a thread named `name`, handing it the [`Stop`] the end
    /// of the session will give it up with. `false` when no thread could be
    /// started.
    pub(crate) fn spawn(&self, name: &str, work: impl FnOnce(&Stop) + Send + 'static) -> bool {
        let stop = Stop::default();
        let theirs = stop.clone();
        let Ok(thread) = super::signals::spawn_unsignalled(name, move || work(&theirs)) else {
            return false;
        };
        let mut running = self.lock();
        // A thread that has finished is joined by being forgotten: there is
        // nothing left of it to wait for.
        running.retain(|(thread, _)| !thread.is_finished());
        running.push((thread, stop));
        true
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(JoinHandle<()>, Stop)>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The end of every thread started so far: each still running is waited
    /// for until `over` says the grace is over -- asked with when the wait
    /// began, and only while a thread is still running -- then given up,
    /// and every thread is joined.
    ///
    /// A session's end is [`GRACE`] on the clock ([`Drop`]). `over` is the
    /// seam a case decides the grace with by the order of events rather than
    /// a second count: it can let a tool answer only once the end has begun,
    /// or end the grace before a tool answers.
    fn end(&self, mut over: impl FnMut(Instant) -> bool) {
        let running = std::mem::take(&mut *self.lock());
        let began = Instant::now();
        while running.iter().any(|(thread, _)| !thread.is_finished()) && !over(began) {
            std::thread::sleep(Duration::from_millis(5));
        }
        for (_, stop) in &running {
            stop.stop();
        }
        for (thread, _) in running {
            let _ = thread.join();
        }
    }
}

impl Drop for Errands {
    fn drop(&mut self) {
        self.end(|began| began.elapsed() >= GRACE);
    }
}

/// The copies this session has started, and how each of them went.
#[derive(Debug)]
pub(crate) struct Copies {
    clipboard: Clipboard,
    /// The threads they run on ([`Errands`]).
    errands: Errands,
    /// Cloned into each copy's thread, which sends one outcome and ends.
    done: Sender<bool>,
    outcomes: Receiver<bool>,
    /// The same for a composer selection's copies (Super+C and Super+X),
    /// apart from `/copy`'s because the two are answered differently: a
    /// `/copy` says how it went, and a selection's copy says nothing
    /// (`copy_selection`/`cut_selection`, `app_input_runtime.zig:356-385`,
    /// whose only report is a debug trace) -- but a cut takes its text out of
    /// the draft only once the clipboard has it.
    selection_done: Sender<(Selection, bool)>,
    selections: Receiver<(Selection, bool)>,
}

/// What a selection was copied for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    /// Super+C: the clipboard is the whole of it.
    Copy,
    /// Super+X, numbered so the session can tell this cut from a later one.
    Cut(u64),
}

impl Copies {
    pub(crate) fn new(clipboard: Clipboard) -> Self {
        let (done, outcomes) = mpsc::channel();
        let (selection_done, selections) = mpsc::channel();
        Self {
            clipboard,
            errands: Errands::default(),
            done,
            outcomes,
            selection_done,
            selections,
        }
    }

    /// Starts putting a selection on the clipboard, through the same tool
    /// and off the same thread as `/copy` ([`Self::start`]); `false` when no
    /// thread could be started, which is a copy that failed.
    pub(crate) fn start_selection(&self, text: Arc<str>, purpose: Selection) -> bool {
        let clipboard = self.clipboard.clone();
        let done = self.selection_done.clone();
        self.errands.spawn(THREAD_NAME, move |stop| {
            let _ = done.send((purpose, clipboard.copy_until(&text, stop)));
        })
    }

    /// How one selection copy that has finished went, or `None` when none has
    /// finished since the last call.
    pub(crate) fn selection_finished(&self) -> Option<(Selection, bool)> {
        self.selections.try_recv().ok()
    }

    /// Starts putting `reply` on the clipboard, and returns without waiting.
    ///
    /// `false` when no thread could be started for it, which is a copy that
    /// failed and will never say so on the channel.
    pub(crate) fn start(&self, reply: Arc<str>) -> bool {
        let clipboard = self.clipboard.clone();
        let done = self.done.clone();
        self.errands.spawn(THREAD_NAME, move |stop| {
            // Nobody left to tell is a session that has ended, and the
            // clipboard was written for it all the same.
            let _ = done.send(clipboard.copy_until(&reply, stop));
        })
    }

    /// How one copy that has finished went -- `true` when the clipboard has
    /// the reply -- or `None` when none has finished since the last call.
    ///
    /// In the order they finished, which for two copies of one reply is the
    /// only order there is to tell apart.
    pub(crate) fn finished(&self) -> Option<bool> {
        self.outcomes.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::fake::FakeTool;
    use std::time::{Duration, Instant};

    /// How long a case waits for a fake tool: a fresh script's first run is
    /// assessed by macOS, which took 8 to 12 s on a machine at load 35 -- and
    /// no case here is about how fast a tool starts.
    const LOADED: Duration = Duration::from_secs(60);

    /// Waits for one copy to finish, or fails the case after `limit`.
    fn outcome(copies: &Copies, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(copied) = copies.finished() {
                return copied;
            }
            assert!(Instant::now() < deadline, "the copy never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread() {
        // D4c's finding: the copy's thread is spawned from the UI thread and
        // must not outlive the restore. Dropped, the copies wait out the
        // grace, kill the tool and join -- seconds before the tool would have
        // ended on its own.
        let tool = crate::clipboard::fake::FakeTool::running(
            "cat >/dev/null\necho $$ > \"${0%/*}/hung\"\nexec sleep 30",
        );
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start(Arc::from("reply")));
        let deadline = Instant::now() + LOADED;
        while tool.hung().is_none() {
            assert!(Instant::now() < deadline, "the tool never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid = tool.hung().expect("the tool started");
        let ending = Instant::now();
        drop(copies);
        let took = ending.elapsed();
        assert!(took >= GRACE, "the copy was not given its grace: {took:?}");
        assert!(
            took < GRACE + Duration::from_secs(10),
            "the end waited on the tool: {took:?}"
        );
        assert!(
            !crate::clipboard::fake::FakeTool::alive(pid),
            "the tool outlived the session"
        );
    }

    /// A tool that keeps what it is fed once its gate is open
    /// ([`FakeTool::gate`]), and says who it is before it waits there. Each
    /// case that gates its tool first asks whether the gate gave up: a case
    /// that would have waited for ever fails there, with the reason.
    fn gated() -> FakeTool {
        FakeTool::running(&format!(
            "echo $$ > \"${{0%/*}}/hung\"\n{}cat > \"${{0%/*}}/copied\"",
            FakeTool::gate()
        ))
    }

    #[test]
    fn an_ending_session_lets_a_copy_that_answers_land() {
        // The tool answers only once the session has begun to end: its gate
        // is opened by the end's first look at the grace. So the copy lands
        // inside the grace or not at all, by the order of events and not by
        // how fast a loaded machine starts a tool -- a 0.2 s tool against
        // the 1 s grace failed under load (FLAKE-COPYEND). The grace this
        // case gives runs out at LOADED, which only a tool that never
        // answers reaches; that it is GRACE in a session is the case above.
        let tool = gated();
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start(Arc::from("the reply")));
        let mut looked = false;
        copies.errands.end(|began| {
            if !looked {
                tool.open();
                looked = true;
            }
            began.elapsed() >= LOADED
        });
        assert!(
            !tool.gave_up(),
            "nobody opened the gate: the start or the end waited for the tool"
        );
        assert!(looked, "the end gave a copy still running no grace");
        assert_eq!(tool.copied().as_deref(), Some("the reply".as_bytes()));
    }

    #[test]
    fn an_ending_session_kills_a_copy_whose_answer_comes_after_its_grace() {
        // The same order the other way round: the tool is running with its
        // answer behind the gate, and the grace is over at the end's first
        // look. The tool is killed, and the gate opened afterwards lets
        // nothing land.
        let tool = gated();
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start(Arc::from("the reply")));
        let deadline = Instant::now() + LOADED;
        while tool.hung().is_none() {
            assert!(Instant::now() < deadline, "the tool never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid = tool.hung().expect("the tool started");
        copies.errands.end(|_| true);
        assert!(
            !tool.gave_up(),
            "the end waited for the tool until its gate gave up"
        );
        assert!(!FakeTool::alive(pid), "the tool outlived the session");
        tool.open();
        assert_eq!(tool.copied(), None, "a copy given up landed");
    }

    #[test]
    fn a_copy_is_started_and_answered_later_rather_than_waited_for() {
        // A tool that answers only when this case opens its gate, so "start
        // returned" and "the copy is over" are two moments in the case's own
        // order, not a second apart. A start that waited for the tool would
        // wait for ever: a watchdog opens the gate after LOADED, so such a
        // start fails the case rather than hanging it.
        let tool = Arc::new(FakeTool::running(&format!(
            "cat >/dev/null\n{}",
            FakeTool::gate()
        )));
        let copies = Copies::new(tool.clipboard());
        let (cancel, cancelled) = mpsc::channel::<()>();
        let watchdog = {
            let tool = Arc::clone(&tool);
            std::thread::spawn(move || {
                let fired = cancelled.recv_timeout(LOADED).is_err();
                if fired {
                    tool.open();
                }
                fired
            })
        };
        assert!(copies.start(Arc::from("reply")));
        assert!(
            !tool.gave_up(),
            "starting a copy waited for the tool until its gate gave up"
        );
        assert_eq!(copies.finished(), None, "a copy still running was answered");
        cancel.send(()).expect("the watchdog is waiting");
        assert!(
            !watchdog.join().expect("the watchdog's thread"),
            "starting a copy waited for the tool"
        );
        tool.open();
        assert!(outcome(&copies, LOADED));
    }

    #[test]
    fn a_selection_copy_is_answered_on_its_own_channel_with_its_purpose() {
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start_selection(Arc::from("picked"), Selection::Cut(7)));
        let deadline = Instant::now() + LOADED;
        let answer = loop {
            if let Some(answer) = copies.selection_finished() {
                break answer;
            }
            assert!(Instant::now() < deadline, "the copy never finished");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(answer, (Selection::Cut(7), true));
        assert_eq!(tool.copied().as_deref(), Some(&b"picked"[..]));
        assert_eq!(
            copies.finished(),
            None,
            "a selection's copy was answered as a /copy"
        );
    }

    #[test]
    fn a_copy_that_cannot_reach_a_tool_says_so_rather_than_nothing() {
        let copies = Copies::new(Clipboard::searching(NOWHERE));
        assert!(copies.start(Arc::from("reply")));
        assert!(!outcome(&copies, LOADED));
    }
}
