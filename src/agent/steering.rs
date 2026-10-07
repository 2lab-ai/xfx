//! What the person says to a turn while it runs, delivered into that turn.
//!
//! Upstream calls it steering: a prompt submitted while a turn is working is
//! bound to that turn rather than queued behind it, and the turn takes it at
//! its next boundary (`vercel-labs/fx@c1db9191
//! src/core/agent/worker_runtime.zig:1080-1104`, `:1179-1278`):
//!
//! * **while the model is answering without a tool**, at once -- the answer is
//!   cut where it had got to, kept as the assistant's message, and the turn
//!   goes on with the prompt (`append_immediate_steering_after_cancel`,
//!   `orchestrator.zig:240-261`; e2e `tui-interrupt-recovery.test.ts:291`);
//! * **while a tool runs**, after its results, before the next request
//!   (`observe_steering_boundary`, `:157-178`);
//! * **when the model has answered**, after that answer, and the turn goes on
//!   rather than ending (`append_pending_steering_after_assistant`,
//!   `:215-238`) -- and once that boundary has found nothing, the turn takes no
//!   more, so a prompt sent after it is an ordinary one for the next turn
//!   (`direct_steering_closed`, `worker_runtime.zig:1206`).
//!
//! The model is told each one as a user message wrapped in upstream's own
//! words ([`message`]); the session records the person's text as they typed
//! it, which is what a resumed history replays (`execution_memory.zig:28-43`).
//!
//! **Shared, not sent.** The person's side (the TUI's band) and the turn's side
//! (the machine) hold one [`Steering`], and each change is made under its lock
//! -- which is what lets the band take a prompt back while a tool runs
//! (`popQueuedSteerForEdit`, `worker_runtime.zig:1340-1386`) and know, in the
//! same breath, that the turn has not already taken it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::Notify;

/// The words around a steer (`steering_open`, `steering_close`,
/// `execution_memory.zig:28-32`).
const OPEN: &str = "<user_steering>\nApply this live user update to the current task. Continue \
working unless the user asks you to stop, the task is complete, or a genuine blocker prevents \
progress.\n\n";
const CLOSE: &str = "\n</user_steering>";

/// The user message the model is given for `text` (`steeringMessage`,
/// `execution_memory.zig:40-42`).
pub fn message(text: &str) -> String {
    format!("{OPEN}{text}{CLOSE}")
}

/// One prompt the person sent into a running turn, under the number the band
/// gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steer {
    pub id: u64,
    pub text: String,
}

/// The one place a running turn's steers wait. Cloning shares it.
#[derive(Debug, Clone, Default)]
pub struct Steering {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    state: Mutex<State>,
    arrived: Notify,
}

#[derive(Debug, Default)]
struct State {
    /// Whether a turn is taking steers. Closed between turns, and from the
    /// moment a turn's last answer found none waiting.
    open: bool,
    /// Whether the turn is inside a tool step: a steer waits for its results,
    /// and only then may the band still take it back.
    tool_running: bool,
    pending: Vec<Steer>,
}

impl Steering {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Nothing that holds the lock can panic, and every field is whole after
        // each assignment.
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The band: hands `text` to the running turn. `false` when no turn is
    /// taking steers -- the prompt is then an ordinary one, for the next turn.
    pub fn offer(&self, id: u64, text: String) -> bool {
        {
            let mut state = self.state();
            if !state.open {
                return false;
            }
            state.pending.push(Steer { id, text });
        }
        self.inner.arrived.notify_one();
        true
    }

    /// The band: takes back the newest steer, but only while it is still
    /// waiting for a tool's results -- one sent while the model was answering
    /// has already cut that answer short (`popQueuedSteerForEdit`).
    pub fn retract(&self) -> Option<Steer> {
        let mut state = self.state();
        if !state.open || !state.tool_running {
            return None;
        }
        state.pending.pop()
    }

