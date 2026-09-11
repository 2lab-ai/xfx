//! An independent record of the one class of failure a session cannot show on
//! its own screen: the screen it was about to report on is the one it just
//! lost.
//!
//! `event_loop::disposed` already knows, the instant it ends a session, which
//! of the two roads a failed emit took -- a torn frame or a budget run out on
//! a refusing one ([`super::event_loop`]). This module carries that answer
//! from there to the exit: [`mark`] stamps the error with a [`Reason`] and
//! nothing else, and [`report`] -- called once, after a restoration attempt
//! has already been made -- writes it to a fixed, bounded, independent file
//! if there is a profile home to hold one and the error is one this module
//! marked. That restoration attempt can itself return `Err`; this module
//! does not assume the termios or the parser state actually came back, only
//! that the attempt is behind it. Anything else is left alone: an error
//! nothing here marked gets no report, and a config with no `profile_dir`
//! gets no guess at another path.
//!
//! **What never lands in the file**: the original error's own
//! [`Display`](std::fmt::Display) text. That text may hold a provider's
//! prompt or a user's own words ([`super::deliver::Prefix`]), so the record
//! carries only a fixed schema version, the [`Reason`], the
//! [`io::ErrorKind`]'s own name, and the OS errno when the original error
//! happened to carry one -- never a byte of what the original error says
//! about itself.

use std::io;

use crate::config::RuntimeConfig;
use crate::provider::profile;

/// Why `event_loop::disposed` ended a session on this error -- the one fact
/// [`report`] is allowed to add to what the original error already says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The frame budget ran out on a screen that kept refusing whole vectors:
    /// nothing was ever incomplete, there was just nowhere left to spend the
    /// time offering it again.
    Exhausted,
    /// A vector went out to the terminal in part and stopped: what reached it
    /// may be incomplete, and no vector this session could write is known to
    /// fix that.
    Partial,
}

impl Reason {
    /// The word this reason is closed under in the record -- and the only
    /// place that spelling is decided.
    fn as_str(self) -> &'static str {
        match self {
            Self::Exhausted => "exhausted",
            Self::Partial => "partial",
        }
    }
}

/// An error [`event_loop::disposed`](super::event_loop) has already decided
/// ends the session, kept alongside why.
///
/// Private: nothing outside this module reads a `Marked` directly. A caller
/// gets it back only as the [`io::Error`] [`mark`] returns, and [`report`]
/// alone downcasts into it.
#[derive(Debug)]
struct Marked {
    reason: Reason,
    original: io::Error,
}

/// The same words the original error would print on its own: wrapping it here
/// must not change what a caller who never calls [`report`] sees.
impl std::fmt::Display for Marked {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.original.fmt(out)
    }
}

impl std::error::Error for Marked {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.original)
    }
}

/// Marks `original` with `reason`, so [`report`] can read the reason back off
/// the error a caller is about to return -- rather than re-deriving it from
/// the error's own text, which this module never parses.
///
/// The returned error keeps `original`'s [`io::ErrorKind`] and its
/// [`Display`](std::fmt::Display) text exactly: a caller that never reaches
/// [`report`] -- because `report` was never called, or because nothing here
/// marked what it was given -- sees exactly the error this function was
/// handed.
pub(crate) fn mark(reason: Reason, original: io::Error) -> io::Error {
    let kind = original.kind();
    io::Error::new(kind, Marked { reason, original })
}

/// The filename a report is written under, inside `profile_dir`. Fixed: this
/// module keeps exactly one record, replacing whatever was there before.
const REPORT_FILE: &str = "last-tui-error.json";

/// The schema version stamped into every record. Bumped, never removed, if a
/// field is ever added -- so a reader can tell an old record from a new one
/// without guessing from its shape.
const SCHEMA_VERSION: u32 = 1;

/// Writes one bounded, independent record of `error`, if `error` is one
/// [`mark`] produced and `config` has a profile home to write it in.
///
/// A no-op, not an error, in every other case: an error nothing here marked
/// is left exactly alone -- a startup failure held above
/// [`event_loop::run`](super::event_loop::run), an ordinary provider or I/O
/// error, anything that is not specifically "the screen gave up" -- so
/// nothing is ever relabeled by guessing at its text, and no existing report
/// is overwritten by a session this module never decided was reportable. A
/// config with no `profile_dir` gets no report either, and this function
/// never invents a path to write one at instead.
///
/// The write itself goes through
/// [`profile::write_document`](crate::provider::profile::write_document),
/// the profile's own atomic-and-private helper: the same discipline that
/// keeps `settings.json` from ever being read half-written keeps this record
/// the same way, and a symlink at the destination is replaced rather than
/// written through.
pub(crate) fn report(config: &RuntimeConfig, error: &io::Error) -> io::Result<()> {
    let Some(marked) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Marked>())
    else {
        return Ok(());
    };
    let Some(dir) = config.profile_dir.as_deref() else {
        return Ok(());
    };
    let path = dir.join(REPORT_FILE);
    profile::write_document(&path, &serialize(marked))
}

