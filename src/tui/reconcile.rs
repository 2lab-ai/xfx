//! Asking the Gateway for the bills a turn's stream did not carry.
//!
//! A request whose stream named its generation and not its cost is recorded
//! pending ([`crate::session::RequestUsage::pending`]), and `/usage` says
//! `Partial data` over it until the bill is known. Upstream learns it the
//! same way every time: a thread of the session's own asks the Gateway by
//! generation id, up to thirty rounds a second apart, and folds each answer
//! into the session (`reconciliationThreadMain` and `reconcilePendingBlocking`,
//! `vercel-labs/fx@c1db9191 src/core/session/session_usage.zig:3257-3423`),
//! started whenever a deferred request is recorded or a session with pending
//! ones is opened (`startReconciliationWithCredential`, `:2008-2100`).
//!
//! Here the asking is a thread the runtime starts, and the folding is the
//! runtime's: an answer comes back on a channel [`super::worker`]'s loop
//! reads between pieces of work, and is written to the session's log by the
//! recorder that owns it -- one writer, as the log requires.
//!
//! **The thread is started with every owned signal blocked**
//! ([`super::signals::spawn_unsignalled`]), so no handler can run on it, and
//! it is never waited for.
//! Cancelled, it stops within a few milliseconds; a question it is in the
//! middle of is dropped, and the generation stays pending in the log for the
//! next session that opens it.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::gateway::generation::Lookup;
use crate::gateway::protocol::Billing;
use crate::session::RequestUsage;

/// The name the asking thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-bills";

/// How many rounds one start may ask (`reconciliationThreadMain` passes 30,
/// `session_usage.zig:3268-3276`).
const ROUNDS: usize = 30;

/// How long between two rounds (`waitForReconciliationRetry`: 200 slices of
/// 5 ms, `session_usage.zig:3425-3439`).
const BETWEEN_ROUNDS: Duration = Duration::from_secs(1);

/// How often a wait or a question looks at the cancellation.
const CANCEL_POLL: Duration = Duration::from_millis(5);

/// What the Gateway said about one generation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Settlement {
    /// Its bill.
    Billed(Billing),
    /// That it will never be known.
    Rejected(String),
}

/// One answer, and the session whose log it belongs in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Answered {
    pub(crate) session: String,
    pub(crate) settlement: Settlement,
}

impl Answered {
    /// The `usage_recorded` frame that writes it into the session's log: a
    /// bill under its generation id, or the id of one that will never be
    /// known ([`crate::session::SessionEvent::UsageRecorded`]).
    pub(crate) fn frame(&self) -> crate::session::SessionEvent {
        let (settled, rejected) = match &self.settlement {
            Settlement::Billed(bill) => (
                vec![RequestUsage {
                    at_ms: bill.at_ms,
                    // Kept from the request it settles: only this side timed it.
                    duration_ms: 0,
                    model: bill.model.clone(),
                    input_tokens: bill.input_tokens,
                    output_tokens: bill.output_tokens,
                    cache_read_tokens: bill.cache_read_tokens,
                    cache_write_tokens: bill.cache_write_tokens,
                    reasoning_tokens: bill.reasoning_tokens,
                    cost: Some(bill.cost),
                    generation_id: bill.generation_id.clone(),
                    origin: None,
                }],
                Vec::new(),
            ),
            Settlement::Rejected(id) => (Vec::new(), vec![id.clone()]),
        };
        crate::session::SessionEvent::UsageRecorded {
            input_tokens: None,
            output_tokens: None,
            requests: Vec::new(),
            unaccounted: false,
            lines_added: 0,
            lines_removed: 0,
            settled,
            rejected,
        }
    }
}

/// One question: where, with which key, about which generation.
type Question = Pin<Box<dyn Future<Output = Lookup> + Send>>;

/// How a question is asked: the Gateway's route in the binary, a script in a
/// test.
type Asker = Arc<dyn Fn(String, String, String) -> Question + Send + Sync>;

/// The runtime's side of the asking: which generations are being asked about,
/// and where the answers arrive.
pub(crate) struct Reconciler {
    ask: Asker,
    between_rounds: Duration,
    /// Set to stop every thread started since the last [`Self::forget`].
    cancel: Arc<AtomicBool>,
    /// Generation ids a live thread is asking about; a thread takes its own
    /// out on the way out, so an id it gave up on can be asked again.
    in_flight: Arc<Mutex<HashSet<String>>>,
    /// Every id ever asked about for the session in front of the user, so a
    /// start is made for a new pending request and not again for an old one
    /// (upstream restarts on a new deferred request, `:2054-2062`).
    asked: HashSet<String>,
    done: UnboundedSender<Answered>,
    answers: UnboundedReceiver<Answered>,
}

