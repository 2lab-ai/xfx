//! A test binary runs its cases only in the copy `support::isolated` makes
//! of it, whatever environment the binary was started with.
//!
//! The copy does not trust its marker: started with one already set -- an
//! old copy's, a developer's shell that kept it -- under a `HOME` that is not
//! exactly the empty directory made for it, or beside a name it is not
//! given, it runs no case at all. What it says when it refuses carries no
//! value of the environment.

mod support;

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use support::isolated::{HOME_PREFIX, MARKER};

/// Leaves a mark in the directory it runs in. Run by the cases below, in a
/// copy whose working directory is the `HOME` it was handed; never in the
/// ordinary run.
#[test]
#[ignore = "run by a_marker_set_before_the_copy_runs_no_case_under_a_home_it_was_not_given"]
fn a_case_marks_where_it_runs() {
    std::fs::write("ran", "").expect("the mark");
}

/// What this binary did, started with `home` as both its `HOME` and its
/// working directory, `tmpdir` as its `TMPDIR`, `marker` as its marker
/// (none: started as a developer starts it) and `extra` beside them, asked
/// to run the marking case alone.
fn started(
    home: &Path,
    tmpdir: &Path,
    marker: Option<&str>,
    extra: &[(&str, &str)],
) -> (bool, String) {
    let mut copy = support::command(std::env::current_exe().expect("this test binary"));
    support::sandbox::clear_env(&mut copy);
    copy.current_dir(home)
        .env("HOME", home)
        .env("TMPDIR", tmpdir)
        .args(["--ignored", "--exact", "a_case_marks_where_it_runs"]);
    if let Some(marker) = marker {
        copy.env(MARKER, marker);
    }
    for (name, value) in extra {
        copy.env(name, value);
    }
    let output = xfx::process::output(&mut copy).expect("this test binary");
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
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
    let tmpdir = scratch.path();
    let marker = |n: u8| format!("{n:032x}");
    let named = |n: u8| tmpdir.join(format!("{HOME_PREFIX}{}", marker(n)));

    // The sentinel's home: the very path the marker names, with a git
    // configuration in it.
    let sentinel = a_private_dir(named(1));
    std::fs::write(sentinel.join(".gitconfig"), "[core]\n").expect("a configuration");
    // An empty home at a path the marker does not name, and one with the
    // name the marker gives, in another directory.
    let other = a_private_dir(tmpdir.join("home"));
    let elsewhere = a_private_dir(tmpdir.join("elsewhere"));
    let misplaced = a_private_dir(elsewhere.join(format!("{HOME_PREFIX}{}", marker(6))));
    // An empty home the marker names, beside a variable the copy is not given.
    let beside = a_private_dir(named(3));
    // The path the marker names, a link to an empty private directory: as
    // it is, and with a trailing `/`, which makes `lstat` follow the link.
    let target = a_private_dir(tmpdir.join("target"));
    let link = named(4);
    std::os::unix::fs::symlink(&target, &link).expect("a link");
    std::os::unix::fs::symlink(&target, named(5)).expect("a link");
    let trailing = PathBuf::from(format!("{}/", named(5).display()));
    // Each home, the marker beside it, any name more, and the refusal.
    type Case<'a> = (&'a Path, String, &'a [(&'a str, &'a str)], &'a str);
    let cases: [Case; 6] = [
        (&sentinel, marker(1), &[], "HOME is not empty"),
        (
            &other,
            marker(2),
            &[],
            "HOME is not the path the parent makes for the marker",
        ),
        (
            &misplaced,
            marker(6),
            &[],
            "HOME is not the path the parent makes for the marker",
        ),
        (
            &beside,
            marker(3),
            &[("XFX_MODEL", "xfx-sentinel-model")],
            "1 name(s) the copy is not given",
        ),
        (
            &link,
            marker(4),
            &[],
            "HOME is not a directory of this user's",
        ),
        (&trailing, marker(5), &[], "HOME ends in /"),
    ];
    for (home, marker, extra, refusal) in cases {
        let (passed, said) = started(home, tmpdir, Some(&marker), extra);
        assert!(
            !passed && said.contains(refusal),
            "started with a marker under {}, the binary did not refuse it for {refusal:?}:\n{said}",
            home.display()
        );
        assert!(
            !home.join("ran").exists(),
            "a case ran under a home the copy was not given ({})",
            home.display()
        );
    }
    assert!(
        !target.join("ran").exists(),
        "a case ran in a link's target"
    );

    // The mark is what a case that ran leaves: started as a developer starts
    // it, the binary makes its copy, and the case marks the directory.
    let control = a_private_dir(tmpdir.join("control"));
    let (passed, said) = started(&control, tmpdir, None, &[]);
    assert!(passed, "the marking case did not run in a copy:\n{said}");
    assert!(
        control.join("ran").exists(),
        "the marking case left no mark"
    );
}

