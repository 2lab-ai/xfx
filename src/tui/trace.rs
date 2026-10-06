//! `/trace` on this surface: the report's slow half, off the thread that holds
//! the terminal.
//!
//! What a report says and how it is written and copied are
//! [`crate::trace`]'s; what this module adds is **where the waiting happens**,
//! for the reason `super::copy` gives for `/copy`. Upstream builds, writes,
//! syncs and copies on the thread the keystroke arrived on, behind a
//! `Preparing trace...` notice it replaces when it is done
//! (`handleTraceReport`, `app_commands.zig:624-660`) -- and a file copy may take
//! its tool up to five seconds. Here that thread is the UI thread, which must
//! go on painting the band. So the UI takes what only it knows at the moment
//! the command is typed -- its screen, its counters, its full transcript, the
//! runtime's last publication -- and hands it to a thread of its own
//! ([`Traces::start`]), which reads the session's log back from disk, builds
//! the report, writes it, copies it, and says how that went on a channel the UI
//! reads once a turn of its loop ([`Traces::finished`]).
//!
//! The log is read back rather than asked for: the runtime thread holds it,
//! and asking that thread would wait behind whatever turn it is running.
//! Every event is published as it is written
//! (`crate::session::SessionRecorder::commit`), so the log read back is the
//! log as of its last event; a report taken mid-turn says so under
//! `## Problems`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::clipboard::Clipboard;
use crate::output::StatusSnapshot;
use crate::session::{Selector, SessionStore};
use crate::trace::{Disposition, Entry, Report, Screen, Session};

use super::worker::TraceFacts;

/// The name each report's thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-trace";

/// Everything one report is made of that the UI thread knows, taken at the
/// moment `/trace` was typed.
pub(crate) struct Job {
    pub(crate) status: StatusSnapshot,
    pub(crate) facts: TraceFacts,
    pub(crate) effort: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) metrics: String,
    pub(crate) outstanding: usize,
    pub(crate) timeline: Vec<Entry>,
    pub(crate) network: Vec<Entry>,
}

impl Job {
    /// The report, with the session's log read back from disk.
    fn render(&self, now_ms: i64) -> String {
        let read = self.facts.session.as_ref().map(|(profile, id)| {
            let store = SessionStore::read_only(profile);
            let detail = store
                .detail(&Selector::Id(id.clone()), Path::new(&self.status.workspace))
                .map_err(|err| err.to_string());
            (id, store.session_dir(id), detail)
        });
        let session = match &read {
            None => Session::None,
            Some((_, dir, Ok(detail))) => Session::Read {
                dir,
                state: &detail.state,
                failure: None,
            },
            Some((id, dir, Err(reason))) => Session::Unreadable {
                id: id.as_str(),
                dir,
                reason,
            },
        };
        Report {
            generated_ms: now_ms,
            status: &self.status,
            effort: Some(&self.effort),
            settings: &self.facts.settings,
            session,
            screen: Some(Screen {
                cols: self.cols,
                rows: self.rows,
                metrics: &self.metrics,
                outstanding: self.outstanding,
                timeline: &self.timeline,
                network: &self.network,
            }),
        }
        .render()
    }
}

/// The reports this session has started, and how each of them went.
///
/// A session that ends with one still in hand joins its thread
/// ([`super::copy::Errands`]): the report is written whole -- the build and the
/// write are not stopped, and are bounded by the disk -- and the file copy, the
/// one wait that is not, is given a moment and then given up. So a `/trace`
/// followed at once by a quit leaves its whole report, and no thread of xfx's
/// outlives the terminal's restore.
#[derive(Debug)]
pub(crate) struct Traces {
    clipboard: Clipboard,
    /// Where each report is written.
    dir: PathBuf,
    /// The threads the reports run on.
    errands: super::copy::Errands,
    /// Cloned into each report's thread, which sends one outcome and ends.
    done: Sender<Disposition>,
    outcomes: Receiver<Disposition>,
}