impl Reconciler {
    /// Asks the Gateway at a generation's origin ([`crate::gateway::generation::lookup`]).
    pub(crate) fn new() -> Self {
        Self::asking(
            Arc::new(|origin: String, secret: String, id: String| -> Question {
                Box::pin(
                    async move { crate::gateway::generation::lookup(&origin, &secret, &id).await },
                )
            }),
            BETWEEN_ROUNDS,
        )
    }

    fn asking(ask: Asker, between_rounds: Duration) -> Self {
        let (done, answers) = unbounded_channel();
        Self {
            ask,
            between_rounds,
            cancel: Arc::new(AtomicBool::new(false)),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            asked: HashSet::new(),
            done,
            answers,
        }
    }

    /// A reconciler whose questions `answer` answers, a round apart by
    /// `between_rounds`.
    #[cfg(test)]
    pub(crate) fn answering(
        answer: impl Fn(&str, &str, &str) -> Lookup + Send + Sync + 'static,
        between_rounds: Duration,
    ) -> Self {
        let answer = Arc::new(answer);
        Self::asking(
            Arc::new(
                move |origin: String, secret: String, id: String| -> Question {
                    let lookup = answer(&origin, &secret, &id);
                    Box::pin(async move { lookup })
                },
            ),
            between_rounds,
        )
    }

    /// Starts asking about `session`'s pending requests with `secret`, when
    /// one of them has not been asked about before.
    ///
    /// Every pending request no thread is asking about goes into the start,
    /// as upstream's restarted thread re-reads every pending one
    /// (`reconcilePendingBlocking`, `:3337-3348`). `false` when nothing was
    /// started.
    pub(crate) fn ask(&mut self, session: &str, pending: &[&RequestUsage], secret: &str) -> bool {
        let questions: Vec<(String, String)> = pending
            .iter()
            .filter_map(|request| Some((request.generation_id.clone()?, request.origin.clone()?)))
            .collect();
        if questions.iter().all(|(id, _)| self.asked.contains(id)) {
            return false;
        }
        let questions: Vec<(String, String)> = {
            let mut in_flight = self
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            questions
                .into_iter()
                .filter(|(id, _)| in_flight.insert(id.clone()))
                .collect()
        };
        self.asked
            .extend(questions.iter().map(|(id, _)| id.clone()));
        if questions.is_empty() {
            return false;
        }
        let job = Job {
            session: session.to_string(),
            secret: secret.to_string(),
            questions,
            ask: Arc::clone(&self.ask),
            between_rounds: self.between_rounds,
            cancel: Arc::clone(&self.cancel),
            in_flight: Arc::clone(&self.in_flight),
            done: self.done.clone(),
        };
        let ids: Vec<String> = job.questions.iter().map(|(id, _)| id.clone()).collect();
        let started = super::signals::spawn_unsignalled(THREAD_NAME, move || job.run()).is_ok();
        if !started {
            // Nothing is asking, so nothing is in flight; the requests stay
            // pending for the next start.
            let mut in_flight = self
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for id in &ids {
                in_flight.remove(id);
                self.asked.remove(id);
            }
        }
        started
    }

    /// The next answer. Never resolves while none has arrived, so it is an arm
    /// a `select!` can always carry.
    pub(crate) async fn next(&mut self) -> Answered {
        match self.answers.recv().await {
            Some(answered) => answered,
            // `self` holds a sender, so the channel cannot close.
            None => std::future::pending().await,
        }
    }

    /// Stops asking about the session that was in front of the user: it was
    /// dropped, replaced or ended. What was not answered stays pending in its
    /// log, and an answer already on its way is told apart by its session.
    pub(crate) fn forget(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.in_flight = Arc::new(Mutex::new(HashSet::new()));
        self.asked.clear();
    }
}

impl Drop for Reconciler {
    /// Stops every thread at its next look at the flag. None is waited for:
    /// none can take a signal (the module's own note), and a question in
    /// flight is one whose generation stays pending in the log.
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl std::fmt::Debug for Reconciler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reconciler")
            .field("asked", &self.asked)
            .finish_non_exhaustive()
    }
}