#[test]
fn a_parent_under_a_relative_tmpdir_makes_no_home_and_runs_no_case() {
    // Run from a scratch directory, with TMPDIR relative to it and there: a
    // home made in it would be removed, at exit, from wherever a case had
    // moved to.
    let scratch = tempfile::TempDir::new().expect("a scratch directory");
    let relative = a_private_dir(scratch.path().join("relative-tmp"));
    let (passed, said) = started(scratch.path(), Path::new("relative-tmp"), None, &[]);
    assert!(
        !passed && said.contains("TMPDIR is not an absolute path"),
        "a relative TMPDIR was not refused"
    );
    assert!(
        std::fs::read_dir(&relative)
            .expect("the relative TMPDIR")
            .next()
            .is_none(),
        "a home was made under a relative TMPDIR"
    );
    assert!(!scratch.path().join("ran").exists(), "a case ran");
}

#[test]
fn a_copy_handed_a_home_under_a_relative_tmpdir_runs_no_case() {
    // The home its parent would have made, relative to where it runs.
    let scratch = tempfile::TempDir::new().expect("a scratch directory");
    let marker = format!("{:032x}", 7);
    let home = PathBuf::from("relative-tmp").join(format!("{HOME_PREFIX}{marker}"));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(scratch.path().join(&home))
        .expect("a home under the relative TMPDIR");
    let (passed, said) = started_in(
        scratch.path(),
        &home,
        Path::new("relative-tmp"),
        Some(&marker),
    );
    assert!(
        !passed && said.contains("TMPDIR is not an absolute path"),
        "a copy accepted a home under a relative TMPDIR"
    );
    assert!(
        !scratch.path().join("ran").exists() && !scratch.path().join(&home).join("ran").exists(),
        "a case ran"
    );
}

/// [`started`], from `cwd`, with `home` as `HOME` (relative to `cwd` if it is).
fn started_in(cwd: &Path, home: &Path, tmpdir: &Path, marker: Option<&str>) -> (bool, String) {
    let mut copy = support::command(std::env::current_exe().expect("this test binary"));
    support::sandbox::clear_env(&mut copy);
    copy.current_dir(cwd)
        .env("HOME", home)
        .env("TMPDIR", tmpdir)
        .args(["--ignored", "--exact", "a_case_marks_where_it_runs"]);
    if let Some(marker) = marker {
        copy.env(MARKER, marker);
    }
    let output = xfx::process::output(&mut copy).expect("this test binary");
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn a_copy_that_cannot_be_made_says_so_without_a_value_of_the_environment() {
    let scratch = tempfile::TempDir::new().expect("a scratch directory");
    // A TMPDIR no home can be made under, whose value is a sentinel.
    let tmpdir = scratch
        .path()
        .join("xfx-sentinel-tmpdir-value")
        .join("gone");
    let home = a_private_dir(scratch.path().join("home"));
    let (passed, said) = started(&home, &tmpdir, None, &[]);
    assert!(
        !passed && said.contains("no case runs") && said.contains("under TMPDIR"),
        "a parent whose TMPDIR takes no home did not refuse"
    );
    assert!(
        !said.contains("xfx-sentinel-tmpdir-value"),
        "the refusal printed TMPDIR's value"
    );
    assert!(!home.join("ran").exists(), "a case ran");
}