impl Traces {
    pub(crate) fn new(clipboard: Clipboard, dir: PathBuf) -> Self {
        let (done, outcomes) = mpsc::channel();
        Self {
            clipboard,
            dir,
            errands: super::copy::Errands::default(),
            done,
            outcomes,
        }
    }

    /// Starts one report, and returns without waiting.
    ///
    /// `false` when no thread could be started for it, which is a report that
    /// was never written and will never say so on the channel.
    pub(crate) fn start(&self, job: Job) -> bool {
        let clipboard = self.clipboard.clone();
        let dir = self.dir.clone();
        let done = self.done.clone();
        self.errands.spawn(THREAD_NAME, move |stop| {
            let now = crate::session::event::system_now_ms();
            let report = job.render(now);
            // Nobody left to tell is a session that has ended, and the
            // report was written for it all the same.
            let _ = done.send(crate::trace::deliver(&report, &dir, &clipboard, now, stop));
        })
    }

    /// How one report that has finished went, or `None` when none has
    /// finished since the last call -- in the order they finished.
    pub(crate) fn finished(&self) -> Option<Disposition> {
        self.outcomes.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use crate::clipboard::fake::FakeTool;
    use crate::config::{Environment, RuntimeConfig};

    fn job(facts: TraceFacts) -> Job {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), Default::default()),
            workspace.path(),
        )
        .expect("a configuration");
        Job {
            status: crate::interactive::status_snapshot(&config, &config.model, None),
            facts,
            effort: "auto".to_string(),
            cols: 120,
            rows: 40,
            metrics: "ansi_bytes=1".to_string(),
            outstanding: 0,
            timeline: Vec::new(),
            network: Vec::new(),
        }
    }

    fn outcome(traces: &Traces) -> Disposition {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(done) = traces.finished() {
                return done;
            }
            assert!(Instant::now() < deadline, "the report never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_report_is_started_and_answered_later_with_the_file_it_wrote() {
        let tool = FakeTool::keeping();
        let dir = tempfile::tempdir().expect("a report directory");
        let traces = Traces::new(tool.clipboard(), dir.path().to_path_buf());
        assert!(traces.start(job(TraceFacts::default())));
        let done = outcome(&traces);
        let written: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .expect("the directory")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        assert_eq!(written.len(), 1, "{written:?}");
        let expected = if cfg!(target_os = "macos") {
            Disposition::Copied(written[0].clone())
        } else {
            Disposition::Saved(written[0].clone())
        };
        assert_eq!(done, expected);
        let report = std::fs::read_to_string(&written[0]).expect("the report");
        assert!(report.starts_with("# xfx trace\n"), "{report}");
        assert!(report.contains("session: none"), "{report}");
        assert!(
            report.contains("\n## Transcript Timeline\n(empty)\n"),
            "{report}"
        );
    }

    #[test]
    fn the_session_is_read_back_from_its_log_on_disk() {
        // The runtime publishes where its session is; the report's thread
        // reads the log there, as the runtime last published it.
        let tool = FakeTool::keeping();
        let dir = tempfile::tempdir().expect("a report directory");
        let home = tempfile::tempdir().expect("a home");
        let store = SessionStore::open(&home.path().join(".xfx")).expect("a store");
        let session = store
            .create(
                crate::session::SessionId::generate(),
                crate::session::NewSession {
                    origin_workspace_root: home.path().to_path_buf(),
                    workspace_root: home.path().to_path_buf(),
                    model: "fake/model-a".to_string(),
                    permission_mode: crate::config::PermissionMode::Auto,
                },
            )
            .expect("a session");
        let mut recorder = crate::session::SessionRecorder::new(store.clone(), session);
        recorder.commit(crate::session::SessionEvent::UserMessage {
            text: "say something".to_string(),
        });
        assert_eq!(recorder.failure(), None);
        let traces = Traces::new(tool.clipboard(), dir.path().to_path_buf());
        assert!(traces.start(job(TraceFacts {
            session: Some((store.profile_dir().to_path_buf(), recorder.id().clone())),
            settings: Default::default(),
        })));
        assert!(!matches!(outcome(&traces), Disposition::Unavailable));
        let path = std::fs::read_dir(dir.path())
            .expect("the directory")
            .next()
            .expect("a report")
            .expect("an entry")
            .path();
        let report = std::fs::read_to_string(path).expect("the report");
        for line in [
            format!("session_id: {}", recorder.id()),
            format!(
                "session_dir: {}",
                store.session_dir(recorder.id()).display()
            ),
            "history_turns: 1".to_string(),
            "    say something".to_string(),
        ] {
            assert!(report.lines().any(|row| row == line), "{line}\n{report}");
        }
        // Its newest turn has no conclusion: it may be running, and is not
        // called lost.
        assert!(!report.contains("has no conclusion recorded"), "{report}");
    }

    #[test]
    fn a_session_whose_log_cannot_be_read_is_a_problem_in_the_report_and_not_a_failure() {
        let tool = FakeTool::keeping();
        let dir = tempfile::tempdir().expect("a report directory");
        let profile = tempfile::tempdir().expect("an empty profile");
        let traces = Traces::new(tool.clipboard(), dir.path().to_path_buf());
        let id = crate::session::SessionId::generate();
        assert!(traces.start(job(TraceFacts {
            session: Some((profile.path().to_path_buf(), id.clone())),
            settings: Default::default(),
        })));
        assert!(!matches!(outcome(&traces), Disposition::Unavailable));
        let path = std::fs::read_dir(dir.path())
            .expect("the directory")
            .next()
            .expect("a report")
            .expect("an entry")
            .path();
        let report = std::fs::read_to_string(path).expect("the report");
        assert!(
            report.contains(&format!("session_id: {id}")),
            "the report lost the session it could not read: {report}"
        );
        assert!(
            report.contains("- session log could not be read: "),
            "{report}"
        );
    }

    #[test]
    fn an_ending_session_joins_a_report_whole_and_gives_its_copy_up() {
        // Quit at once after `/trace`, with a clipboard tool that never
        // answers: the shell's drop joins the report's thread, the report is
        // on disk whole, and the tool is killed rather than waited for.
        let tool = FakeTool::keeping();
        tool.hang();
        let dir = tempfile::tempdir().expect("a report directory");
        let traces = Traces::new(tool.clipboard(), dir.path().to_path_buf());
        assert!(traces.start(job(TraceFacts::default())));
        let ending = Instant::now();
        drop(traces);
        let took = ending.elapsed();
        assert!(
            took < super::super::copy::GRACE + Duration::from_secs(10),
            "the end waited on the tool: {took:?}"
        );
        let written: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .expect("the directory")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        assert_eq!(written.len(), 1, "{written:?}");
        let report = std::fs::read_to_string(&written[0]).expect("the report");
        assert!(report.starts_with("# xfx trace\n"), "{report}");
        assert!(
            report.ends_with("\n## Transcript Timeline\n(empty)\n"),
            "not the whole report: {report}"
        );
        // A tool that got as far as saying who it is was killed; one the
        // stop reached sooner never got that far.
        if let Some(pid) = tool.hung() {
            assert!(!FakeTool::alive(pid), "the tool outlived the session");
        }
    }

    #[test]
    fn a_report_with_nowhere_to_be_written_is_unavailable() {
        let tool = FakeTool::keeping();
        let traces = Traces::new(tool.clipboard(), PathBuf::from(super::super::copy::NOWHERE));
        assert!(traces.start(job(TraceFacts::default())));
        assert_eq!(outcome(&traces), Disposition::Unavailable);
        assert_eq!(
            tool.copied_file(),
            None,
            "a report never written was copied"
        );
    }
}