/// One start's questions, and everything its thread needs.
struct Job {
    session: String,
    secret: String,
    /// Generation id and origin, for every request still to be settled.
    questions: Vec<(String, String)>,
    ask: Asker,
    between_rounds: Duration,
    cancel: Arc<AtomicBool>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    done: UnboundedSender<Answered>,
}

impl Job {
    /// Asks every question, round after round, until each is answered, none
    /// is worth asking again, the rounds run out, or the start is cancelled
    /// (`reconcilePendingBlocking`, `session_usage.zig:3322-3423`).
    fn run(mut self) {
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            self.rounds(&runtime);
            // A question dropped mid-flight may have a resolver thread still
            // in `getaddrinfo`; it is not waited for.
            runtime.shutdown_background();
        }
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (id, _) in &self.questions {
            in_flight.remove(id);
        }
    }

    fn rounds(&mut self, runtime: &tokio::runtime::Runtime) {
        let mut open: Vec<(String, String)> = self.questions.clone();
        for round in 0..ROUNDS {
            let mut again = false;
            let mut still_open = Vec::new();
            for (id, origin) in open {
                let question = (self.ask)(origin.clone(), self.secret.clone(), id.clone());
                let Some(lookup) = runtime.block_on(unless_cancelled(question, &self.cancel))
                else {
                    return;
                };
                let settlement = match lookup {
                    Lookup::Found(bill) => Settlement::Billed(bill),
                    Lookup::Reject => Settlement::Rejected(id),
                    // Asked again next round, with the rest.
                    Lookup::Retry => {
                        again = true;
                        still_open.push((id, origin));
                        continue;
                    }
                    // Kept pending, and asked again only alongside one that
                    // is retried (`.preserve_pending`, `:3376-3383`).
                    Lookup::Preserve => {
                        still_open.push((id, origin));
                        continue;
                    }
                };
                // Nobody left to tell is a runtime that has ended, and the
                // generation stays pending in the log.
                let _ = self.done.send(Answered {
                    session: self.session.clone(),
                    settlement,
                });
            }
            if !again || round + 1 == ROUNDS {
                return;
            }
            open = still_open;
            if !wait_unless_cancelled(self.between_rounds, &self.cancel) {
                return;
            }
        }
    }
}

/// `question`'s answer, or `None` once `cancel` is set.
async fn unless_cancelled(question: Question, cancel: &AtomicBool) -> Option<Lookup> {
    if cancel.load(Ordering::Acquire) {
        return None;
    }
    let watch = async {
        while !cancel.load(Ordering::Acquire) {
            tokio::time::sleep(CANCEL_POLL).await;
        }
    };
    tokio::select! {
        lookup = question => Some(lookup),
        () = watch => None,
    }
}

