//! The prompt history's half that only a session has: whether submitted lines
//! are kept, and the thread that writes them down.
//!
//! What a record is and how the file is kept are
//! [`crate::session::prompt_history`]'s. This module is **where the write
//! waits**: an append takes the file's lock, reads its tail and ends in an
//! `fsync`, and the UI thread sits in `pselect(2)` holding the terminal -- so a
//! slow or networked home would be a frozen band, and on any home it is a
//! delay between a submitted line and the frame that answers it. So appends go
//! to one writer thread, **one** so that lines land in the order they were
//! submitted, and a failure comes back on a channel the UI reads once a turn
//! of its loop -- [`super::sound`]'s shape for `/sound`'s saves, for the same
//! reasons.
//!
//! Upstream appends on the thread that submitted (`recordAcceptedInput`,
//! `input_submit_runtime.zig:1246-1281`); what the user sees is the same --
//! the line is in the file and, after a failure, one warning -- except that
//! the warning reads under whatever the submission wrote rather than before it
//! settled.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use crate::session::prompt_history::PromptHistory;

/// The name the writer thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-history";

/// One line to append.
#[derive(Debug)]
struct Line {
    timestamp_ms: u64,
    text: String,
}

/// The writer thread, once there is one.
#[derive(Debug)]
struct Writer {
    lines: Sender<Line>,
    thread: JoinHandle<()>,
}

/// Whether submitted lines are kept, where, and the thread that keeps them
/// (`PromptHistoryRuntime`, `prompt_history_runtime.zig:24-31`).
#[derive(Debug)]
pub(crate) struct Recall {
    /// `/settings`' `Prompt history`: the profile's `prompt_history.enabled`
    /// at launch, and every change since. Off, a submitted line is kept
    /// nowhere -- not in the file and not in this session's recall either
    /// (`recordComposerHistory`'s guard, `input_submit_runtime.zig:1327-1329`).
    pub(crate) enabled: bool,
    /// `~/.xfx/history.jsonl`, or `None` for a session with no profile -- and
    /// for a shell a test built, unless the test hands it one.
    store: Option<PromptHistory>,
    /// The workspace a line is filed under, as a record spells it, or `None`
    /// when its path is not UTF-8 and so cannot be one (`validateWorkspaceRoot`,
    /// `prompt_history_store.zig:570-579`).
    workspace: Option<String>,
    /// Started on the first line kept: a session that submits nothing has no
    /// thread for it.
    writer: Option<Writer>,
    /// Cloned into the writer, which sends the error's words for each append
    /// that failed.
    done: Sender<String>,
    failed: Receiver<String>,
    /// Whether a failure has been said since the last append that worked:
    /// one warning per run of failures (`append_warning_active`,
    /// `prompt_history_runtime.zig:133-140`).
    warned: bool,
    /// Appends handed to the writer whose outcome has not come back.
    #[cfg(test)]
    in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Recall {
    pub(crate) fn new(
        enabled: bool,
        store: Option<PromptHistory>,
        workspace: Option<String>,
    ) -> Self {
        let (done, failed) = mpsc::channel();
        Self {
            enabled,
            store,
            workspace,
            writer: None,
            done,
            failed,
            warned: false,
            #[cfg(test)]
            in_flight: std::sync::Arc::default(),
        }
    }

    /// The workspace's saved lines, oldest first, for a launch's recall
    /// (`loadAndInstallPromptHistory`, `input_history_runtime.zig:20-28`):
    /// none while the history is off, and none -- said nowhere -- when the
    /// file cannot be read, because a recall that starts empty is still a
    /// session that works. Read here, at launch, before anything is typed.
    pub(crate) fn saved_lines(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let (Some(store), Some(workspace)) = (self.store.as_ref(), self.workspace.as_deref())
        else {
            return Vec::new();
        };
        store
            .load_recent(workspace, crate::session::prompt_history::LOAD_LIMIT)
            .unwrap_or_default()
    }

    /// Hands one submitted line to the writer, when the history is on and has
    /// a file. Answers the words of a failure that has to be said **now** --
    /// the thread could not be started or is gone -- and `None` otherwise; a
    /// failed append is said when [`Self::failure`] reads it.
    pub(crate) fn keep(&mut self, text: String) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let (Some(store), Some(workspace)) = (self.store.clone(), self.workspace.clone()) else {
            return None;
        };
        if self.writer.is_none() {
            match spawn_writer(
                store,
                workspace,
                self.done.clone(),
                #[cfg(test)]
                std::sync::Arc::clone(&self.in_flight),
            ) {
                Ok(writer) => self.writer = Some(writer),
                Err(reason) => return self.warn(reason),
            }
        }
        let writer = self.writer.as_ref().expect("the writer was just started");
        #[cfg(test)]
        self.in_flight
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let line = Line {
            timestamp_ms: u64::try_from(crate::session::event::system_now_ms()).unwrap_or(0),
            text,
        };
        if writer.lines.send(line).is_err() {
            #[cfg(test)]
            self.in_flight
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            // Only a panic in the thread ends it early; it is not started
            // again, because whatever killed it would kill the next.
            return self.warn("the history writer stopped".to_string());
        }
        None
    }

    /// The words of the next failed append that has to be said, or `None`.
    /// The first of a run is said and the rest are not; an append that worked
    /// since ends the run ([`Self::kept`]).
    pub(crate) fn failure(&mut self) -> Option<String> {
        while let Ok(reason) = self.failed.try_recv() {
            if reason.is_empty() {
                self.warned = false;
                continue;
            }
            if let Some(said) = self.warn(reason) {
                return Some(said);
            }
        }
        None
    }

    fn warn(&mut self, reason: String) -> Option<String> {
        if self.warned {
            return None;
        }
        self.warned = true;
        Some(reason)
    }

    /// Hands the session a file a test chose.
    #[cfg(test)]
    pub(crate) fn keep_in(&mut self, store: PromptHistory) {
        self.store = Some(store);
    }

    /// The workspace lines are filed under.
    #[cfg(test)]
    pub(crate) fn workspace(&self) -> Option<&str> {
        self.workspace.as_deref()
    }

    /// Waits until every line handed to the writer has been written or has
    /// failed -- the one thing a test about the file has to know.
    #[cfg(test)]
    pub(crate) fn wait_for_writes(&self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while self.in_flight.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the history writer never finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

impl Drop for Recall {
    /// Lets the lines in flight land before the session is gone --
    /// [`super::sound`]'s `Drop`, for the reasons it gives.
    fn drop(&mut self) {
        if let Some(Writer { lines, thread }) = self.writer.take() {
            drop(lines);
            let _ = thread.join();
        }
    }
}

/// Starts the one thread that appends this session's lines, in order. It
/// sends an empty string for an append that worked and the error's words for
/// one that did not.
fn spawn_writer(
    store: PromptHistory,
    workspace: String,
    done: Sender<String>,
    #[cfg(test)] in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> Result<Writer, String> {
    let (lines, received) = mpsc::channel::<Line>();
    let thread = super::signals::spawn_unsignalled(THREAD_NAME, move || {
        for line in received {
            let outcome = store
                .append(line.timestamp_ms, &workspace, &line.text)
                .map_or_else(|err| err.to_string(), |_| String::new());
            // Nobody left to tell is a session that has ended.
            let _ = done.send(outcome);
            #[cfg(test)]
            in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    })
    .map_err(|err| format!("no thread to save input history with: {err}"))?;
    Ok(Writer { lines, thread })
}
