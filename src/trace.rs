//! `/trace`: a private diagnostic report about this session, written to a file
//! of its own and put on the clipboard as that file.
//!
//! Upstream's command (`handleTraceReport`, `vercel-labs/fx@c1db9191
//! src/core/app/app_commands.zig:624-660`) is four steps, and each is given
//! here once for both shells:
//!
//! - **Build** the report ([`Report::render`]): markdown, built from what this
//!   process knows about itself -- the versions, the configuration it resolved,
//!   the session log, the problems those record, and on the TUI what its screen
//!   was given. Upstream's section names where xfx has the fact
//!   (`buildTraceReport`, `:2229-2290`); the sections whose facts xfx does not
//!   have -- a `ps` snapshot, renderer events, title generation, compaction, a
//!   debug log tail -- are absent rather than written empty.
//! - **Write** it where upstream writes it ([`write_report`]): `$TMPDIR`, or
//!   `/tmp`, as `xfx-trace-<UTC>-<12 hex>.md`, created exclusively, owner-only,
//!   and synced (`writeTraceReportFile`, `:2183-2227`).
//! - **Copy** it as a file, not as text
//!   ([`crate::clipboard::Clipboard::copy_file`]).
//! - **Say** one sentence about how it went ([`Disposition::sentence`]), in
//!   upstream's words (`format_trace_notice`, `:75-90`).
//!
//! # What is never in it
//!
//! The report is private by design -- it carries prompts, paths, command
//! output and file snippets, and the sentence says to review it before sharing
//! -- but three things are kept out of it whatever the session held:
//!
//! - **xfx's own credential.** The report names it by the variable that
//!   supplied it (`auth: source=AI_GATEWAY_API_KEY`), the way `/status` does,
//!   and no input to [`Report`] carries the value.
//! - **What looks like someone else's.** Every piece of text that came from a
//!   person, a model, a tool or a settings file goes through [`mask_secrets`]:
//!   upstream's rules (`maskSecrets`, `src/core/shared/text_utils.zig:457-476`)
//!   and the credential shapes `scripts/check-no-secrets.sh` refuses to commit.
//!   A `KEY=${VAR}` assignment names a variable rather than holding a secret,
//!   and is left as written, as upstream leaves it.
//! - **A URL's credentials** ([`redact_url`], upstream's
//!   `redactUrlForDisplay`, `text_utils.zig:478-550`).
//!
//! Nothing is uploaded: the report is a file on this machine and a clipboard
//! entry naming it.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::clipboard::{Clipboard, Stop};
use crate::config::RuntimeConfig;
use crate::output::StatusSnapshot;
use crate::session::{DurableState, TurnConclusion, TurnStep};

/// What `/trace` says when the clipboard holds the report
/// (`app_commands.zig:84`).
pub const COPIED: &str = "Trace copied to clipboard. Review and redact it before sharing.";

/// What `/trace` says when the report could not be written at all
/// (`app_commands.zig:87`).
pub const UNAVAILABLE: &str = "Could not create trace.";

/// How a `/trace` ended, which is what decides its one sentence and its tone
/// (`TraceReportDisposition`, `app_commands.zig:67-72`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Written, and on the clipboard as a file.
    Copied(PathBuf),
    /// Written, and the clipboard refused it -- on macOS, the one system
    /// where upstream copies a file at all.
    CopyFailed(PathBuf),
    /// Written, on a system with no file clipboard to put it on.
    Saved(PathBuf),
    /// Not written.
    Unavailable,
}

impl Disposition {
    /// Upstream's sentence for it, exactly (`format_trace_notice`,
    /// `app_commands.zig:75-90`). The path is the file's own, which is how a
    /// person who could not paste it finds it.
    pub fn sentence(&self) -> String {
        match self {
            Self::Copied(_) => COPIED.to_string(),
            Self::CopyFailed(path) => format!(
                "Clipboard copy failed. Trace saved at {}. Review and redact it before sharing.",
                path.display()
            ),
            Self::Saved(path) => format!(
                "Trace saved at {}. Review and redact it before sharing.",
                path.display()
            ),
            Self::Unavailable => UNAVAILABLE.to_string(),
        }
    }

    /// Whether it is said as an error: a copy that failed and a report that
    /// was never written are, the other two are not (`app_commands.zig:655-658`).
    pub fn failed(&self) -> bool {
        matches!(self, Self::CopyFailed(_) | Self::Unavailable)
    }
}

/// Writes `report` under `dir` and puts the file on the clipboard, and says how
/// that went (`handleTraceReport`, `app_commands.zig:637-650`).
///
/// **Blocks** for the write's `fsync` and for the clipboard tool, up to its
/// deadline: the line shell calls it inline, the TUI from a thread of its own.
/// `stop` gives the copy up -- the write is never given up, so a report is
/// whole or absent ([`Stop`]).
pub fn deliver(
    report: &str,
    dir: &Path,
    clipboard: &Clipboard,
    now_ms: i64,
    stop: &Stop,
) -> Disposition {
    let Ok(path) = write_report(dir, report, now_ms) else {
        return Disposition::Unavailable;
    };
    if clipboard.copy_file_until(&path, stop) {
        Disposition::Copied(path)
    } else if cfg!(target_os = "macos") {
        Disposition::CopyFailed(path)
    } else {
        Disposition::Saved(path)
    }
}

/// Where a report is written: `$TMPDIR`, or `/tmp` without one
/// (`writeTraceReportFile`, `app_commands.zig:2183-2184`).
///
/// A `TMPDIR` that is set and empty is no directory at all, so it is `/tmp`
/// too, where upstream would write at the root of the filesystem.
pub fn report_dir(tmpdir: Option<&std::ffi::OsStr>) -> PathBuf {
    match tmpdir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from("/tmp"),
    }
}

/// How many names are tried before a write gives up, each with fresh random
/// hex (`writeTraceReportFile`'s `attempts < 8`, `app_commands.zig:2196`).
const NAME_ATTEMPTS: usize = 8;

/// Writes `contents` to a file of its own under `dir`, and returns its path.
///
/// The name is `xfx-trace-YYYY-MM-DD-HHMMSS-<12 hex>.md`, the time in UTC, as
/// upstream names its `fx-trace-` files (`app_commands.zig:2197-2206`). The
/// file is **owner-only from its first byte** (mode `0600` at creation, which
/// no umask can widen) and synced before its path is handed back. A name that
/// is taken is tried again with other hex, at most [`NAME_ATTEMPTS`] times.
///
/// **A report is whole or it is not there.** Upstream creates the report
/// under its own name and writes into it (`app_commands.zig:2207-2224`), so a
/// process killed mid-write leaves a truncated report where a person would
/// look for one. Here the bytes go to a hidden stage beside it
/// (`.<name>.partial`), created exclusively and synced, and only then is the
/// stage renamed onto the report's name -- **without replacing** anything
/// already there ([`publish`]). So neither an existing file nor a link
/// planted under the name is ever written through or replaced, and a kill at
/// any moment leaves either the whole report or no report: at worst a hidden
/// stage, never a report that stops short.
///
/// A stage that could not be written or published is removed again.
pub fn write_report(dir: &Path, contents: &str, now_ms: i64) -> io::Result<PathBuf> {
    let stamp = file_stamp(now_ms);
    write_named(
        dir,
        contents,
        || {
            format!(
                "xfx-trace-{stamp}-{}.md",
                &crate::session::new_identifier()[..12]
            )
        },
        before_publish,
    )
}

/// [`write_report`], with each name it tries asked of `name`, and `staged`
/// told of each stage once it is whole and synced, before it is published.
fn write_named(
    dir: &Path,
    contents: &str,
    mut name: impl FnMut() -> String,
    mut staged: impl FnMut(&Path),
) -> io::Result<PathBuf> {
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for _ in 0..NAME_ATTEMPTS {
        let name = name();
        let path = dir.join(&name);
        let stage = dir.join(format!(".{name}.partial"));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&stage) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last = err;
                continue;
            }
            Err(err) => return Err(err),
        };
        let written = file
            .write_all(contents.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        if let Err(err) = written {
            let _ = std::fs::remove_file(&stage);
            return Err(err);
        }
        staged(&stage);
        match publish(&stage, &path) {
            Ok(()) => return Ok(path),
            Err(err) => {
                let _ = std::fs::remove_file(&stage);
                if err.kind() != io::ErrorKind::AlreadyExists {
                    return Err(err);
                }
                last = err;
            }
        }
    }
    Err(last)
}

/// Gives the whole `stage` the report's name, failing with `AlreadyExists`
/// rather than replacing whatever is there.
///
/// One `rename` that refuses to replace (`RENAME_EXCL` on macOS,
/// `RENAME_NOREPLACE` on Linux), so the name appears with every byte behind it
/// at once. Where the filesystem has no such rename, a hard link -- which
/// refuses an existing name the same way -- and the stage's removal; a kill
/// between the two leaves the whole report and a hidden copy of it.
fn publish(stage: &Path, path: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use rustix::fs::{renameat_with, RenameFlags, CWD};
        use rustix::io::Errno;
        match renameat_with(CWD, stage, CWD, path, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            Err(err) if err == Errno::INVAL || err == Errno::NOSYS || err == Errno::OPNOTSUPP => {}
            Err(err) => return Err(err.into()),
        }
    }
    std::fs::hard_link(stage, path)?;
    let _ = std::fs::remove_file(stage);
    Ok(())
}

