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
    let output = runner.output().expect("a runner");
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
                "it is mode 755; session state must not be reachable by group or other"
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
        assert_eq!(store, refusal(&profile, &Problem::Link.detail()));
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
        Problem::Owner { owner: 0, current }.detail(),
        format!("it is owned by uid 0, not by uid {current} running xfx")
    );
}

#[test]
fn a_missing_home_and_every_level_below_the_profile_home_are_made_private() {
    let bench = Bench::new();
    let home = bench.root.join("absent").join("home");
    let profile = home.join(crate::config::PROFILE_DIR_NAME);
    let deepest = profile.join("skills").join("demo");
    ensure(&profile, &deepest).expect("made");
    for dir in [bench.root.join("absent"), home, profile.clone()]
        .into_iter()
        .chain(directories(&profile))
    {
        assert_eq!(mode(&dir), 0o700, "{}", dir.display());
    }
    assert!(deepest.is_dir());
}

#[test]
fn a_level_below_the_profile_home_that_is_there_is_used_as_it_is() {
    let bench = Bench::new();
    let profile = made(&bench.profile()).to_path_buf();
    let skills = profile.join("skills");
    fs::create_dir(&skills).expect("skills");
    fs::set_permissions(&skills, fs::Permissions::from_mode(0o755)).expect("open");
    ensure(&profile, &skills.join("demo")).expect("made below it");
    assert_eq!(mode(&skills), 0o755, "an existing level is never chmodded");
    assert_eq!(mode(&skills.join("demo")), 0o700);

    // Something in the way that is not a directory is an error, not replaced.
    fs::write(profile.join("blocked"), b"").expect("a file");
    let err = ensure(&profile, &profile.join("blocked").join("x")).expect_err("in the way");
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
    assert!(profile.join("blocked").is_file());
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

/// `text` with every comment line and its tests module cut away.
fn product(text: &str) -> String {
    // The tests module: `#[cfg(test)]` over a `mod tests` of any visibility.
    let lines: Vec<&str> = text.lines().collect();
    let cut = lines
        .windows(2)
        .position(|pair| {
            let next = pair[1].trim_start();
            pair[0].trim() == "#[cfg(test)]"
                && ["mod tests", "pub mod tests", "pub(crate) mod tests"]
                    .iter()
                    .any(|head| next.starts_with(head))
        })
        .unwrap_or(lines.len());
    lines[..cut]
        .iter()
        .filter(|line| !line.trim_start().starts_with("//"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

fn source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn every_profile_writer_makes_its_directories_through_this_module() {
    // Each writer that can be the first to need `~/.xfx` makes no directory
    // of its own: one `create_dir_all` there is a home with the umask's mode.
    for (file, through) in [
        (
            "src/provider/profile.rs",
            "crate::profile_home::ensure(dir, dir)?",
        ),
        ("src/mcp/config.rs", "crate::profile_home::ensure(dir, dir)"),
        (
            "src/session/prompt_history.rs",
            "crate::profile_home::ensure(&self.dir, &self.dir)?",
        ),
        (
            "src/skills/command.rs",
            "super::install::ensure_managed(managed, &dir)",
        ),
        (
            "src/skills/install.rs",
            "crate::profile_home::ensure(profile_of(managed)?, dir)",
        ),
    ] {
        let code = product(&source(file));
        for plain in ["create_dir_all(", "fs::create_dir(", "DirBuilder"] {
            assert!(
                !code.contains(plain),
                "{file} makes a directory with `{plain}`"
            );
        }
        assert!(
            code.contains(through),
            "{file} does not go through `{through}`"
        );
    }
    // The settings writers: every one of the five.
    let profile = product(&source("src/provider/profile.rs"));
    assert_eq!(
        profile
            .matches("crate::profile_home::ensure(dir, dir)?")
            .count(),
        5
    );
    // `/skills add`: the managed root twice, the lock directory, and every
    // directory the copy makes, its transaction's included.
    let install = product(&source("src/skills/install.rs"));
    assert_eq!(
        install.matches("ensure_managed(managed, managed)?").count(),
        2
    );
    assert_eq!(install.matches("crate::profile_home::make_new(").count(), 2);
    assert!(install.contains("crate::profile_home::ensure(parent, &dir)?"));

    // The session store makes `~/.xfx` through this module and nothing above
    // a directory it is handed.
    let store = product(&source("src/session/store.rs"));
    assert!(
        !store.contains("create_dir_all("),
        "the store makes a parent"
    );
    assert!(store.contains("crate::profile_home::ensure(&store.profile_dir, &store.profile_dir)"));
    assert!(store.contains("crate::profile_home::problem(path)"));
}

#[test]
fn no_product_code_but_this_module_makes_a_directory_tree() {
    // `create_dir_all` makes every missing level with the umask's mode; the
    // only product use left is this module's, on a platform without modes.
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut offenders = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("a listing") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "tests") {
                    pending.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs")
                || path.file_name().is_some_and(|name| name == "tests.rs")
            {
                continue;
            }
            let code = product(&fs::read_to_string(&path).expect("a source"));
            if code.contains("create_dir_all(") && !path.ends_with("src/profile_home.rs") {
                offenders.push(path.display().to_string());
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:?}");
}
