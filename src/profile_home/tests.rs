//! The profile home is made private by every writer that can make it, and an
//! existing one that is not private is refused by every writer, in the session
//! store's words, and never `chmod`ed.

use super::*;
use crate::session::prompt_history::PromptHistory;
use crate::session::SessionStore;
use crate::skills::command::{execute, parse, Outcome};
use crate::skills::discovery::{discover, managed_root};
use std::os::unix::fs::PermissionsExt;

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .expect("a stat")
        .permissions()
        .mode()
        & 0o777
}

/// A home and a workspace beside it, under a canonical root, the home with no
/// `~/.xfx` in it yet.
struct Bench {
    _root: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    work: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("a root");
        let canonical = fs::canonicalize(root.path()).expect("canonical");
        let home = canonical.join("home");
        let work = canonical.join("work");
        fs::create_dir(&home).expect("a home");
        fs::create_dir(&work).expect("a workspace");
        // A skill to install, with a directory inside it so the copy makes one.
        let source = canonical.join("source").join("carried");
        fs::create_dir_all(source.join("refs")).expect("a source");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: carried\ndescription: Carried\n---\n",
        )
        .expect("a SKILL.md");
        fs::write(source.join("refs").join("note.md"), "note\n").expect("a note");
        Self {
            _root: root,
            root: canonical,
            home,
            work,
        }
    }

    fn profile(&self) -> PathBuf {
        self.home.join(crate::config::PROFILE_DIR_NAME)
    }
}

/// What one writer answered: `Ok` for a write that happened, the words it
/// refused with otherwise.
type Answer = Result<(), String>;