/// Where a build made to fail on purpose waits between a report's write and
/// its publication, so a test can kill the process there
/// (`XFX_TUI_FAULT=trace-before-publish`). Nothing in any other build.
#[cfg(all(unix, feature = "fault-injection"))]
fn before_publish(_stage: &Path) {
    use crate::tui::fault::{injected, Fault};
    if injected(Fault::TraceBeforePublish) {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

#[cfg(not(all(unix, feature = "fault-injection")))]
fn before_publish(_stage: &Path) {}

/// `YYYY-MM-DD-HHMMSS` for the file name, in UTC; a clock before 1970 is the
/// epoch, as upstream clamps it (`@max(..., 0)`, `app_commands.zig:2187`).
fn file_stamp(now_ms: i64) -> String {
    let (year, month, day, hour, minute, second, _) = utc(now_ms.max(0));
    format!("{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}")
}

/// `YYYY-MM-DDTHH:MM:SSZ`: the report's `generated:` line
/// (`app_commands.zig:2240-2252`).
fn generated_stamp(now_ms: i64) -> String {
    let (year, month, day, hour, minute, second, _) = utc(now_ms.max(0));
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// `[YYYY-MM-DDTHH:MM:SS.mmmZ]` for a moment the report lists, or upstream's
/// dashes for one that is not a moment (`writeTraceTimestampUtc`,
/// `app_commands.zig:3453-3473`).
fn moment(at_ms: i64) -> String {
    if at_ms <= 0 {
        return "[----------T--:--:--Z]".to_string();
    }
    let (year, month, day, hour, minute, second, millis) = utc(at_ms);
    format!("[{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z]")
}

/// A moment in UTC: year, month, day, hour, minute, second, millisecond.
fn utc(at_ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let seconds = at_ms.div_euclid(1000);
    let (year, month, day) = civil(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    (
        year,
        month,
        day,
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        at_ms.rem_euclid(1000),
    )
}

/// The calendar date `days` after 1970-01-01, proleptic Gregorian -- Howard
/// Hinnant's `civil_from_days`, the same arithmetic `std.time.epoch` does for
/// upstream. The full transcript's headers are dated by it too.
pub(crate) fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

// ---------------------------------------------------------------------------
// what the report is built from
// ---------------------------------------------------------------------------

/// The settings files and what they decided, as one configuration resolved
/// them -- the report's `## Configuration` and the configuration half of its
/// `## Problems`.
///
/// Facts *about* the configuration and never its values: which file, whether
/// it was there and read, which layer each setting came from, and what was
/// wrong with a file. The credential is not one of them -- it is named by its
/// variable in `## Current State`, from the status snapshot -- and a
/// diagnostic's own words are masked when they are written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `~/.xfx/settings.json` and how it stood, when there is a home.
    pub user: Option<(PathBuf, &'static str)>,
    /// `<workspace>/.xfx.json` and how it stood.
    pub project: Option<(PathBuf, &'static str)>,
    /// `name=layer` for each setting that has a source.
    pub sources: Vec<(&'static str, &'static str)>,
    /// Each configuration diagnostic, in the words `doctor` uses.
    pub diagnostics: Vec<String>,
}

impl Settings {
    /// What `config` says about its own files.
    pub fn of(config: &RuntimeConfig) -> Self {
        let standing = |present: bool, loaded: bool| match (present, loaded) {
            (_, true) => "loaded",
            (true, false) => "present, not loaded",
            (false, _) => "absent",
        };
        Self {
            user: config.user_settings_path.clone().map(|path| {
                (
                    path,
                    standing(config.user_settings_present, config.user_settings_loaded),
                )
            }),
            project: Some((
                config.project_settings_path.clone(),
                standing(
                    config.project_settings_present,
                    config.project_settings_loaded,
                ),
            )),
            sources: vec![
                ("model", config.sources.model.label()),
                ("permission_mode", config.sources.permission_mode.label()),
                ("max_agent_steps", config.sources.max_agent_steps.label()),
                ("provider", config.sources.provider.label()),
                ("llmux_url", config.sources.llmux_url.label()),
            ],
            diagnostics: config
                .diagnostics
                .iter()
                .map(crate::config::Diagnostic::detail)
                .collect(),
        }
    }
}

/// The session the report is about, as far as it could be read.
#[derive(Debug, Clone, Copy)]
pub enum Session<'a> {
    /// None has been opened: xfx opens one at the first prompt.
    None,
    /// Its log, replayed.
    Read {
        dir: &'a Path,
        state: &'a DurableState,
        /// What stopped the log being written, when something did -- the line
        /// shell's recorder knows; a log read back from disk does not.
        failure: Option<&'a str>,
    },
    /// One is open and its log could not be read back, and why.
    Unreadable {
        id: &'a str,
        dir: &'a Path,
        reason: &'a str,
    },
}

/// One thing the TUI's screen was given, as text: what its full transcript
/// (`C-o`) records, painted at a fixed width and stripped of colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub at_ms: i64,
    /// What kind of entry it is, in upstream's timeline words where xfx has
    /// one of the same kind: `user`, `assistant`, `semantic notice`, `raw`,
    /// and xfx's `tool` and `questions`.
    pub kind: &'static str,
    pub rows: Vec<String>,
    /// Whether it says something went wrong: an error notice, or a provider
    /// request that was refused.
    pub problem: bool,
}

/// What only the TUI knows: its screen, its writer's counters, the work it
/// has handed the runtime, and what its full transcript recorded.
#[derive(Debug, Clone, Copy)]
pub struct Screen<'a> {
    pub cols: u16,
    pub rows: u16,
    /// `/stats`'s body.
    pub metrics: &'a str,
    /// Work the runtime has in hand, running and waiting together.
    pub outstanding: usize,
    /// What the document was given, in its order.
    pub timeline: &'a [Entry],
    /// The `* network:` records, one per provider request a turn settled.
    pub network: &'a [Entry],
}

/// Everything one report is made of.
#[derive(Debug, Clone, Copy)]
pub struct Report<'a> {
    pub generated_ms: i64,
    pub status: &'a StatusSnapshot,
    /// The session's reasoning effort, when the surface holds one.
    pub effort: Option<&'a str>,
    pub settings: &'a Settings,
    pub session: Session<'a>,
    /// The TUI's half; `None` on the line shell, which has no screen.
    pub screen: Option<Screen<'a>>,
}

/// The most bytes of a call's arguments the report quotes
/// (`trace_tool_args_max_bytes`, `app_commands.zig:2978`).
const ARGS_BYTES: usize = 1200;

/// The most bytes of a result's first line the report quotes
/// (`writeToolResultPreview`, `app_commands.zig:3320`).
const PREVIEW_BYTES: usize = 180;

/// How many of each kind of problem the report lists, newest first
/// (`writeProblemsSummary`'s caps, `app_commands.zig:2615-2640`).
const NETWORK_PROBLEMS: usize = 3;
const TOOL_PROBLEMS: usize = 5;
const NOTICE_PROBLEMS: usize = 5;

impl Report<'_> {
    /// The report, as the file holds it.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("# xfx trace\n\n");
        // Upstream's sentence (`app_commands.zig:2237`).
        out.push_str(
            "Private diagnostic report. It may include prompts, file paths, command output, \
             and file snippets.\n",
        );
        self.summary(&mut out);
        self.current_state(&mut out);
        self.configuration(&mut out);
        self.problems(&mut out);
        if let Some(screen) = self.screen {
            network_calls(&mut out, screen.network);
        }
        self.tool_calls(&mut out);
        self.permissions(&mut out);
        self.session_log(&mut out);
        if let Some(screen) = self.screen {
            timeline(&mut out, screen.timeline);
        }
        out
    }

    fn state(&self) -> Option<&DurableState> {
        match self.session {
            Session::Read { state, .. } => Some(state),
            _ => None,
        }
    }

    /// `## Summary` (`app_commands.zig:2239-2263`): when, which build, on
    /// what, and what a turn here would run under.
    fn summary(&self, out: &mut String) {
        let status = self.status;
        out.push_str("\n## Summary\n");
        let _ = writeln!(out, "generated: {}", generated_stamp(self.generated_ms));
        match &status.build_revision {
            Some(revision) => {
                let _ = writeln!(out, "version: {} ({revision})", crate::VERSION);
            }
            None => {
                let _ = writeln!(out, "version: {}", crate::VERSION);
            }
        }
        let _ = writeln!(
            out,
            "platform: {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let _ = writeln!(out, "build: {}", status.build_channel);
        let _ = writeln!(out, "provider: {}", status.provider);
        if let Some(url) = &status.provider_url {
            let _ = writeln!(out, "provider_url: {}", redact_url(url));
        }
        let _ = writeln!(out, "model: {}", status.model);
        if let Some(effort) = self.effort {
            let _ = writeln!(out, "effort: {effort}");
        }
        let _ = writeln!(out, "permission_mode: {}", status.permission_mode);
        let _ = writeln!(out, "workspace: {}", status.workspace);
    }

    /// `## Current State` (`writeCurrentStateSummary`,
    /// `app_commands.zig:2367-2470`): the session, its counts, the credential
    /// by name, the process, and on the TUI its screen and its work.
    fn current_state(&self, out: &mut String) {
        out.push_str("\n## Current State\n");
        match self.session {
            Session::None => out.push_str("session: none (xfx opens one at the first prompt)\n"),
            Session::Read { dir, state, .. } => {
                let _ = writeln!(out, "session_id: {}", state.id);
                let _ = writeln!(out, "session_dir: {}", dir.display());
                if let Some(title) = state.title() {
                    let _ = writeln!(out, "session_title: {}", one_line(&mask_secrets(&title)));
                }
                let _ = writeln!(out, "history_turns: {}", state.turns.len());
                let _ = writeln!(
                    out,
                    "tokens_total: {}->{}",
                    state.total_input_tokens, state.total_output_tokens
                );
            }
            Session::Unreadable { id, dir, .. } => {
                let _ = writeln!(out, "session_id: {id}");
                let _ = writeln!(out, "session_dir: {}", dir.display());
            }
        }
        let _ = writeln!(out, "agent_step_limit: {}", self.status.agent_step_limit);
        let _ = writeln!(
            out,
            "auth: source={} refreshable={}",
            self.status.auth, self.status.auth_refreshable
        );
        let _ = writeln!(out, "process: pid={}", std::process::id());
        if let Some(screen) = self.screen {
            let _ = writeln!(
                out,
                "work: outstanding={} queued={}",
                screen.outstanding,
                screen.outstanding.saturating_sub(1)
            );
            let _ = writeln!(out, "terminal: {} cols x {} rows", screen.cols, screen.rows);
            let _ = writeln!(out, "metrics: {}", screen.metrics);
        }
    }

    /// `## Configuration`: the settings files and the layer each setting came
    /// from. xfx's own section, standing where upstream's runtime context
    /// stands; values are left out, so nothing a file holds is copied here.
    fn configuration(&self, out: &mut String) {
        let settings = self.settings;
        out.push_str("\n## Configuration\n");
        let file = |out: &mut String, name: &str, entry: &Option<(PathBuf, &str)>| {
            if let Some((path, standing)) = entry {
                let _ = writeln!(out, "{name}: {} ({standing})", path.display());
            }
        };
        file(out, "user_settings", &settings.user);
        file(out, "project_settings", &settings.project);
        if !settings.sources.is_empty() {
            let sources: Vec<String> = settings
                .sources
                .iter()
                .map(|(name, layer)| format!("{name}={layer}"))
                .collect();
            let _ = writeln!(out, "sources: {}", sources.join(" "));
        }
        let _ = writeln!(out, "diagnostics: {}", settings.diagnostics.len());
    }

    /// `## Problems` (`writeProblemsSummary`, `app_commands.zig:2574-2700`):
    /// everything the other sections hold that says something went wrong,
    /// in one list, or `(none recorded)`.
    fn problems(&self, out: &mut String) {
        out.push_str("\n## Problems\n");
        let before = out.len();
        if let Some(screen) = self.screen {
            if screen.outstanding > 0 {
                let _ = writeln!(
                    out,
                    "- report captured an active turn; the session log may end mid-turn \
                     outstanding={} queued={}",
                    screen.outstanding,
                    screen.outstanding.saturating_sub(1)
                );
            }
        }
        if let Some(help) = &self.status.auth_help {
            let _ = writeln!(
                out,
                "- auth: {} ({})",
                self.status.auth,
                one_line(&mask_secrets(help))
            );
        }
        if let Some(rejected) = &self.status.provider_rejected {
            let _ = writeln!(
                out,
                "- provider: rejected value {}",
                one_line(&mask_secrets(rejected))
            );
        }
        for diagnostic in &self.settings.diagnostics {
            let _ = writeln!(out, "- config: {}", one_line(&mask_secrets(diagnostic)));
        }
        match self.session {
            Session::Read {
                failure: Some(failure),
                ..
            } => {
                let _ = writeln!(out, "- session log: {}", one_line(&mask_secrets(failure)));
            }
            Session::Unreadable { reason, .. } => {
                let _ = writeln!(
                    out,
                    "- session log could not be read: {}",
                    one_line(&mask_secrets(reason))
                );
            }
            _ => {}
        }
        if let Some(state) = self.state() {
            let last = state.turns.len();
            for (index, turn) in state.turns.iter().enumerate() {
                match &turn.outcome {
                    Some(TurnConclusion::Interrupted { reason, .. }) => {
                        let _ = writeln!(
                            out,
                            "- interrupted turn {} reason={}",
                            index + 1,
                            one_line(&mask_secrets(reason))
                        );
                    }
                    // The newest turn may still be running; any other turn
                    // with no conclusion is one the log lost the end of.
                    None if index + 1 < last => {
                        let _ = writeln!(out, "- turn {} has no conclusion recorded", index + 1);
                    }
                    _ => {}
                }
            }
            let failed: Vec<Call<'_>> = calls(state)
                .into_iter()
                .filter(|call| call.outcome() == "failed")
                .collect();
            for call in failed.iter().rev().take(TOOL_PROBLEMS) {
                let _ = writeln!(out, "- tool {}", call.compact());
            }
        }
        if let Some(screen) = self.screen {
            for record in screen
                .network
                .iter()
                .rev()
                .filter(|entry| entry.problem)
                .take(NETWORK_PROBLEMS)
            {
                let _ = writeln!(
                    out,
                    "- network {} {}",
                    moment(record.at_ms),
                    flattened(&record.rows)
                );
            }
            for notice in screen
                .timeline
                .iter()
                .rev()
                .filter(|entry| entry.problem)
                .take(NOTICE_PROBLEMS)
            {
                let _ = writeln!(
                    out,
                    "- notice {} {}",
                    moment(notice.at_ms),
                    flattened(&notice.rows)
                );
            }
        }
        if out.len() == before {
            out.push_str("(none recorded)\n");
        }
    }

    /// `## Tool Calls` (`writeToolCallsSummary`, `app_commands.zig:3216-3308`):
    /// every call the session log holds, its arguments and the first line of
    /// what it returned.
    fn tool_calls(&self, out: &mut String) {
        out.push_str("\n## Tool Calls\n");
        let calls = self.state().map(calls).unwrap_or_default();
        if calls.is_empty() {
            out.push_str("(none recorded)\n");
            return;
        }
        let count = |outcome: &str| {
            calls
                .iter()
                .filter(|call| call.outcome() == outcome)
                .count()
        };
        let _ = writeln!(
            out,
            "calls={} ok={} failed={} pending={}",
            calls.len(),
            count("ok"),
            count("failed"),
            count("pending")
        );
        for call in &calls {
            let _ = writeln!(out, "{}", call.compact());
            let args = one_line(&mask_secrets(&call.input.to_string()));
            let _ = writeln!(out, "  args: {}", clipped(&args, ARGS_BYTES));
            if let Some((_, output)) = call.result {
                if let Some(line) = output
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty() && *line != ":")
                {
                    let line = one_line(&mask_secrets(line));
                    let _ = write!(out, "  result_preview: {}", clipped(&line, PREVIEW_BYTES));
                    if output.len() > line.len() {
                        let _ = write!(out, " ({} bytes total)", output.len());
                    }
                    out.push('\n');
                }
            }
        }
    }

    /// `## Permissions` (`writePermissionsSummary`, `app_commands.zig:3128-3137`),
    /// the grants as the session log holds them.
    fn permissions(&self, out: &mut String) {
        let grants = self.state().map(|state| state.grants.as_slice());
        match grants {
            None | Some([]) => out.push_str("\n## Permissions\npermission grants: none\n"),
            Some(grants) => {
                let _ = writeln!(
                    out,
                    "\n## Permissions\npermission grants ({}):",
                    grants.len()
                );
                for grant in grants {
                    let _ = writeln!(
                        out,
                        "  - {} :: {}",
                        grant.tool,
                        one_line(&mask_secrets(&grant.target))
                    );
                }
            }
        }
    }

    /// `## Session Log`: what the log recorded, turn by turn -- the prompt,
    /// what the model said, the calls it made and how each turn ended. xfx's
    /// own section: upstream's report reads its history out of memory, and
    /// xfx's history is this log.
    fn session_log(&self, out: &mut String) {
        out.push_str("\n## Session Log\n");
        let Session::Read { dir, state, .. } = self.session else {
            out.push_str("(none recorded)\n");
            return;
        };
        let _ = writeln!(
            out,
            "log: {} events={} created={} updated={}",
            dir.join(crate::session::EVENTS_FILE).display(),
            state.last_event_seq,
            moment(state.created_at_ms),
            moment(state.updated_at_ms)
        );
        out.push_str("only obvious secrets masked\n");
        if state.turns.is_empty() {
            out.push_str("(no turns)\n");
            return;
        }
        for (index, turn) in state.turns.iter().enumerate() {
            let _ = write!(
                out,
                "\nturn {} {}",
                index + 1,
                moment(turn.timing.started_at_ms)
            );
            match &turn.outcome {
                Some(TurnConclusion::Final {
                    finish_reason,
                    steps,
                }) => {
                    let _ = write!(
                        out,
                        " outcome=final finish_reason={finish_reason} steps={steps}"
                    );
                }
                Some(TurnConclusion::Interrupted { .. }) => out.push_str(" outcome=interrupted"),
                None => out.push_str(" outcome=none"),
            }
            if let Some(at) = turn.timing.concluded_at_ms {
                let _ = write!(out, " concluded={}", moment(at));
            }
            if let Some(written) = turn.timing.output_tokens {
                let _ = write!(out, " output_tokens={written}");
            }
            out.push('\n');
            block(out, "user", &turn.user);
            for step in &turn.steps {
                match step {
                    TurnStep::Assistant {
                        text, tool_calls, ..
                    } => {
                        if !text.is_empty() {
                            block(out, "assistant", text);
                        }
                        for call in tool_calls {
                            let _ = writeln!(out, "  tool_call {} id={}", call.name, call.id);
                        }
                    }
                    TurnStep::ToolResult {
                        call_id,
                        tool,
                        ok,
                        output,
                        ..
                    } => {
                        let _ = writeln!(
                            out,
                            "  tool_result {tool} id={call_id} ok={ok} bytes={}",
                            output.len()
                        );
                    }
                    TurnStep::ToolFeedback { call_id, text } => {
                        block(out, &format!("feedback id={call_id}"), text);
                    }
                }
            }
        }
    }
}