    /// The band: drops every steer still waiting, as a stop or a reset does.
    pub fn drop_pending(&self) -> usize {
        let mut state = self.state();
        let dropped = state.pending.len();
        state.pending.clear();
        dropped
    }

    /// The runtime: a turn begins, and takes steers.
    pub fn open(&self) {
        let mut state = self.state();
        state.open = true;
        state.tool_running = false;
        state.pending.clear();
    }

    /// The runtime: the turn is over. What was still waiting is dropped -- the
    /// band, which sent it, sends it again as an ordinary prompt.
    pub fn close(&self) {
        let mut state = self.state();
        state.open = false;
        state.tool_running = false;
        state.pending.clear();
    }

    /// The machine: whether anything is waiting.
    pub fn pending(&self) -> bool {
        !self.state().pending.is_empty()
    }

    /// The machine: a tool step begins or ends.
    pub fn tool_running(&self, running: bool) {
        self.state().tool_running = running;
    }

    /// The machine: everything waiting, taken at a boundary.
    pub fn take(&self) -> Vec<Steer> {
        std::mem::take(&mut self.state().pending)
    }

    /// The machine, at the model's answer: everything waiting -- or, when
    /// nothing is, the turn takes no more (`direct_steering_closed`).
    pub fn take_or_close(&self) -> Vec<Steer> {
        let mut state = self.state();
        if state.pending.is_empty() {
            state.open = false;
        }
        std::mem::take(&mut state.pending)
    }

    /// The machine: resolves once a steer is waiting. Cancel-safe, and never
    /// misses one offered between the check and the wait.
    pub async fn arrived(&self) {
        loop {
            let notified = self.inner.arrived.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.pending() {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steer_is_upstreams_message_around_the_persons_words() {
        assert_eq!(
            message("what are you doing"),
            "<user_steering>\nApply this live user update to the current task. Continue working \
             unless the user asks you to stop, the task is complete, or a genuine blocker \
             prevents progress.\n\nwhat are you doing\n</user_steering>"
        );
    }

    #[test]
    fn a_closed_turn_takes_nothing_and_the_last_answer_closes_an_empty_one() {
        let steering = Steering::new();
        assert!(!steering.offer(1, "early".into()), "a steer with no turn");
        steering.open();
        assert!(steering.offer(2, "one".into()));
        assert_eq!(
            steering.take_or_close(),
            [Steer {
                id: 2,
                text: "one".into()
            }]
        );
        assert!(
            steering.offer(3, "two".into()),
            "a turn that took one closed"
        );
        assert_eq!(steering.take_or_close().len(), 1);
        assert!(steering.take_or_close().is_empty());
        assert!(!steering.offer(4, "late".into()), "after the last answer");
        steering.open();
        assert!(steering.offer(5, "next turn".into()));
        steering.close();
        assert!(!steering.pending(), "a closed turn kept a steer");
    }

    #[test]
    fn only_a_steer_waiting_on_a_tool_can_be_taken_back() {
        let steering = Steering::new();
        steering.open();
        steering.offer(1, "first".into());
        steering.offer(2, "second".into());
        assert_eq!(steering.retract(), None, "a steer that cut an answer");
        steering.tool_running(true);
        assert_eq!(steering.retract().map(|steer| steer.id), Some(2));
        steering.tool_running(false);
        assert_eq!(steering.take().len(), 1);
        assert_eq!(steering.drop_pending(), 0);
    }

    #[tokio::test]
    async fn arrival_wakes_a_waiter_and_a_waiting_steer_resolves_at_once() {
        let steering = Steering::new();
        steering.open();
        let waiter = steering.clone();
        let wait = tokio::spawn(async move { waiter.arrived().await });
        tokio::task::yield_now().await;
        steering.offer(1, "now".into());
        tokio::time::timeout(std::time::Duration::from_secs(5), wait)
            .await
            .expect("woken")
            .expect("joined");
        // Still waiting, so a second wait does not sleep.
        tokio::time::timeout(std::time::Duration::from_secs(5), steering.arrived())
            .await
            .expect("resolved at once");
    }
}