/// One writer, named for the failure messages.
type Writer = (&'static str, fn(&Bench) -> Answer);

/// Every product writer that can be the first to need `~/.xfx`, each run as
/// its surface runs it.
fn writers() -> Vec<Writer> {
    fn skills(bench: &Bench, line: &str) -> Answer {
        let found = discover(&bench.work, Some(&bench.home));
        match execute(
            &parse(line),
            &found,
            &managed_root(&bench.home),
            &bench.work,
        ) {
            Outcome::Changed { .. } => Ok(()),
            // The last sentence is the answer: an install says what it is
            // about to do first.
            Outcome::Said(said) => Err(said.last().cloned().unwrap_or_default()),
            other => Err(format!("{other:?}")),
        }
    }
    vec![
        ("the session store", |bench| {
            SessionStore::open(&bench.profile())
                .map(drop)
                .map_err(|err| err.to_string())
        }),
        ("a settings preference", |bench| {
            crate::provider::profile::write_preference(
                &bench.profile().join("settings.json"),
                &["profile_home_test"],
                serde_json::Value::Bool(true),
            )
            .map_err(|err| err.to_string())
        }),
        ("a profile document", |bench| {
            crate::provider::profile::write_document(
                &bench.profile().join("last-tui-error.json"),
                b"{}\n",
            )
            .map_err(|err| err.to_string())
        }),
        // The three settings transactions that take `settings.lock` after
        // making the home: a home refused after the lock was taken would hold
        // a `settings.lock` it never should have.
        ("/allowlist", |bench| {
            crate::provider::profile::write_permission(
                &bench.profile().join("settings.json"),
                None,
                &crate::permission::allowlist::Patch::Add {
                    category: "command".to_string(),
                    pattern: "ls".to_string(),
                    action: crate::permission::rules::RuleAction::Allow,
                },
            )
            .map(drop)
            .map_err(|err| err.to_string())
        }),
        ("a settings edit", |bench| {
            crate::provider::profile::edit_document::<io::Error>(
                &bench.profile().join("settings.json"),
                |document| {
                    document.insert("profile_home_test".to_string(), true.into());
                    Ok(true)
                },
            )
            .map(drop)
            .map_err(|err| err.to_string())
        }),
        ("xfx setup's rewrite", |bench| {
            crate::provider::profile::rewrite_document(
                &bench.profile().join("settings.json"),
                |_| Ok(b"{}\n".to_vec()),
            )
            .map_err(|err| err.to_string())
        }),
        ("the MCP profile", |bench| {
            crate::mcp::config::add_profile_server(
                &bench.profile().join("mcp.json"),
                crate::mcp::config::ServerConfig::local("demo", "true", &[]),
            )
            .map_err(|err| err.to_string())
        }),
        ("the prompt history", |bench| {
            PromptHistory::in_profile(&bench.profile())
                .append(1, &bench.work.to_string_lossy(), "hello")
                .map(drop)
                .map_err(|err| err.to_string())
        }),
        ("/skills create", |bench| skills(bench, "create demo")),
        ("/skills add", |bench| {
            let source = bench.root.join("source").join("carried");
            skills(bench, &format!("add {}", source.display()))
        }),
    ]
}

/// Every directory at or below `dir`, links not followed.
fn directories(dir: &Path) -> Vec<PathBuf> {
    let mut found = vec![dir.to_path_buf()];
    let mut index = 0;
    while index < found.len() {
        let here = found[index].clone();
        index += 1;
        for entry in fs::read_dir(&here).expect("a listing") {
            let entry = entry.expect("an entry");
            if entry.file_type().expect("a type").is_dir() {
                found.push(entry.path());
            }
        }
    }
    found
}

/// The body both umask cases run: each writer, from a home with no `~/.xfx`,
/// makes every directory in the profile `0700`, and the session store opens
/// what it left.
fn every_writer_makes_the_profile_home_private() {
    for (writer, write) in writers() {
        let bench = Bench::new();
        assert_eq!(write(&bench), Ok(()), "{writer}");
        let profile = bench.profile();
        for dir in directories(&profile) {
            assert_eq!(mode(&dir), 0o700, "{writer} made {}", dir.display());
        }
        SessionStore::open(&profile)
            .unwrap_or_else(|err| panic!("after {writer} the store refused: {err}"));
    }
}

#[test]
fn every_writer_makes_a_missing_profile_home_private_and_the_store_opens_it() {
    every_writer_makes_the_profile_home_private();
}

/// The case below, in a copy of this test binary started under `umask 0`.
const UNDER_OPEN_UMASK_CASE: &str =
    "profile_home::tests::under_an_open_umask_every_writer_makes_the_profile_home_private";

/// Set in the copy, which alone runs the case's body.
const UNDER_OPEN_UMASK: &str = "XFX_TEST_PROFILE_HOME_UNDER_OPEN_UMASK";

/// How long that copy may run: it runs one case, which takes well under a
/// second; a copy still running after this is hung.
const CHILD_LIMIT: std::time::Duration = std::time::Duration::from_secs(120);

#[test]
fn under_an_open_umask_every_writer_makes_the_profile_home_private() {
    if std::env::var_os(UNDER_OPEN_UMASK).is_none() {
        return;
    }
    // The umask took: a directory made the plain way is open to everyone.
    let bench = Bench::new();
    let plain = bench.root.join("plain");
    fs::create_dir_all(plain.join("below")).expect("a plain directory");
    assert_eq!(mode(&plain), 0o777, "the copy is not under umask 0");
    every_writer_makes_the_profile_home_private();
}

#[test]
fn every_writer_makes_the_profile_home_private_under_any_umask() {
    use std::os::unix::process::CommandExt;
    let mut runner = crate::process::command(std::env::current_exe().expect("this test binary"));
    runner.args(["--exact", "--test-threads=1", UNDER_OPEN_UMASK_CASE]);
    runner.env(UNDER_OPEN_UMASK, "1");
    // SAFETY: `umask` is async-signal-safe and touches nothing of the parent.
    unsafe {
        runner.pre_exec(|| {
            libc::umask(0);
            Ok(())
        });
    }
    // Bounded: a child that hangs is killed and reaped, and the case fails.
    let output = crate::process::output_within(&mut runner, CHILD_LIMIT).expect("a runner");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Counted, not only exited zero: a filter that matched nothing passes.
    assert!(
        output.status.success() && report.contains("test result: ok. 1 passed"),
        "{report}"
    );
}

/// What `writer` is expected to say for a home refused with `store`, the
/// session store's own sentence: each says it in its surface's shape, the MCP
/// profile in upstream's error name.
fn refused_as(writer: &str, store: &str, name: &str) -> String {
    match writer {
        "the MCP profile" => name.to_string(),
        "/skills create" => format!("Failed to create skill 'demo': {store}."),
        "/skills add" => format!("Failed to install: {store}."),
        _ => store.to_string(),
    }
}

#[test]
fn an_open_profile_home_is_refused_by_every_writer_in_the_stores_words_and_never_chmodded() {
    for (writer, write) in writers() {
        let bench = Bench::new();
        let profile = bench.profile();
        fs::create_dir(&profile).expect("a profile home");
        fs::set_permissions(&profile, fs::Permissions::from_mode(0o755)).expect("open it");
        let store = SessionStore::open(&profile)
            .expect_err("the store refuses an open home")
            .to_string();
        assert_eq!(
            store,
            refusal(
                &profile,
                &format!(
                    "it is mode 755; session state must not be reachable by group or other \
                     (`chmod 700 {}` makes it private)",
                    profile.display()
                )
            )
        );
        assert_eq!(
            write(&bench),
            Err(refused_as(
                writer,
                &store,
                "PrivateStatePermissionsUnsupported"
            )),
            "{writer}"
        );
        assert_eq!(mode(&profile), 0o755, "{writer} changed the home's mode");
        assert_eq!(
            fs::read_dir(&profile).expect("a listing").count(),
            0,
            "{writer} wrote in a home it refused"
        );
    }
}

#[test]
fn a_linked_profile_home_is_refused_by_every_writer_and_nothing_is_made_where_it_points() {
    for (writer, write) in writers() {
        let bench = Bench::new();
        let profile = bench.profile();
        let elsewhere = bench.root.join("elsewhere");
        fs::create_dir(&elsewhere).expect("a directory");
        fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).expect("private");
        std::os::unix::fs::symlink(&elsewhere, &profile).expect("a link");
        let store = SessionStore::open(&profile)
            .expect_err("the store refuses a linked home")
            .to_string();
        assert_eq!(store, refusal(&profile, &Problem::Link.detail(&profile)));
        assert_eq!(
            write(&bench),
            Err(refused_as(writer, &store, "DurablePathUnsafe")),
            "{writer}"
        );
        assert!(fs::symlink_metadata(&profile)
            .expect("still there")
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_dir(&elsewhere).expect("a listing").count(),
            0,
            "{writer} wrote where the link points"
        );
        assert_eq!(mode(&elsewhere), 0o700, "{writer}");
    }
}