/// `## Network Calls` (`writeNetworkCallsSummary`, `app_commands.zig:3005-3126`):
/// the full transcript's `* network:` records, one line each.
fn network_calls(out: &mut String, records: &[Entry]) {
    out.push_str("\n## Network Calls\n");
    if records.is_empty() {
        out.push_str("(none recorded)\n");
        return;
    }
    let refused = records.iter().filter(|record| record.problem).count();
    let _ = writeln!(
        out,
        "calls={} ok={} refused={refused}",
        records.len(),
        records.len() - refused
    );
    for record in records {
        let _ = writeln!(out, "{} {}", moment(record.at_ms), flattened(&record.rows));
    }
}

/// `## Transcript Timeline` (`writeTranscriptTimeline`,
/// `app_commands.zig:3475-3567`): what the screen was given, each entry under
/// its moment and its kind, colour stripped and secrets masked.
fn timeline(out: &mut String, entries: &[Entry]) {
    out.push_str("\n## Transcript Timeline\n");
    if entries.is_empty() {
        out.push_str("(empty)\n");
        return;
    }
    out.push_str("ansi stripped; only obvious secrets masked\n");
    let mut emitted = false;
    for entry in entries {
        // Masked whole rather than row by row: a key block the screen gave
        // several rows is one secret, and its middle rows are no less of it.
        let text = mask_secrets(&printable(&entry.rows.join("\n"))).into_owned();
        let rows: Vec<&str> = text
            .lines()
            .map(str::trim_end)
            .filter(|row| !row.is_empty())
            .collect();
        if rows.is_empty() {
            continue;
        }
        // The screen's gutter is not the entry's text: what every row of an
        // entry is indented by is taken off, as upstream lists the text itself.
        let indent = rows
            .iter()
            .map(|row| row.len() - row.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);
        if emitted {
            out.push('\n');
        }
        let _ = writeln!(out, "{} {}", moment(entry.at_ms), entry.kind);
        for row in rows {
            let _ = writeln!(out, "  {}", &row[indent..]);
        }
        emitted = true;
    }
    if !emitted {
        out.push_str("(empty after filtering)\n");
    }
}