/// The record itself: the fixed field set and nothing the original error
/// said about itself. `errno` is `null` where `original.raw_os_error()` is
/// `None` -- which it always is for a wrapped [`Prefix`](super::deliver), a
/// private type this module cannot see into and must not guess an errno for.
fn serialize(marked: &Marked) -> Vec<u8> {
    let kind = format!("{:?}", marked.original.kind());
    let errno = match marked.original.raw_os_error() {
        Some(code) => code.to_string(),
        None => "null".to_string(),
    };
    let mut body = format!(
        "{{\"schema\":{SCHEMA_VERSION},\"reason\":\"{reason}\",\"error_kind\":\"{kind}\",\"errno\":{errno}}}",
        reason = marked.reason.as_str(),
    );
    body.push('\n');
    body.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::path::Path;

    use crate::config::Environment;

    /// A `RuntimeConfig` whose only fact this module reads is `profile_dir`
    /// -- `home/.xfx`, the same derivation `RuntimeConfig::load_with` uses --
    /// built against a real, empty workspace so `load_with` itself never
    /// fails.
    fn config_with_home(home: &Path, workspace: &Path) -> RuntimeConfig {
        RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), BTreeMap::new()),
            workspace,
        )
        .expect("load a configuration against a fresh home")
    }

    /// A `RuntimeConfig` with no home at all, so `profile_dir` is `None` and
    /// nothing this module could write has anywhere to go.
    fn config_without_home(workspace: &Path) -> RuntimeConfig {
        RuntimeConfig::load_with(&Environment::new(None, BTreeMap::new()), workspace)
            .expect("load a configuration against no home")
    }

    fn report_path(home: &Path) -> std::path::PathBuf {
        home.join(".xfx").join("last-tui-error.json")
    }

    #[test]
    fn a_marked_partial_reports_its_reason_without_the_original_sentence() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());

        let cause = io::Error::from_raw_os_error(libc::EIO);
        let original = super::super::deliver::Emit::Partial {
            delivered: 7,
            cause,
        }
        .into_error();
        let marked = mark(Reason::Partial, original);

        report(&config, &marked).expect("the report was written");

        let path = report_path(home.path());
        let body = std::fs::read_to_string(&path).expect("read the report");
        assert!(
            body.len() <= 1024,
            "the report exceeded its byte budget: {body:?}"
        );
        assert!(body.contains("\"schema\":1"), "{body:?}");
        assert!(body.contains("\"reason\":\"partial\""), "{body:?}");
        assert!(
            body.contains("\"errno\":null"),
            "a wrapped Prefix has no raw errno of its own; this invented one: {body:?}"
        );
        assert!(
            !body.contains("accepted") && !body.contains('7'),
            "the private Prefix's own words or its count leaked into the report: {body:?}"
        );

        // The substring checks above prove the four fields are present
        // somewhere in the text; parsing proves the record is *exactly*
        // those four keys and nothing else -- an extra field would pass
        // every `contains` check above and still be a schema violation.
        let parsed: serde_json::Value = serde_json::from_str(&body)
            .unwrap_or_else(|err| panic!("the report is not valid JSON: {err}: {body:?}"));
        let object = parsed
            .as_object()
            .unwrap_or_else(|| panic!("the report is not a JSON object: {body:?}"));
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["errno", "error_kind", "reason", "schema"],
            "the report's key set is not exactly the closed schema: {body:?}"
        );

        // The write goes through `profile::write_document`, the same
        // atomic-and-private helper `settings.json` uses; this asserts that
        // discipline held for this record specifically rather than trusting
        // it by association.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("stat the report after it was written")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o600,
                "the report was not written private (mode {mode:o})"
            );
        }
    }

    #[test]
    fn a_marked_exhaustion_reports_the_original_kind_and_errno() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());

        let marked = mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO));
        report(&config, &marked).expect("the report was written");

        let body = std::fs::read_to_string(report_path(home.path())).expect("read the report");
        assert!(body.contains("\"reason\":\"exhausted\""), "{body:?}");
        assert!(
            body.contains(&format!("\"errno\":{}", libc::EIO)),
            "the original errno did not reach the report: {body:?}"
        );
        let expected_kind = format!("{:?}", io::Error::from_raw_os_error(libc::EIO).kind());
        assert!(
            body.contains(&format!("\"error_kind\":\"{expected_kind}\"")),
            "the original kind did not reach the report: {body:?}"
        );
    }

    #[test]
    fn a_bogus_secret_in_the_original_cause_never_reaches_the_bytes() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());

        let secret = "sk-test-do-not-leak-this-token";
        let marked = mark(
            Reason::Exhausted,
            io::Error::other(format!("upstream said: {secret}")),
        );
        report(&config, &marked).expect("the report was written");

        let body = std::fs::read_to_string(report_path(home.path())).expect("read the report");
        assert!(
            !body.contains(secret),
            "the original error's own text reached the independent report: {body:?}"
        );
    }

    #[test]
    fn an_unmarked_error_writes_no_report_and_leaves_an_existing_one_alone() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());
        let path = report_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).expect("create the profile dir");
        std::fs::write(&path, b"preseeded\n").expect("preseed a report");

        let unmarked = io::Error::other("an ordinary provider error, not a screen giving up");
        report(&config, &unmarked).expect("an unmarked error is a no-op, not a failure");

        assert_eq!(
            std::fs::read(&path).expect("read back"),
            b"preseeded\n",
            "an unmarked error touched a report it did not own"
        );
    }

    #[test]
    fn no_home_means_no_report() {
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_without_home(workspace.path());

        let marked = mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO));
        report(&config, &marked).expect("no profile_dir is a no-op, not a failure");
        // There is no home at all here, so there is nothing to list; the
        // claim under test is only that this call did not fail and did not
        // invent a path of its own to write to.
    }

    #[test]
    fn a_second_report_replaces_the_first_rather_than_appending_to_it() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());

        report(
            &config,
            &mark(Reason::Partial, io::Error::from_raw_os_error(libc::EIO)),
        )
        .expect("first report");
        report(
            &config,
            &mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO)),
        )
        .expect("second report");

        let body = std::fs::read_to_string(report_path(home.path())).expect("read the report");
        assert_eq!(
            body.matches("\"schema\"").count(),
            1,
            "the second report was appended to the first instead of replacing it: {body:?}"
        );
        assert!(body.contains("\"reason\":\"exhausted\""), "{body:?}");
    }

    #[test]
    fn a_symlink_at_the_destination_is_replaced_not_followed() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());
        let xfx_dir = home.path().join(".xfx");
        std::fs::create_dir_all(&xfx_dir).expect("create the profile dir");
        let sentinel = home.path().join("sentinel.json");
        std::fs::write(&sentinel, b"do not touch\n").expect("write the sentinel");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&sentinel, report_path(home.path()))
            .expect("symlink the report path at the sentinel");

        report(
            &config,
            &mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO)),
        )
        .expect("the write replaces the symlink's target inode");

        assert_eq!(
            std::fs::read_to_string(&sentinel).expect("read the sentinel back"),
            "do not touch\n",
            "the sentinel a symlink pointed at was written through"
        );
        assert!(
            !std::fs::symlink_metadata(report_path(home.path()))
                .expect("the report path exists")
                .file_type()
                .is_symlink(),
            "the symlink was left in place rather than replaced"
        );
    }

    #[test]
    fn a_directory_at_the_destination_fails_without_destroying_it() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());
        std::fs::create_dir_all(report_path(home.path())).expect("occupy the path with a dir");

        let result = report(
            &config,
            &mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO)),
        );

        assert!(
            result.is_err(),
            "a directory at the destination was written through"
        );
        assert!(
            report_path(home.path()).is_dir(),
            "the occupying directory was destroyed by a failed write"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_profile_directory_fails_the_write() {
        // Skipped rather than falsely green under a privileged account: root
        // (and some sandboxed CI runners) ignore directory write permission
        // bits entirely, and a pass under that account would prove nothing
        // about the permission check this case exists to prove.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipping: running as root, which ignores directory permissions");
            return;
        }
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let config = config_with_home(home.path(), workspace.path());
        let xfx_dir = home.path().join(".xfx");
        std::fs::create_dir_all(&xfx_dir).expect("create the profile dir");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&xfx_dir, std::fs::Permissions::from_mode(0o500))
            .expect("make the profile dir read-only");

        let result = report(
            &config,
            &mark(Reason::Exhausted, io::Error::from_raw_os_error(libc::EIO)),
        );

        std::fs::set_permissions(&xfx_dir, std::fs::Permissions::from_mode(0o700))
            .expect("restore permissions so the tempdir can be cleaned up");
        assert!(
            result.is_err(),
            "a read-only profile directory accepted a write"
        );
    }

    #[test]
    fn marking_preserves_the_original_kind_display_and_source() {
        let cause = io::Error::new(io::ErrorKind::BrokenPipe, "the pipe closed");
        let text_before = cause.to_string();
        let kind_before = cause.kind();

        let marked = mark(Reason::Exhausted, cause);

        assert_eq!(
            marked.kind(),
            kind_before,
            "marking changed the error's kind"
        );
        assert_eq!(
            marked.to_string(),
            text_before,
            "marking changed what the error prints"
        );
        let source = std::error::Error::source(&marked).expect("the original is the source");
        assert_eq!(source.to_string(), text_before);
    }
}