#[test]
fn a_file_where_the_profile_home_goes_is_refused_and_left_alone() {
    let bench = Bench::new();
    let profile = bench.profile();
    fs::write(&profile, b"a file\n").expect("a file");
    let err = ensure(&profile, &profile.join("skills")).expect_err("refused");
    assert_eq!(
        err.to_string(),
        refusal(&profile, "it is not a directory"),
        "{err}"
    );
    assert_eq!(
        Refused::of(&err).map(|refused| refused.problem),
        Some(Problem::NotDirectory)
    );
    assert_eq!(fs::read(&profile).expect("still a file"), b"a file\n");
}

#[test]
fn another_accounts_directory_is_refused_before_its_mode_is_looked_at() {
    // `/` is root's and `0755`: owned by someone else first, open second.
    let current = rustix::process::geteuid().as_raw();
    if current == 0 {
        return;
    }
    assert_eq!(
        problem(Path::new("/")).expect("a stat"),
        Some(Problem::Owner { owner: 0, current })
    );
    assert_eq!(
        Problem::Owner { owner: 0, current }.detail(Path::new("/")),
        format!("it is owned by uid 0, not by uid {current} running xfx")
    );
}

#[test]
fn a_missing_home_is_refused_and_nothing_is_made() {
    // Upstream refuses a home that is not there (`settings_store.zig:343-347`);
    // the base made it, `0755` through the store.
    let bench = Bench::new();
    let home = bench.root.join("absent").join("home");
    let profile = home.join(crate::config::PROFILE_DIR_NAME);
    for (writer, result) in [
        ("ensure", ensure(&profile, &profile.join("skills"))),
        (
            "ensure_content",
            ensure_content(&profile, &profile.join("skills")),
        ),
        (
            "the session store",
            SessionStore::open(&profile)
                .map(drop)
                .map_err(|err| io::Error::other(err.to_string())),
        ),
    ] {
        let err = result.expect_err(writer);
        assert!(
            err.to_string().contains(&format!(
                "the home directory {} is not there, and xfx does not make one",
                home.display()
            )),
            "{writer}: {err}"
        );
    }
    assert_eq!(
        ensure(&profile, &profile).expect_err("no home").kind(),
        io::ErrorKind::NotFound
    );
    assert!(!bench.root.join("absent").exists(), "a home was made");
}