/// One labelled block of someone else's text: the label, then each line
/// indented, masked and made printable.
fn block(out: &mut String, label: &str, text: &str) {
    let _ = writeln!(out, "  {label}:");
    for line in mask_secrets(&printable(text)).lines() {
        let line = line.trim_end();
        if !line.is_empty() {
            let _ = writeln!(out, "    {line}");
        }
    }
}

/// Rows joined into one line, for a list item.
fn flattened(rows: &[String]) -> String {
    let joined = rows
        .iter()
        .map(|row| row.trim())
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    one_line(&mask_secrets(&printable(&joined)))
}

/// One tool call as the log holds it, and its result when it has one.
struct Call<'a> {
    turn: usize,
    id: &'a str,
    name: &'a str,
    input: &'a serde_json::Value,
    /// `ok` and the output.
    result: Option<(bool, &'a str)>,
}

impl Call<'_> {
    fn outcome(&self) -> &'static str {
        match self.result {
            Some((true, _)) => "ok",
            Some((false, _)) => "failed",
            None => "pending",
        }
    }

    /// `turn=N name=X outcome=Y id=Z`, upstream's compact line
    /// (`writeToolCallCompact`, `app_commands.zig:3139-3144`) with xfx's facts.
    fn compact(&self) -> String {
        format!(
            "turn={} name={} outcome={} id={}",
            self.turn,
            self.name,
            self.outcome(),
            one_line(self.id)
        )
    }
}

/// Every call in the log, in order, each paired with its result.
fn calls(state: &DurableState) -> Vec<Call<'_>> {
    let mut calls = Vec::new();
    for (index, turn) in state.turns.iter().enumerate() {
        for step in &turn.steps {
            if let TurnStep::Assistant { tool_calls, .. } = step {
                for call in tool_calls {
                    let result = turn.steps.iter().find_map(|step| match step {
                        TurnStep::ToolResult {
                            call_id,
                            ok,
                            output,
                            ..
                        } if *call_id == call.id => Some((*ok, output.as_str())),
                        _ => None,
                    });
                    calls.push(Call {
                        turn: index + 1,
                        id: &call.id,
                        name: &call.name,
                        input: &call.input,
                        result,
                    });
                }
            }
        }
    }
    calls
}

/// `text` clipped to `max` bytes on a character boundary, saying how much was
/// left out (`writeToolFieldBlock`'s `... (N more bytes)`,
/// `app_commands.zig:3351-3353`).
fn clipped(text: &str, max: usize) -> Cow<'_, str> {
    if text.len() <= max {
        return Cow::Borrowed(text);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!(
        "{} ... ({} more bytes)",
        &text[..end],
        text.len() - end
    ))
}

/// `text` on one line: every control character a space.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// `text` with its terminal escape sequences taken out and every other control
/// character but the line break and the tab made a space: a report a person
/// `cat`s must not be able to command their terminal, and one they paste
/// should read as it was shown (upstream's `stripAnsiEscapes`,
/// `app_commands.zig:3553`).
fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\u{1b}' => match chars.next() {
                // CSI: parameters and intermediates, up to a final byte.
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC, DCS and the other strings: up to BEL or ST.
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // Any other escape is two characters long.
                _ => {}
            },
            '\n' | '\t' => out.push(character),
            other if other.is_control() => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// redaction
// ---------------------------------------------------------------------------

/// What a secret is replaced with (`text_utils.zig:463`).
const REDACTED: &str = "[redacted]";

/// One secret found in a text: where its value starts and how long it is.
struct Span {
    start: usize,
    len: usize,
}

/// `text` with everything that looks like a secret replaced by `[redacted]`
/// (`maskSecrets`, `text_utils.zig:457-476`).
///
/// Upstream's rules, in upstream's order (`findSecretSpan`, `:643-662`): a
/// URL's `user:password@`, an AWS access key id, a sensitive `KEY=value`
/// assignment, a known variable's assignment, and a token with a known issuer
/// prefix. Then the credential shapes `scripts/check-no-secrets.sh` refuses to
/// let into this repository, which upstream's rules do not all reach: a Google
/// API key, a Slack token of any kind, a GitHub token shorter than upstream's
/// 36, a JSON web token, a lowercase `bearer`, and a PEM private key block.
///
/// Borrowed back when nothing was found, so masking clean text costs a scan.
pub fn mask_secrets(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        if let Some(span) = secret_at(bytes, index) {
            let masked = out.get_or_insert_with(|| String::with_capacity(text.len()));
            masked.push_str(&text[copied..span.start]);
            masked.push_str(REDACTED);
            index = span.start + span.len;
            copied = index;
        } else {
            index += 1;
        }
    }
    match out {
        None => Cow::Borrowed(text),
        Some(mut masked) => {
            masked.push_str(&text[copied..]);
            Cow::Owned(masked)
        }
    }
}

/// The variable names whose assignment is a secret whatever the value
/// (`secret_prefixes`, `text_utils.zig:628-641`).
const SECRET_PREFIXES: &[&str] = &[
    "OPENAI_API_KEY=",
    "ANTHROPIC_API_KEY=",
    "AI_GATEWAY_API_KEY=",
    "VERCEL_OIDC_TOKEN=",
    "GITHUB_TOKEN=",
    "AWS_SECRET_ACCESS_KEY=",
    "DATABASE_URL=",
    "SECRET_KEY=",
    "API_KEY=",
    "TOKEN=",
    "PASSWORD=",
    "PRIVATE_KEY=",
];

/// The secret that starts at `start`, if one does (`findSecretSpan`,
/// `text_utils.zig:643-662`, then xfx's shapes).
fn secret_at(text: &[u8], start: usize) -> Option<Span> {
    if let Some(span) = basic_auth_url(text, start) {
        return Some(span);
    }
    if let Some(span) = aws_access_key(text, start) {
        return Some(span);
    }
    if let Some(span) = sensitive_assignment(text, start) {
        return Some(span);
    }
    for prefix in SECRET_PREFIXES {
        let end = start + prefix.len();
        if (start == 0 || !is_key_byte(text[start - 1]))
            && end < text.len()
            && text[start..end].eq_ignore_ascii_case(prefix.as_bytes())
        {
            if let Some(span) = secret_value(text, start, end) {
                return Some(span);
            }
        }
    }
    if start + 3 < text.len() {
        if let Some(span) = inline_key(text, start) {
            return Some(span);
        }
    }
    xfx_shape(text, start)
}

/// A token with a known issuer prefix, 16 bytes or more with it
/// (`matchesInlineKey`, `text_utils.zig:664-677`), or a GitHub token
/// (`matchesGithubToken`, `:679-690`).
fn inline_key(text: &[u8], start: usize) -> Option<Span> {
    if start > 0 && is_token_byte(text[start - 1]) {
        return None;
    }
    for prefix in [
        "sk-",
        "sk_live_",
        "pk_live_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "Bearer ",
    ] {
        if start + prefix.len() < text.len() && text[start..].starts_with(prefix.as_bytes()) {
            let end = run_end(text, start + prefix.len(), is_token_byte);
            if end - start >= 16 {
                return Some(Span {
                    start,
                    len: end - start,
                });
            }
        }
    }
    github_token(text, start)
}

