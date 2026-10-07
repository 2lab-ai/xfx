//! `/credits`, asked at once and answered when the Gateway answers.
//!
//! Upstream asks on the spot -- `commandShowCredits` blocks its UI thread on
//! the request and prints what comes back (`vercel-labs/fx@c1db9191
//! src/core/app/app_commands.zig:2052-2082`) -- so a `/credits` typed while a
//! turn runs is answered while the turn runs, a full queue does not refuse it,
//! and a Ctrl-C meant for the turn does not take it away. Here the request goes
//! to a short-lived thread of its own, started the moment the command is read,
//! and the answer comes back on a channel the UI reads once a turn of its loop,
//! the way `/copy`'s outcomes do ([`super::copy`]).
//!
//! Not the work queue: its two places belong to prompts, and its runtime
//! thread may be a minute into a model request. Not a control message the
//! runtime answers between steps either, for the same minute.
//!
//! **The thread is started with every owned signal blocked**
//! ([`super::signals::spawn_unsignalled`]), so it is never waited for: quitting
//! while the Gateway is slow to answer leaves at once, and the thread ends on
//! its own -- at its timeout, or with the process.

use std::sync::mpsc::{self, Receiver, Sender};

use crate::credits::{Answer, Question};

/// The name each asking thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-credits";

/// What a failed start says: no thread is a question nobody asked.
const NO_THREAD: &str = "failed to fetch credits from gateway";

/// The questions in flight, and the answers that have come back.
#[derive(Debug)]
pub(crate) struct Asks {
    done: Sender<Answer>,
    answers: Receiver<Answer>,
    /// The questions a set built by a test was asked, in order; such a set
    /// starts no thread, so what it answers is only what the test delivers.
    #[cfg(test)]
    asked: Option<Vec<Question>>,
}

impl Asks {
    pub(crate) fn new() -> Self {
        let (done, answers) = mpsc::channel();
        Self {
            done,
            answers,
            #[cfg(test)]
            asked: None,
        }
    }

    /// A set that asks nobody: it keeps each question, and answers only with
    /// what [`Self::deliver`] hands it.
    #[cfg(test)]
    pub(crate) fn by_hand() -> Self {
        let mut asks = Self::new();
        asks.asked = Some(Vec::new());
        asks
    }

    /// How many questions a [`Self::by_hand`] set was asked.
    #[cfg(test)]
    pub(crate) fn asked(&self) -> usize {
        self.asked.as_ref().map_or(0, Vec::len)
    }

    /// Stands in for a thread, for the cases about what the UI does with an
    /// answer.
    #[cfg(test)]
    pub(crate) fn deliver(&self, answer: Answer) {
        let _ = self.done.send(answer);
    }

    /// Starts asking `question`, and returns without waiting. Every start is
    /// its own question, answered in the order the answers arrive.
    ///
    /// `Err` with the answer to show when no thread could be started for it.
    pub(crate) fn start(&mut self, question: Question) -> Result<(), Answer> {
        #[cfg(test)]
        if let Some(asked) = self.asked.as_mut() {
            asked.push(question);
            return Ok(());
        }
        let done = self.done.clone();
        super::signals::spawn_unsignalled(THREAD_NAME, move || {
            let answer = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => {
                    let answer = runtime.block_on(crate::credits::ask(&question));
                    // A resolver still in `getaddrinfo` is not waited for.
                    runtime.shutdown_background();
                    answer
                }
                Err(_) => Answer::Failed(NO_THREAD.to_string()),
            };
            // Nobody left to tell is a session that has ended.
            let _ = done.send(answer);
        })
        .map(drop)
        .map_err(|_| Answer::Failed(NO_THREAD.to_string()))
    }

    /// An answer that has arrived, made inert, or `None` when none has since
    /// the last call.
    ///
    /// Inert here because the fields and an error body are the Gateway's text,
    /// about to be painted: every control character but a line break becomes
    /// a space ([`super::bridge::inert`]). A line break is a field's own
    /// continuation row.
    pub(crate) fn finished(&self) -> Option<Answer> {
        let inert = |text: String| super::bridge::inert(&text).into_owned();
        self.answers.try_recv().ok().map(|answer| match answer {
            Answer::Failed(reason) => Answer::Failed(inert(reason)),
            Answer::Credits {
                balance,
                used,
                plan,
            } => Answer::Credits {
                balance: balance.map(inert),
                used: used.map(inert),
                plan: plan.map(inert),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_made_inert_on_the_way_to_the_screen() {
        let asks = Asks::by_hand();
        asks.deliver(Answer::Credits {
            balance: Some("4\u{1b}[2J2".to_string()),
            used: Some("1\n2".to_string()),
            plan: None,
        });
        assert_eq!(
            asks.finished(),
            Some(Answer::Credits {
                balance: Some("4 [2J2".to_string()),
                used: Some("1\n2".to_string()),
                plan: None,
            })
        );
        asks.deliver(Answer::Failed("HTTP 500: \u{7}".to_string()));
        assert_eq!(
            asks.finished(),
            Some(Answer::Failed("HTTP 500:  ".to_string()))
        );
        assert_eq!(asks.finished(), None);
    }
}