#[test]
fn every_level_below_the_profile_home_is_made_private() {
    let bench = Bench::new();
    let profile = bench.profile();
    let deepest = profile.join(".skill-install-locks").join("deeper");
    ensure(&profile, &deepest).expect("made");
    let content = profile.join("skills").join("demo");
    ensure_content(&profile, &content).expect("made");
    for dir in directories(&profile) {
        assert_eq!(mode(&dir), 0o700, "{}", dir.display());
    }
    assert!(deepest.is_dir() && content.is_dir());
}

#[test]
fn a_private_level_that_is_a_link_is_refused_and_its_target_left_alone() {
    // `.skill-install-locks` is private state: upstream opens it without
    // following a link (`openOrCreateVerifiedPrivateDirFromDir`), and so a
    // link there is refused -- by `ensure` and by `/skills add`, which takes
    // its lock there -- with nothing made where it points.
    let bench = Bench::new();
    let profile = made(&bench.profile()).to_path_buf();
    let locks = profile.join(".skill-install-locks");
    let elsewhere = bench.root.join("elsewhere");
    fs::create_dir(&elsewhere).expect("a directory");
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).expect("private");
    std::os::unix::fs::symlink(&elsewhere, &locks).expect("a link");
    let err = ensure(&profile, &locks).expect_err("a linked private level");
    assert_eq!(
        Refused::of(&err).map(|refused| (refused.path.clone(), refused.problem)),
        Some((locks.clone(), Problem::Link))
    );
    assert_eq!(
        err.to_string(),
        refusal(&locks, &Problem::Link.detail(&locks))
    );
    let source = bench.root.join("source").join("carried");
    let found = discover(&bench.work, Some(&bench.home));
    let said = execute(
        &parse(&format!("add {}", source.display())),
        &found,
        &managed_root(&bench.home),
        &bench.work,
    );
    assert_eq!(
        said,
        Outcome::Said(vec![
            format!("Installing from {}...", source.display()),
            format!("Failed to install: {err}."),
        ])
    );
    assert!(fs::symlink_metadata(&locks)
        .expect("there")
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_dir(&elsewhere).expect("a listing").count(), 0);
    assert_eq!(mode(&elsewhere), 0o700);
    assert!(!managed_root(&bench.home).join("carried").exists());
}

#[test]
fn an_open_private_level_is_refused_before_a_lock_is_taken_in_it() {
    // The lock directory is held to `~/.xfx`'s rule: an open one is refused
    // and nothing -- no lock file -- is made in it (the base took its lock
    // there).
    let bench = Bench::new();
    let profile = made(&bench.profile()).to_path_buf();
    let locks = profile.join(".skill-install-locks");
    fs::create_dir(&locks).expect("a lock directory");
    fs::set_permissions(&locks, fs::Permissions::from_mode(0o755)).expect("open it");
    let source = bench.root.join("source").join("carried");
    let found = discover(&bench.work, Some(&bench.home));
    let said = execute(
        &parse(&format!("add {}", source.display())),
        &found,
        &managed_root(&bench.home),
        &bench.work,
    );
    let refused = refusal(&locks, &Problem::Open { mode: 0o755 }.detail(&locks));
    assert_eq!(
        said,
        Outcome::Said(vec![
            format!("Installing from {}...", source.display()),
            format!("Failed to install: {refused}."),
        ])
    );
    assert_eq!(mode(&locks), 0o755, "never chmodded");
    assert_eq!(fs::read_dir(&locks).expect("a listing").count(), 0);
    assert!(!managed_root(&bench.home).join("carried").exists());

    // Something there that is not a directory is refused the same way.
    let blocked = profile.join("blocked");
    fs::write(&blocked, b"").expect("a file");
    let err = ensure(&profile, &blocked.join("x")).expect_err("in the way");
    assert_eq!(
        Refused::of(&err).map(|refused| refused.problem),
        Some(Problem::NotDirectory)
    );
    assert!(blocked.is_file());
}

