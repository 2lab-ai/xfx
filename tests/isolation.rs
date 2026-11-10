//! A test binary runs its cases only in the copy `support::isolated` makes
//! of it, whatever environment the binary was started with.
//!
//! The copy does not trust its marker: started with one already set -- an
//! old copy's, a developer's shell that kept it -- under a `HOME` that is not
//! the empty one made for it, or beside a name it is not given, it runs no
//! case at all.

mod support;

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use support::isolated::{HOME_PREFIX, MARKER};

/// Leaves a mark in the directory it runs in. Run by the case below, in a
/// copy whose working directory is the `HOME` it was handed; never in the
/// ordinary run.
#[test]
#[ignore = "run by a_marker_set_before_the_copy_runs_no_case_under_a_home_it_was_not_given"]
fn a_case_marks_where_it_runs() {
    std::fs::write("ran", "").expect("the mark");
}

/// What this binary did, started with `home` as both its `HOME` and its
/// working directory, `marker` as its marker (none: started as a developer
/// starts it) and `extra` beside them, asked to run the marking case alone.
fn started(home: &Path, marker: Option<&str>, extra: &[(&str, &str)]) -> (bool, String) {
    let mut copy = support::command(std::env::current_exe().expect("this test binary"));
    support::sandbox::clear_env(&mut copy);
    copy.current_dir(home).env("HOME", home).args([
        "--ignored",
        "--exact",
        "a_case_marks_where_it_runs",
    ]);
    if let Some(marker) = marker {
        copy.env(MARKER, marker);
    }
    for (name, value) in extra {
        copy.env(name, value);
    }
    let output = xfx::process::output(&mut copy).expect("this test binary");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn a_private_dir(path: PathBuf) -> PathBuf {
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .expect("a private directory");
    path
}

#[test]
fn a_marker_set_before_the_copy_runs_no_case_under_a_home_it_was_not_given() {
    let scratch = tempfile::TempDir::new().expect("a scratch directory");
    let marker = |n: u8| format!("{n:032x}");
    let named = |n: u8| a_private_dir(scratch.path().join(format!("{HOME_PREFIX}{}", marker(n))));

    // The sentinel's home: named as the marker says, with a git
    // configuration in it.
    let sentinel = named(1);
    std::fs::write(sentinel.join(".gitconfig"), "[core]\n").expect("a configuration");
    // An empty home the marker does not name.
    let other = a_private_dir(scratch.path().join("home"));
    // An empty home the marker names, beside a variable the copy is not given.
    let beside = named(3);
    // Each home, the marker beside it, any name more, and the refusal.
    type Case<'a> = (&'a Path, String, &'a [(&'a str, &'a str)], &'a str);
    let cases: [Case; 3] = [
        (&sentinel, marker(1), &[], "HOME is not empty"),
        (
            &other,
            marker(2),
            &[],
            "HOME is not the directory the marker names",
        ),
        (
            &beside,
            marker(3),
            &[("XFX_MODEL", "xfx-sentinel-model")],
            "1 name(s) the copy is not given",
        ),
    ];
    for (home, marker, extra, refusal) in cases {
        let (passed, stderr) = started(home, Some(&marker), extra);
        assert!(
            !passed && stderr.contains(refusal),
            "started with a marker under {}, the binary did not refuse it for {refusal:?}:\n{stderr}",
            home.display()
        );
        assert!(
            !home.join("ran").exists(),
            "a case ran under a home the copy was not given ({})",
            home.display()
        );
    }

    // The mark is what a case that ran leaves: started as a developer starts
    // it, the binary makes its copy, and the case marks the directory.
    let control = a_private_dir(scratch.path().join("control"));
    let (passed, stderr) = started(&control, None, &[]);
    assert!(passed, "the marking case did not run in a copy:\n{stderr}");
    assert!(
        control.join("ran").exists(),
        "the marking case left no mark"
    );
}