fn github_token(text: &[u8], start: usize) -> Option<Span> {
    if start + 40 > text.len() {
        return None;
    }
    let rest = &text[start..];
    if !["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
        .iter()
        .any(|prefix| rest.starts_with(prefix.as_bytes()))
    {
        return None;
    }
    let end = run_end(text, start + 4, is_token_byte);
    if end - (start + 4) < 36 {
        return None;
    }
    Some(Span {
        start,
        len: end - start,
    })
}

/// Twenty upper-case letters and digits behind a known AWS prefix, standing
/// alone (`matchesAwsAccessKey`, `text_utils.zig:692-705`).
fn aws_access_key(text: &[u8], start: usize) -> Option<Span> {
    for prefix in ["AKIA", "ASIA", "AIDA", "AGPA", "AROA", "ANPA"] {
        if start + 20 > text.len() || !text[start..].starts_with(prefix.as_bytes()) {
            continue;
        }
        if !text[start..start + 20]
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        {
            return None;
        }
        if start > 0 && is_token_byte(text[start - 1]) {
            return None;
        }
        if start + 20 < text.len() && is_token_byte(text[start + 20]) {
            return None;
        }
        return Some(Span { start, len: 20 });
    }
    None
}

/// The `user:password` of an `https://user:password@host` URL
/// (`matchesBasicAuthUrl`, `text_utils.zig:707-724`).
fn basic_auth_url(text: &[u8], start: usize) -> Option<Span> {
    const SCHEME: &[u8] = b"https://";
    if !text[start..].starts_with(SCHEME) {
        return None;
    }
    let credentials = start + SCHEME.len();
    let mut colon = None;
    for (index, &byte) in text.iter().enumerate().skip(credentials) {
        if byte.is_ascii_whitespace() {
            return None;
        }
        match byte {
            b'/' | b'?' | b'#' => return None,
            b':' if colon.is_none() => colon = Some(index),
            b'@' => {
                return (colon.is_some() && index > credentials).then_some(Span {
                    start: credentials,
                    len: index - credentials,
                });
            }
            _ => {}
        }
    }
    None
}

/// The value of `KEY=value` when the key names a secret
/// (`matchesSensitiveAssignment`, `text_utils.zig:726-741`).
fn sensitive_assignment(text: &[u8], start: usize) -> Option<Span> {
    if start > 0 && is_key_byte(text[start - 1]) {
        return None;
    }
    if !is_key_byte(text[start]) {
        return None;
    }
    let equals = run_end(text, start, is_key_byte);
    if equals >= text.len() || text[equals] != b'=' {
        return None;
    }
    if !sensitive_key(&text[start..equals]) {
        return None;
    }
    secret_value(text, start, equals + 1)
}

/// The value an assignment holds, unless it only names a variable
/// (`secretAssignmentValueSpan`, `text_utils.zig:748-756`).
fn secret_value(text: &[u8], assignment: usize, value_start: usize) -> Option<Span> {
    let value = assignment_value(text, assignment, value_start)?;
    if only_names_a_variable(text, assignment, value_start, &value) {
        return None;
    }
    Some(value)
}

/// Whether an assignment's value is a `$VAR` or `${VAR}` reference and nothing
/// else, which says where a secret is rather than what it is
/// (`isPureSymbolicAssignment`, `text_utils.zig:758-779`).
fn only_names_a_variable(text: &[u8], assignment: usize, value_start: usize, value: &Span) -> bool {
    if text[value_start] == b'\'' {
        return false;
    }
    if !is_variable_reference(&text[value.start..value.start + value.len]) {
        return false;
    }
    let value_end = value.start + value.len;
    let outer_double_quoted = assignment > 0 && text[assignment - 1] == b'"';
    if text[value_start] != b'"' && !outer_double_quoted {
        return value_end == text.len() || is_word_boundary(text, value_end);
    }
    if value_end >= text.len() || text[value_end] != b'"' {
        return false;
    }
    let next = value_end + 1;
    next == text.len() || is_word_boundary(text, next)
}

/// `isShellWordBoundary`, `text_utils.zig:781-787`.
fn is_word_boundary(text: &[u8], index: usize) -> bool {
    match text[index] {
        b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b')' => true,
        b'<' | b'>' => index + 1 == text.len() || text[index + 1] != b'(',
        _ => false,
    }
}

/// `$NAME` or `${NAME}` (`isPureShellVariableReference`,
/// `text_utils.zig:789-796`).
fn is_variable_reference(value: &[u8]) -> bool {
    if value.len() < 2 || value[0] != b'$' {
        return false;
    }
    if value[1] == b'{' {
        return value.len() >= 4
            && value[value.len() - 1] == b'}'
            && is_variable_name(&value[2..value.len() - 1]);
    }
    is_variable_name(&value[1..])
}

fn is_variable_name(value: &[u8]) -> bool {
    match value.split_first() {
        Some((first, rest)) => {
            (first.is_ascii_alphabetic() || *first == b'_')
                && rest
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        }
        None => false,
    }
}

/// Where an assignment's value is: inside its quotes, up to the closing quote
/// of a string the whole assignment is quoted in, or up to the next blank
/// (`assignmentValueSpan`, `text_utils.zig:808-839`).
fn assignment_value(text: &[u8], assignment: usize, value_start: usize) -> Option<Span> {
    if value_start >= text.len() {
        return None;
    }
    let quote = text[value_start];
    if quote == b'"' || quote == b'\'' {
        let content = value_start + 1;
        let end = run_end(text, content, |byte| {
            byte != b'\n' && byte != b'\r' && byte != quote
        });
        return (end > content).then_some(Span {
            start: content,
            len: end - content,
        });
    }
    if assignment > 0 && text[assignment - 1] == b'"' {
        let end = run_end(text, value_start, |byte| {
            byte != b'\n' && byte != b'\r' && byte != b'"'
        });
        return (end > value_start).then_some(Span {
            start: value_start,
            len: end - value_start,
        });
    }
    let end = run_end(text, value_start, |byte| {
        !byte.is_ascii_whitespace() && byte != b'"' && byte != b'\''
    });
    (end > value_start).then_some(Span {
        start: value_start,
        len: end - value_start,
    })
}

/// `sensitiveAssignmentKey`, `text_utils.zig:841-863`: one of the exact names,
/// or a name that contains one of the words, in any case.
fn sensitive_key(key: &[u8]) -> bool {
    let key = String::from_utf8_lossy(key).to_ascii_lowercase();
    ["passwd", "apikey"].contains(&key.as_str())
        || [
            "password",
            "api_key",
            "apikey",
            "secret",
            "token",
            "private_key",
            "access_key",
        ]
        .iter()
        .any(|word| key.contains(word))
}

/// xfx's own credential shapes, the ones `scripts/check-no-secrets.sh` names
/// that upstream's rules above do not already reach.
fn xfx_shape(text: &[u8], start: usize) -> Option<Span> {
    let rest = &text[start..];
    // A PEM private key, from its header to its footer, or to the end of the
    // text when the footer never came.
    if rest.starts_with(b"-----BEGIN ") {
        let header = run_end(text, start + 11, |byte| {
            byte.is_ascii_uppercase() || byte == b' '
        });
        if text[start + 11..header].ends_with(b"PRIVATE KEY")
            && text[header..].starts_with(b"-----")
        {
            let body = header + 5;
            let end = find(text, body, b"-----END ")
                .and_then(|footer| find(text, footer + 9, b"-----").map(|close| close + 5))
                .unwrap_or(text.len());
            return Some(Span {
                start,
                len: end - start,
            });
        }
        return None;
    }
    if start > 0 && is_token_byte(text[start - 1]) {
        return None;
    }
    let shaped = |prefix_len: usize, body: fn(u8) -> bool, least: usize| {
        let end = run_end(text, start + prefix_len, body);
        (end - (start + prefix_len) >= least).then_some(Span {
            start,
            len: end - start,
        })
    };
    let alphanumeric = |byte: u8| byte.is_ascii_alphanumeric();
    // `gh[pousr]_[A-Za-z0-9]{20,}`
    if rest.len() > 4 && rest.starts_with(b"gh") && b"pousr".contains(&rest[2]) && rest[3] == b'_' {
        if let Some(span) = shaped(4, alphanumeric, 20) {
            return Some(span);
        }
    }
    // `AIza[0-9A-Za-z_-]{30,}`
    if rest.starts_with(b"AIza") {
        if let Some(span) = shaped(
            4,
            |byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-',
            30,
        ) {
            return Some(span);
        }
    }
    // `xox[abprs]-[0-9A-Za-z-]{10,}`
    if rest.len() > 4 && rest.starts_with(b"xox") && b"abprs".contains(&rest[3]) && rest[4] == b'-'
    {
        if let Some(span) = shaped(5, |byte| byte.is_ascii_alphanumeric() || byte == b'-', 10) {
            return Some(span);
        }
    }
    // `[Bb]earer [A-Za-z0-9_.=-]{24,}`
    if rest.starts_with(b"bearer ") || rest.starts_with(b"Bearer ") {
        if let Some(span) = shaped(
            7,
            |byte| byte.is_ascii_alphanumeric() || b"_.=-".contains(&byte),
            24,
        ) {
            return Some(span);
        }
    }
    // `eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.` and its signature.
    if rest.starts_with(b"eyJ") {
        let segment = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
        let header = run_end(text, start + 3, segment);
        if header - (start + 3) >= 10
            && text.get(header) == Some(&b'.')
            && text[header + 1..].starts_with(b"eyJ")
        {
            let payload = run_end(text, header + 4, segment);
            if payload - (header + 4) >= 10 && text.get(payload) == Some(&b'.') {
                let signature = run_end(text, payload + 1, segment);
                return Some(Span {
                    start,
                    len: signature - start,
                });
            }
        }
    }
    None
}

/// Where the run of bytes `keep` accepts, starting at `from`, ends.
fn run_end(text: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    text[from.min(text.len())..]
        .iter()
        .position(|&byte| !keep(byte))
        .map_or(text.len(), |offset| from + offset)
}

/// Where `needle` first occurs at or after `from`.
fn find(text: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    text.get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

/// `isAssignmentKeyChar`, `text_utils.zig:936-938`.
fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// `isTokenChar`, `text_utils.zig:940-942`.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.'
}

/// `url` with its credentials and every sensitive query value replaced by
/// `[redacted]` (`redactUrlForDisplay`, `text_utils.zig:478-550`).
pub fn redact_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    let scheme_end = url.find("://");
    let mut query_from = 0;
    if let Some(end) = scheme_end {
        let authority = end + 3;
        let authority_end = url[authority..]
            .find(['/', '?', '#'])
            .map_or(url.len(), |offset| authority + offset);
        match url[authority..authority_end].find('@') {
            Some(at) => {
                out.push_str(&url[..authority]);
                out.push_str(REDACTED);
                out.push('@');
                out.push_str(&url[authority + at + 1..authority_end]);
            }
            None => out.push_str(&url[..authority_end]),
        }
        query_from = authority_end;
    }
    let Some(marker) = url[query_from..]
        .find('?')
        .map(|offset| query_from + offset)
    else {
        out.push_str(&url[query_from..]);
        return out;
    };
    out.push_str(&url[query_from..=marker]);
    let query_end = url[marker + 1..]
        .find('#')
        .map_or(url.len(), |offset| marker + 1 + offset);
    let query = &url[marker + 1..query_end];
    let segments: Vec<String> = query
        .split('&')
        .map(|segment| match segment.split_once('=') {
            Some((key, _)) if sensitive_query_key(key) => format!("{key}={REDACTED}"),
            _ => segment.to_string(),
        })
        .collect();
    out.push_str(&segments.join("&"));
    out.push_str(&url[query_end..]);
    out
}