#[test]
fn a_content_level_that_is_there_is_used_as_it_is_and_a_link_to_one_is_followed() {
    // The managed skills root is the user's: an open one is used, not
    // chmodded, and one linked into the user's dotfiles is followed, as
    // upstream's `makeDirRecursive` follows it (`skills.zig:247-252`).
    let bench = Bench::new();
    let profile = made(&bench.profile()).to_path_buf();
    let dotfiles = bench.root.join("dotfiles").join("skills");
    fs::create_dir_all(&dotfiles).expect("a dotfiles skills directory");
    fs::set_permissions(&dotfiles, fs::Permissions::from_mode(0o755)).expect("open");
    let skills = managed_root(&bench.home);
    std::os::unix::fs::symlink(&dotfiles, &skills).expect("a link");
    let found = discover(&bench.work, Some(&bench.home));
    let said = execute(&parse("create demo"), &found, &skills, &bench.work);
    assert!(matches!(said, Outcome::Changed { .. }), "{said:?}");
    assert!(fs::symlink_metadata(&skills)
        .expect("there")
        .file_type()
        .is_symlink());
    assert!(dotfiles.join("demo").join("SKILL.md").is_file());
    assert_eq!(
        mode(&dotfiles),
        0o755,
        "an existing level is never chmodded"
    );
    assert_eq!(
        mode(&dotfiles.join("demo")),
        0o700,
        "a level made is private"
    );

    // Something in the way that is not a directory is an error, not replaced.
    let blocked = profile.join("blocked");
    fs::write(&blocked, b"").expect("a file");
    let err = ensure_content(&profile, &blocked.join("x")).expect_err("in the way");
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
    assert!(blocked.is_file());
}

#[test]
fn an_open_home_names_the_chmod_that_makes_it_private_as_one_shell_word() {
    for (path, word) in [
        ("/h/u/.xfx", "/h/u/.xfx"),
        ("/h/a b/.xfx", "'/h/a b/.xfx'"),
        ("/h/it's/.xfx", "'/h/it'\\''s/.xfx'"),
    ] {
        assert_eq!(
            Problem::Open { mode: 0o750 }.detail(Path::new(path)),
            format!(
                "it is mode 750; session state must not be reachable by group or other \
                 (`chmod 700 {word}` makes it private)"
            ),
            "{path}"
        );
    }
}

#[test]
fn nothing_outside_the_profile_home_is_made() {
    let bench = Bench::new();
    let profile = bench.profile();
    for dir in [
        bench.root.join("beside"),
        profile.join("..").join("beside"),
        profile.join("skills").join("..").join("..").join("beside"),
    ] {
        let err = ensure(&profile, &dir).expect_err("outside");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{}", dir.display());
    }
    assert!(!bench.root.join("beside").exists());
    assert!(!bench.home.join("beside").exists());
    // Refused before anything was made, the profile home included.
    assert!(!profile.exists());
}

#[test]
fn an_existing_private_home_is_used_and_its_mode_left_as_it_was() {
    let bench = Bench::new();
    let profile = bench.profile();
    fs::create_dir(&profile).expect("a home");
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700)).expect("0700");
    ensure(&profile, &profile).expect("used");
    assert_eq!(mode(&profile), 0o700);
}

mod scan;