/// Waits `length` in slices, and says whether it was not cancelled.
fn wait_unless_cancelled(length: Duration, cancel: &AtomicBool) -> bool {
    let deadline = std::time::Instant::now() + length;
    while std::time::Instant::now() < deadline {
        if cancel.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(CANCEL_POLL.min(length));
    }
    !cancel.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    const FIRST: &str = "gen_01JABCDEFGHJKMNPQRSTVWXYZ1";
    const SECOND: &str = "gen_01JABCDEFGHJKMNPQRSTVWXYZ2";

    fn pending(id: &str) -> RequestUsage {
        RequestUsage {
            at_ms: 1_000,
            duration_ms: 700,
            model: "fake/model-a".to_string(),
            input_tokens: 10,
            output_tokens: 2,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: None,
            cost: None,
            generation_id: Some(id.to_string()),
            origin: Some("http://127.0.0.1:4100".to_string()),
        }
    }

    fn bill(id: &str) -> Billing {
        Billing {
            at_ms: 900,
            model: "fake/model-a".to_string(),
            generation_id: Some(id.to_string()),
            cost: 0.0125,
            input_tokens: 12,
            output_tokens: 3,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: None,
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// The next answer, or a failed case after a few seconds.
    fn answer(runtime: &tokio::runtime::Runtime, reconciler: &mut Reconciler) -> Answered {
        runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(5), reconciler.next()).await
            })
            .expect("an answer")
    }

    #[test]
    fn a_bill_not_written_yet_is_asked_for_again_and_arrives_for_its_session() {
        let asks = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asks);
        let mut reconciler = Reconciler::answering(
            move |origin, secret, id| {
                assert_eq!((origin, secret), ("http://127.0.0.1:4100", "key"));
                // Not written yet the first time it is asked.
                if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                    Lookup::Retry
                } else {
                    Lookup::Found(bill(id))
                }
            },
            Duration::from_millis(1),
        );
        let request = pending(FIRST);
        assert!(reconciler.ask("s1", &[&request], "key"));
        let runtime = runtime();
        assert_eq!(
            answer(&runtime, &mut reconciler),
            Answered {
                session: "s1".to_string(),
                settlement: Settlement::Billed(bill(FIRST)),
            }
        );
        assert_eq!(asks.load(Ordering::SeqCst), 2);
        // Asked about once, it is not started again for the same request.
        assert!(!reconciler.ask("s1", &[&request], "key"));
    }

    #[test]
    fn a_bill_that_will_never_be_known_is_rejected_and_one_the_key_may_not_read_is_kept() {
        let mut reconciler = Reconciler::answering(
            |_, _, id| {
                if id == FIRST {
                    Lookup::Reject
                } else {
                    Lookup::Preserve
                }
            },
            Duration::from_millis(1),
        );
        let (first, second) = (pending(FIRST), pending(SECOND));
        assert!(reconciler.ask("s1", &[&first, &second], "key"));
        let runtime = runtime();
        assert_eq!(
            answer(&runtime, &mut reconciler).settlement,
            Settlement::Rejected(FIRST.to_string())
        );
        // Nothing is said about the one kept pending.
        assert!(runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_millis(100), reconciler.next()).await
            })
            .is_err());
    }

    #[test]
    fn the_rounds_run_out_and_the_request_stays_pending() {
        let asks = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asks);
        let mut reconciler = Reconciler::answering(
            move |_, _, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                Lookup::Retry
            },
            Duration::from_millis(1),
        );
        assert!(reconciler.ask("s1", &[&pending(FIRST)], "key"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while reconciler.in_flight.lock().unwrap().contains(FIRST) {
            assert!(Instant::now() < deadline, "the thread never gave up");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(asks.load(Ordering::SeqCst), ROUNDS);
    }

    #[test]
    fn a_forgotten_session_stops_being_asked_about_at_once() {
        let asks = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asks);
        let mut reconciler = Reconciler::answering(
            move |_, _, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                Lookup::Retry
            },
            Duration::from_secs(60),
        );
        assert!(reconciler.ask("s1", &[&pending(FIRST)], "key"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while asks.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "never asked");
            std::thread::sleep(Duration::from_millis(1));
        }
        let in_flight = Arc::clone(&reconciler.in_flight);
        reconciler.forget();
        // The thread was a minute from its next round, and leaves now.
        let deadline = Instant::now() + Duration::from_secs(1);
        while in_flight.lock().unwrap().contains(FIRST) {
            assert!(Instant::now() < deadline, "the thread kept waiting");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(asks.load(Ordering::SeqCst), 1);
        // The next session's requests are its own to ask about.
        assert!(reconciler.ask("s2", &[&pending(FIRST)], "key"));
    }

    /// The question is asked on the asking thread, so the asker reads that
    /// thread's mask: every owned signal blocked, whoever started it.
    #[test]
    fn the_asking_thread_can_take_no_owned_signal() {
        let mut reconciler = Reconciler::answering(
            // A thread that could take one rejects the bill, which fails the
            // case below.
            |_, _, id| {
                if super::super::signals::owned_blocked_here()
                    .iter()
                    .all(|(_, blocked)| *blocked)
                {
                    Lookup::Found(bill(id))
                } else {
                    Lookup::Reject
                }
            },
            Duration::from_millis(1),
        );
        assert!(reconciler.ask("s1", &[&pending(FIRST)], "key"));
        let runtime = runtime();
        assert_eq!(
            answer(&runtime, &mut reconciler).settlement,
            Settlement::Billed(bill(FIRST))
        );
    }

    #[test]
    fn an_answer_is_the_frame_that_settles_its_request() {
        let billed = Answered {
            session: "s1".to_string(),
            settlement: Settlement::Billed(bill(FIRST)),
        };
        let crate::session::SessionEvent::UsageRecorded {
            requests,
            settled,
            rejected,
            input_tokens,
            ..
        } = billed.frame()
        else {
            panic!("not a usage frame");
        };
        assert!(requests.is_empty() && rejected.is_empty());
        assert_eq!(input_tokens, None);
        assert_eq!(settled[0].cost, Some(0.0125));
        assert_eq!(settled[0].generation_id.as_deref(), Some(FIRST));
        assert!(!settled[0].pending());
    }
}