/// `sensitiveUrlQueryKey`, `text_utils.zig:865-884`: `sig`, or a key that
/// contains one of the words once percent-decoded, in any case.
fn sensitive_query_key(key: &str) -> bool {
    let key = percent_decoded(key).to_ascii_lowercase();
    key == "sig"
        || [
            "password",
            "passwd",
            "token",
            "api_key",
            "apikey",
            "secret",
            "authorization",
            "cookie",
            "signature",
            "credential",
            "access_key",
            "private_key",
        ]
        .iter()
        .any(|word| key.contains(word))
}

/// `%XX` decoded where it is well formed (`nextPercentDecodedByte`,
/// `text_utils.zig:913-926`), everything else as it is.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = |byte: u8| (byte as char).to_digit(16);
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Environment, PermissionMode};
    use crate::session::{
        NewSession, RecordedToolCall, SessionEvent, SessionId, SessionRecorder, SessionStore,
    };

    /// 2026-10-07T03:12:45.123Z.
    const NOW: i64 = 1_791_342_765_123;

    fn masked(text: &str) -> String {
        mask_secrets(text).into_owned()
    }

    // -- redaction ------------------------------------------------------------

    #[test]
    fn upstreams_env_style_and_quoted_assignments_are_masked() {
        // `text_utils.zig:969-998`, literal.
        assert_eq!(
            masked("AI_GATEWAY_API_KEY=abcdefghijklmnop end"),
            "AI_GATEWAY_API_KEY=[redacted] end"
        );
        assert_eq!(
            masked(
                "API_KEY=\"double-secret\"\nPASSWORD='single-secret'\nACCESS_TOKEN=\"access-secret\""
            ),
            "API_KEY=\"[redacted]\"\nPASSWORD='[redacted]'\nACCESS_TOKEN=\"[redacted]\""
        );
    }

    #[test]
    fn an_assignment_that_only_names_a_variable_is_left_as_written() {
        // `text_utils.zig:1000-1016`: `KEY=$VAR` and `KEY=${VAR}` say where a
        // secret is, not what it is.
        let input = concat!(
            "AI_GATEWAY_API_KEY=\"$key\"\n",
            "sandbox -e \"AI_GATEWAY_API_KEY=$key\" \"$@\"\n",
            "GITHUB_TOKEN=$token\n",
            "DATABASE_URL=\"${database_url}\"\n",
            "TOKEN=\"$token\"; run-next\n",
            "PASSWORD=\"$password\"\tcheck-next\n",
            "ACCESS_TOKEN=\"$token\">token.out\n",
            "SECRET_KEY=\"$key\")",
        );
        assert!(matches!(mask_secrets(input), Cow::Borrowed(_)));
    }

    #[test]
    fn a_compound_or_literal_assignment_is_masked_whatever_it_starts_with() {
        // `text_utils.zig:1018-1068`, literal.
        let input = concat!(
            "AI_GATEWAY_API_KEY=\"literal-value\"\n",
            "sandbox -e \"AI_GATEWAY_API_KEY=literal-value\"\n",
            "sandbox -e \"AI_GATEWAY_API_KEY=$key literal-suffix\"\n",
            "sandbox -e \"GITHUB_TOKEN=$token;literal-suffix\"\n",
            "sandbox -e \"ACCESS_TOKEN=$token>token.out\"\n",
            "sandbox -e \"SECRET_KEY=$key$tail\"\n",
            "sandbox -e \"GITHUB_TOKEN=$token-suffix\"\n",
            "sandbox -e \"API_KEY=$(load-key)\"\n",
            "GITHUB_TOKEN=\"$token-suffix\"\n",
            "DATABASE_URL=\"${database_url:-fallback}\"\n",
            "API_KEY=\"$(load-key)\"\n",
            "VERCEL_OIDC_TOKEN=\"$token\"literal-suffix\n",
            "OPENAI_API_KEY=$key\"literal-suffix\"\n",
            "PASSWORD=\"$password\"\u{b}literal-suffix\n",
            "ACCESS_TOKEN=\"$token\"\u{c}literal-suffix\n",
            "SECRET=\"$secret\"\rliteral-suffix\n",
            "AI_GATEWAY_API_KEY=\"$key\"(literal-suffix)\n",
            "GITHUB_TOKEN=\"$token\"<(literal-suffix)\n",
            "ACCESS_TOKEN=\"$token\">(literal-suffix)\n",
            "SECRET_KEY=\"$key",
        );
        let expected = concat!(
            "AI_GATEWAY_API_KEY=\"[redacted]\"\n",
            "sandbox -e \"AI_GATEWAY_API_KEY=[redacted]\"\n",
            "sandbox -e \"AI_GATEWAY_API_KEY=[redacted]\"\n",
            "sandbox -e \"GITHUB_TOKEN=[redacted]\"\n",
            "sandbox -e \"ACCESS_TOKEN=[redacted]\"\n",
            "sandbox -e \"SECRET_KEY=[redacted]\"\n",
            "sandbox -e \"GITHUB_TOKEN=[redacted]\"\n",
            "sandbox -e \"API_KEY=[redacted]\"\n",
            "GITHUB_TOKEN=\"[redacted]\"\n",
            "DATABASE_URL=\"[redacted]\"\n",
            "API_KEY=\"[redacted]\"\n",
            "VERCEL_OIDC_TOKEN=\"[redacted]\"literal-suffix\n",
            "OPENAI_API_KEY=[redacted]\"literal-suffix\"\n",
            "PASSWORD=\"[redacted]\"\u{b}literal-suffix\n",
            "ACCESS_TOKEN=\"[redacted]\"\u{c}literal-suffix\n",
            "SECRET=\"[redacted]\"\rliteral-suffix\n",
            "AI_GATEWAY_API_KEY=\"[redacted]\"(literal-suffix)\n",
            "GITHUB_TOKEN=\"[redacted]\"<(literal-suffix)\n",
            "ACCESS_TOKEN=\"[redacted]\">(literal-suffix)\n",
            "SECRET_KEY=\"[redacted]",
        );
        assert_eq!(masked(input), expected);
    }

    #[test]
    fn ordinary_words_and_non_secret_assignments_are_left_alone() {
        // `text_utils.zig:1070-1098`.
        for input in [
            "PROJECT_NAME=\"secret-service\"\nGREETING='hello world'",
            "printf output > ask-turn-default-auto.txt",
            "tokens_total: 1200->40",
        ] {
            assert!(matches!(mask_secrets(input), Cow::Borrowed(_)), "{input}");
        }
    }

    #[test]
    fn upstreams_inline_and_issuer_shaped_tokens_are_masked() {
        // `text_utils.zig:1080-1121`. The samples are assembled rather than
        // written out, so `scripts/check-no-secrets.sh` -- which reads this
        // file -- finds no credential in it.
        assert_eq!(
            masked("token sk-abcdefghijklmnop now"),
            "token [redacted] now"
        );
        let aws = ["AKIA", "0123456789ABCDEF"].concat();
        let github = ["ghs", "_abcdefghijklmnopqrstuvwxyz0123456789AB"].concat();
        let input = format!(
            "aws={aws}\ngithub={github}\nurl=https://user:token@example.com/path\n\
             password=hunter2\nCUSTOM_API_KEY=abc123"
        );
        assert_eq!(
            masked(&input),
            "aws=[redacted]\ngithub=[redacted]\nurl=https://[redacted]@example.com/path\n\
             password=[redacted]\nCUSTOM_API_KEY=[redacted]"
        );
    }

    #[test]
    fn the_shapes_xfx_refuses_to_commit_are_masked_too() {
        // `scripts/check-no-secrets.sh`'s patterns that upstream's rules do not
        // reach: each one alone in a sentence, and gone from what is left.
        let filler = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for secret in [
            ["AI", "za", filler].concat(),
            ["xo", "xa-", &filler[..12]].concat(),
            ["xo", "xs-", &filler[..12]].concat(),
            ["gh", "p_", &filler[..24]].concat(),
            [
                "ey",
                "J",
                &filler[..12],
                ".",
                "ey",
                "J",
                &filler[..12],
                ".sig-nature",
            ]
            .concat(),
            ["bea", "rer ", &filler[..30]].concat(),
        ] {
            let input = format!("before {secret} after");
            assert_eq!(masked(&input), "before [redacted] after", "{secret}");
        }
        let key = "PRIVATE KEY";
        let pem = format!(
            "key:\n-----BEGIN RSA {key}-----\nMIIEow\nIBAAK\n-----END RSA {key}-----\nafter"
        );
        assert_eq!(masked(&pem), "key:\n[redacted]\nafter");
        // A block whose footer never came is masked to the end.
        let open = format!("-----BEGIN {key}-----\nMIIEow");
        assert_eq!(masked(&open), "[redacted]");
    }

    #[test]
    fn a_secret_beside_text_in_any_script_is_masked_on_its_own_boundaries() {
        assert_eq!(
            masked("한글 password=hunter2 끝 \u{1F600}"),
            "한글 password=[redacted] 끝 \u{1F600}"
        );
    }

    #[test]
    fn a_urls_credentials_and_sensitive_query_values_are_redacted() {
        // `text_utils.zig:1123-1143`, literal.
        assert_eq!(
            redact_url(
                "https://user:pass@example.com/docs?safe=ok&token=abc123\
                 &X-Amz-%43redential=credential-value&X-Amz-Signature=signature-value"
            ),
            "https://[redacted]@example.com/docs?safe=ok&token=[redacted]\
             &X-Amz-%43redential=[redacted]&X-Amz-Signature=[redacted]"
        );
        assert_eq!(
            redact_url("https://example.com/docs?design=blue&sig=secret"),
            "https://example.com/docs?design=blue&sig=[redacted]"
        );
        for clean in [
            "http://127.0.0.1:3456",
            "http://127.0.0.1:3456/v1?model=a#frag",
        ] {
            assert_eq!(redact_url(clean), clean);
        }
    }

    #[test]
    fn printable_text_keeps_its_lines_and_loses_what_would_command_a_terminal() {
        assert_eq!(
            printable("a\u{1b}[1mbold\u{1b}[0m\tb\n\u{1b}]2;title\u{7}c\u{1b}]8;;x\u{1b}\\d\u{7}"),
            "abold\tb\ncd "
        );
    }

    // -- the file -------------------------------------------------------------

    #[test]
    fn the_sentences_are_upstreams_and_the_two_failures_are_errors() {
        // `format_trace_notice` and its tones (`app_commands.zig:75-90`,
        // `:655-658`).
        let path = PathBuf::from("/tmp/xfx-trace-2026-10-07-031245-0123456789ab.md");
        let said: Vec<(String, bool)> = [
            Disposition::Copied(path.clone()),
            Disposition::CopyFailed(path.clone()),
            Disposition::Saved(path),
            Disposition::Unavailable,
        ]
        .iter()
        .map(|done| (done.sentence(), done.failed()))
        .collect();
        assert_eq!(
            said,
            [
                (
                    "Trace copied to clipboard. Review and redact it before sharing.".to_string(),
                    false
                ),
                (
                    "Clipboard copy failed. Trace saved at \
                     /tmp/xfx-trace-2026-10-07-031245-0123456789ab.md. \
                     Review and redact it before sharing."
                        .to_string(),
                    true
                ),
                (
                    "Trace saved at /tmp/xfx-trace-2026-10-07-031245-0123456789ab.md. \
                     Review and redact it before sharing."
                        .to_string(),
                    false
                ),
                ("Could not create trace.".to_string(), true),
            ]
        );
    }

    #[test]
    fn the_report_goes_where_tmpdir_says_or_to_tmp() {
        assert_eq!(report_dir(None), PathBuf::from("/tmp"));
        assert_eq!(report_dir(Some("".as_ref())), PathBuf::from("/tmp"));
        assert_eq!(
            report_dir(Some("/private/tmp/run/".as_ref())),
            PathBuf::from("/private/tmp/run/")
        );
    }

    #[test]
    fn a_report_is_a_new_owner_only_file_named_for_its_moment() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = write_report(dir.path(), "# xfx trace\n", NOW).expect("written");
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let hex = name
            .strip_prefix("xfx-trace-2026-10-07-031245-")
            .and_then(|rest| rest.strip_suffix(".md"))
            .unwrap_or_else(|| panic!("not upstream's shape: {name}"));
        assert_eq!(hex.len(), 12, "{name}");
        assert!(hex.bytes().all(|byte| byte.is_ascii_hexdigit()), "{name}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# xfx trace\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
        // A second report in the same second is a second file.
        let again = write_report(dir.path(), "two", NOW).expect("written");
        assert_ne!(again, path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# xfx trace\n");
        // And the two reports are all there is: no stage is left behind.
        let mut names: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        names.sort();
        let mut expected = vec![path, again];
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn a_report_killed_between_its_write_and_its_publication_is_no_report() {
        // The moment a kill can land that upstream's write leaves a truncated
        // report at (`app_commands.zig:2207-2224`): the bytes are written and
        // synced and the name is not yet given. A panic there stands in for
        // the process ending -- nothing after it runs -- and what is on disk
        // is what a SIGKILL would leave.
        let dir = tempfile::tempdir().expect("a directory");
        let killed = std::panic::catch_unwind(|| {
            let _ = write_named(
                dir.path(),
                "# xfx trace\nthe whole report\n",
                || "xfx-trace-2026-10-07-031245-0123456789ab.md".to_string(),
                |_| panic!("killed"),
            );
        });
        assert!(killed.is_err());
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            names,
            [".xfx-trace-2026-10-07-031245-0123456789ab.md.partial"],
            "a report was published before it was whole"
        );
        // Every earlier moment is the stage, partly written or not created;
        // every later one is the whole report under its name.
        let published = write_named(
            dir.path(),
            "# xfx trace\nthe whole report\n",
            || "xfx-trace-2026-10-07-031246-0123456789ab.md".to_string(),
            |stage| {
                assert_eq!(
                    std::fs::read_to_string(stage).unwrap(),
                    "# xfx trace\nthe whole report\n",
                    "the stage is published before it is whole"
                );
            },
        )
        .expect("published");
        assert_eq!(
            std::fs::read_to_string(published).unwrap(),
            "# xfx trace\nthe whole report\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_report_is_never_published_over_a_file_that_appeared_under_its_name() {
        // The name was free when the stage was made and taken before it was
        // published: the publication refuses rather than replacing it, and the
        // next name is tried.
        let dir = tempfile::tempdir().expect("a directory");
        let mut names = 0;
        let path = write_named(
            dir.path(),
            "report",
            || {
                names += 1;
                format!("xfx-trace-{names}.md")
            },
            |stage| {
                if stage.ends_with(".xfx-trace-1.md.partial") {
                    std::fs::write(stage.with_file_name("xfx-trace-1.md"), "theirs").unwrap();
                }
            },
        )
        .expect("published under the next name");
        assert_eq!(path.file_name().unwrap(), "xfx-trace-2.md");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("xfx-trace-1.md")).unwrap(),
            "theirs"
        );
        assert!(!dir.path().join(".xfx-trace-1.md.partial").exists());
    }

    #[test]
    #[cfg(unix)]
    fn a_report_never_writes_through_a_name_that_is_already_there() {
        // Exclusive publication: a link planted under every name the write
        // could pick is neither followed nor replaced, the file it points at
        // is not touched, and no stage is left behind.
        let dir = tempfile::tempdir().expect("a directory");
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let mut names = 0;
        let refused = write_named(
            dir.path(),
            "report",
            || {
                names += 1;
                let name = format!("taken-{names}.md");
                std::os::unix::fs::symlink(&victim, dir.path().join(&name)).unwrap();
                name
            },
            |_| {},
        );
        assert_eq!(
            refused.map_err(|err| err.kind()).unwrap_err(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(names, NAME_ATTEMPTS);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        for taken in 1..=NAME_ATTEMPTS {
            let link = dir.path().join(format!("taken-{taken}.md"));
            assert!(std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
            assert!(!dir
                .path()
                .join(format!(".taken-{taken}.md.partial"))
                .exists());
        }
    }

    #[test]
    fn a_directory_that_is_not_there_is_no_report_and_no_copy() {
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let done = deliver(
            "report",
            Path::new("/nonexistent/xfx-trace-dir"),
            &tool.clipboard(),
            NOW,
            &Stop::default(),
        );
        assert_eq!(done, Disposition::Unavailable);
        assert_eq!(tool.copied_file(), None);
    }

    #[test]
    fn a_written_report_is_copied_as_its_path_or_saved_where_there_is_no_file_clipboard() {
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let dir = tempfile::tempdir().expect("a directory");
        let done = deliver(
            "report",
            dir.path(),
            &tool.clipboard(),
            NOW,
            &Stop::default(),
        );
        let written = match &done {
            Disposition::Copied(path) | Disposition::Saved(path) => path.clone(),
            other => panic!("not written: {other:?}"),
        };
        if cfg!(target_os = "macos") {
            assert_eq!(done, Disposition::Copied(written.clone()));
            assert_eq!(
                tool.copied_file().as_deref(),
                Some(written.to_str().unwrap())
            );
        } else {
            assert_eq!(done, Disposition::Saved(written.clone()));
        }
        tool.fail();
        let refused = deliver(
            "report",
            dir.path(),
            &tool.clipboard(),
            NOW,
            &Stop::default(),
        );
        if cfg!(target_os = "macos") {
            assert!(matches!(refused, Disposition::CopyFailed(_)), "{refused:?}");
        } else {
            assert!(matches!(refused, Disposition::Saved(_)), "{refused:?}");
        }
    }

    // -- the report -----------------------------------------------------------

    /// A configuration the real loader resolved over an empty home, with the
    /// Gateway's credential set to `secret`.
    fn configured(secret: &str) -> (tempfile::TempDir, tempfile::TempDir, RuntimeConfig) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("AI_GATEWAY_API_KEY".to_string(), secret.to_string());
        let config = RuntimeConfig::load_with(
            &Environment::new(Some(home.path().to_path_buf()), vars),
            workspace.path(),
        )
        .expect("a configuration");
        (home, workspace, config)
    }

    /// A session written through the real log.
    fn recorded(home: &Path, events: Vec<SessionEvent>) -> (SessionStore, SessionRecorder) {
        let store = SessionStore::open(&home.join(".xfx")).expect("a store");
        let session = store
            .create(
                SessionId::generate(),
                NewSession {
                    origin_workspace_root: home.to_path_buf(),
                    workspace_root: home.to_path_buf(),
                    model: "fake/model-a".to_string(),
                    permission_mode: PermissionMode::Ask,
                },
            )
            .expect("a session");
        let mut recorder = SessionRecorder::new(store.clone(), session);
        for event in events {
            recorder.commit(event);
        }
        assert_eq!(recorder.failure(), None);
        (store, recorder)
    }

    fn render(config: &RuntimeConfig, session: Session<'_>, screen: Option<Screen<'_>>) -> String {
        let status = crate::interactive::status_snapshot(config, &config.model, None);
        Report {
            generated_ms: NOW,
            status: &status,
            effort: Some("auto"),
            settings: &Settings::of(config),
            session,
            screen,
        }
        .render()
    }

    fn sections(report: &str) -> Vec<&str> {
        report
            .lines()
            .filter(|line| line.starts_with("## "))
            .collect()
    }

    #[test]
    fn a_report_before_the_first_prompt_says_there_is_no_session_and_names_the_build() {
        let (_home, _workspace, config) = configured("value-of-the-credential");
        let report = render(&config, Session::None, None);
        assert!(report.starts_with(
            "# xfx trace\n\nPrivate diagnostic report. It may include prompts, file paths, \
             command output, and file snippets.\n\n## Summary\ngenerated: 2026-10-07T03:12:45Z\n"
        ));
        assert_eq!(
            sections(&report),
            [
                "## Summary",
                "## Current State",
                "## Configuration",
                "## Problems",
                "## Tool Calls",
                "## Permissions",
                "## Session Log",
            ],
            "the line shell has no screen, so no network calls and no timeline"
        );
        let version = match crate::build_info().revision {
            Some(revision) => format!("version: {} ({revision})", crate::VERSION),
            None => format!("version: {}", crate::VERSION),
        };
        for line in [
            version,
            format!(
                "platform: {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            format!("workspace: {}", config.workspace_root.display()),
            "session: none (xfx opens one at the first prompt)".to_string(),
            "auth: source=AI_GATEWAY_API_KEY refreshable=false".to_string(),
            format!("process: pid={}", std::process::id()),
            "permission grants: none".to_string(),
            "diagnostics: 0".to_string(),
        ] {
            assert!(report.lines().any(|row| row == line), "{line}\n{report}");
        }
        assert!(
            report.contains("\n## Problems\n(none recorded)\n"),
            "{report}"
        );
        assert!(
            !report.contains("value-of-the-credential"),
            "the credential reached the report"
        );
    }

    #[test]
    fn the_session_log_gives_the_turns_the_calls_the_grants_and_what_went_wrong() {
        let (home, _workspace, config) = configured("value-of-the-credential");
        let (store, recorder) = recorded(
            home.path(),
            vec![
                SessionEvent::UserMessage {
                    text: "read it, password=hunter2".to_string(),
                },
                SessionEvent::AssistantMessage {
                    text: "Looking.".to_string(),
                    tool_calls: vec![
                        RecordedToolCall {
                            id: "call-1".to_string(),
                            name: "read_file".to_string(),
                            input: serde_json::json!({"path": "notes.txt"}),
                        },
                        RecordedToolCall {
                            id: "call-2".to_string(),
                            name: "terminal".to_string(),
                            input: serde_json::json!({"action": "exec", "command": "false"}),
                        },
                    ],
                    raw_content: Vec::new(),
                    responses_state: Vec::new(),
                    wire: None,
                },
                SessionEvent::ToolResult {
                    created_at_ms: None,
                    call_id: "call-1".to_string(),
                    tool: "read_file".to_string(),
                    ok: true,
                    output: "\n1: first line\n2: second line\n".to_string(),
                    ending: None,
                    lines: None,
                    change: None,
                },
                SessionEvent::ToolResult {
                    created_at_ms: None,
                    call_id: "call-2".to_string(),
                    tool: "terminal".to_string(),
                    ok: false,
                    output: "exit code 1".to_string(),
                    ending: None,
                    lines: None,
                    change: None,
                },
                SessionEvent::PermissionGrantRecorded {
                    tool: "terminal".to_string(),
                    target: "cargo test".to_string(),
                },
                SessionEvent::UsageRecorded {
                    input_tokens: Some(1200),
                    output_tokens: Some(40),
                    requests: Vec::new(),
                    unaccounted: false,
                    lines_added: 0,
                    lines_removed: 0,
                    settled: Vec::new(),
                    rejected: Vec::new(),
                },
                SessionEvent::TurnConcluded {
                    outcome: TurnConclusion::Interrupted {
                        reason: "cancelled by the user".to_string(),
                        terminal_reason: None,
                        cancellation_origin: crate::session::CancellationOrigin::Turn,
                    },
                },
            ],
        );
        let dir = store.session_dir(recorder.id());
        let report = render(
            &config,
            Session::Read {
                dir: &dir,
                state: recorder.state(),
                failure: None,
            },
            None,
        );
        for line in [
            format!("session_id: {}", recorder.id()),
            format!("session_dir: {}", dir.display()),
            "history_turns: 1".to_string(),
            "tokens_total: 1200->40".to_string(),
            "- interrupted turn 1 reason=cancelled by the user".to_string(),
            "- tool turn=1 name=terminal outcome=failed id=call-2".to_string(),
            "calls=2 ok=1 failed=1 pending=0".to_string(),
            "turn=1 name=read_file outcome=ok id=call-1".to_string(),
            "  args: {\"path\":\"notes.txt\"}".to_string(),
            "  result_preview: 1: first line (30 bytes total)".to_string(),
            "permission grants (1):".to_string(),
            "  - terminal :: cargo test".to_string(),
            "    read it, password=[redacted]".to_string(),
            "    Looking.".to_string(),
            "  tool_call read_file id=call-1".to_string(),
            "  tool_result terminal id=call-2 ok=false bytes=11".to_string(),
        ] {
            assert!(report.lines().any(|row| row == line), "{line}\n{report}");
        }
        assert!(!report.contains("hunter2"), "{report}");
        assert!(report.contains("outcome=interrupted"), "{report}");
    }

    #[test]
    fn the_screen_adds_its_work_its_network_calls_and_its_timeline() {
        let (_home, _workspace, config) = configured("value-of-the-credential");
        let entry = |at_ms, kind, rows: &[&str], problem| Entry {
            at_ms,
            kind,
            rows: rows.iter().map(|row| row.to_string()).collect(),
            problem,
        };
        let timeline = [
            entry(NOW, "user", &["say TOKEN=abc123"], false),
            entry(NOW + 1, "assistant", &["  Traced answer.   ", ""], false),
            entry(
                NOW + 2,
                "semantic notice",
                &["\u{2717} command: Unknown command. Try /help."],
                true,
            ),
        ];
        let network = [
            entry(
                NOW + 3,
                "semantic notice",
                &[
                    "* network: provider: gateway \u{b7} model: m \u{b7} 5ms",
                    "  finish: stop",
                ],
                false,
            ),
            entry(
                NOW + 4,
                "semantic notice",
                &[
                    "! network: provider: gateway \u{b7} model: m \u{b7} 9ms",
                    "  failed: unauthorized",
                ],
                true,
            ),
        ];
        let report = render(
            &config,
            Session::None,
            Some(Screen {
                cols: 120,
                rows: 40,
                metrics: "ansi_bytes=10, redraws=0, debounced_resizes=0, footer_updates=0, \
                          stream_chunks=0",
                outstanding: 1,
                timeline: &timeline,
                network: &network,
            }),
        );
        assert_eq!(
            sections(&report),
            [
                "## Summary",
                "## Current State",
                "## Configuration",
                "## Problems",
                "## Network Calls",
                "## Tool Calls",
                "## Permissions",
                "## Session Log",
                "## Transcript Timeline",
            ]
        );
        for line in [
            "work: outstanding=1 queued=0",
            "terminal: 120 cols x 40 rows",
            "metrics: ansi_bytes=10, redraws=0, debounced_resizes=0, footer_updates=0, \
             stream_chunks=0",
            "- report captured an active turn; the session log may end mid-turn \
             outstanding=1 queued=0",
            "- network [2026-10-07T03:12:45.127Z] ! network: provider: gateway \u{b7} model: \
             m \u{b7} 9ms failed: unauthorized",
            "- notice [2026-10-07T03:12:45.125Z] \u{2717} command: Unknown command. Try /help.",
            "calls=2 ok=1 refused=1",
            "[2026-10-07T03:12:45.126Z] * network: provider: gateway \u{b7} model: m \u{b7} \
             5ms finish: stop",
            "ansi stripped; only obvious secrets masked",
            "[2026-10-07T03:12:45.123Z] user",
            "  say TOKEN=[redacted]",
            "[2026-10-07T03:12:45.124Z] assistant",
            "  Traced answer.",
        ] {
            assert!(report.lines().any(|row| row == line), "{line}\n{report}");
        }
        assert!(!report.contains("abc123"), "{report}");
    }

    #[test]
    fn a_key_block_the_screen_gave_several_rows_is_masked_whole() {
        let key = "PRIVATE KEY";
        let rows = vec![
            format!("\u{2503} -----BEGIN {key}-----"),
            "\u{2503} MIIEowIBAAKCAQEA-middle-row".to_string(),
            format!("\u{2503} -----END {key}-----"),
            "\u{2503} and after it".to_string(),
        ];
        let mut out = String::new();
        timeline(
            &mut out,
            &[Entry {
                at_ms: NOW,
                kind: "user",
                rows,
                problem: false,
            }],
        );
        assert!(!out.contains("middle-row"), "{out}");
        assert!(
            out.ends_with("] user\n  \u{2503} [redacted]\n  \u{2503} and after it\n"),
            "{out}"
        );
    }

    #[test]
    fn a_daemon_url_is_reported_without_its_credentials() {
        let (_home, _workspace, config) = configured("value-of-the-credential");
        let mut status = crate::interactive::status_snapshot(&config, &config.model, None);
        status.provider_url = Some("http://user:pw@127.0.0.1:3456/?token=t0".to_string());
        let report = Report {
            generated_ms: NOW,
            status: &status,
            effort: None,
            settings: &Settings::default(),
            session: Session::None,
            screen: None,
        }
        .render();
        assert!(
            report.lines().any(
                |row| row == "provider_url: http://[redacted]@127.0.0.1:3456/?token=[redacted]"
            ),
            "{report}"
        );
        assert!(!report.contains("pw@"), "{report}");
    }

    #[test]
    fn the_moments_are_upstreams_shapes() {
        assert_eq!(generated_stamp(NOW), "2026-10-07T03:12:45Z");
        assert_eq!(moment(NOW), "[2026-10-07T03:12:45.123Z]");
        assert_eq!(moment(951_868_799_999), "[2000-02-29T23:59:59.999Z]");
        assert_eq!(moment(0), "[----------T--:--:--Z]");
        assert_eq!(file_stamp(-5), "1970-01-01-000000");
    }
}
