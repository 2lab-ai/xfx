//! RED harness for P5-PROFILE-DIR on the base: the writers as base has them.
//! The profile home is made private by every writer that can make it, and an
//! existing one that is not private is refused by every writer, in the session
//! store's words, and never `chmod`ed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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

/// Every product writer that can be the first to need `~/.xfx`, each run as
/// its surface runs it.
fn writers() -> Vec<(&'static str, fn(&Bench) -> Answer)> {
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


/// The store's own sentence, as base says it.
fn refusal(path: &Path, detail: &str) -> String {
    format!("{} cannot hold session state: {detail}", path.display())
}

/// What `writer` is expected to say for a home refused with `store`.
fn refused_as(writer: &str, store: &str, name: &str) -> String {
    match writer {
        "the MCP profile" => name.to_string(),
        "/skills create" => format!("Failed to create skill 'demo': {store}."),
        "/skills add" => format!("Failed to install: {store}."),
        _ => store.to_string(),
    }
}

#[test]
fn red_every_writer_makes_a_missing_profile_home_private_and_the_store_opens_it() {
    let mut failures = Vec::new();
    for (writer, write) in writers() {
        let bench = Bench::new();
        let answer = write(&bench);
        let profile = bench.profile();
        let modes: Vec<String> = if profile.is_dir() {
            directories(&profile)
                .iter()
                .map(|dir| format!("{:o} {}", mode(dir), dir.strip_prefix(&bench.home).unwrap().display()))
                .collect()
        } else {
            Vec::new()
        };
        let open = SessionStore::open(&profile).map(drop).map_err(|err| err.to_string());
        let private = directories(&profile).iter().all(|dir| mode(dir) == 0o700);
        println!("RED {writer}: answer={answer:?} dirs={modes:?} store_after={open:?}");
        if answer.is_err() || !private || open.is_err() {
            failures.push(writer);
        }
    }
    assert!(failures.is_empty(), "writers that left a home that is not private or not openable: {failures:?}");
}

#[test]
fn red_an_open_profile_home_is_refused_by_every_writer_and_never_chmodded() {
    let mut failures = Vec::new();
    for (writer, write) in writers() {
        let bench = Bench::new();
        let profile = bench.profile();
        fs::create_dir(&profile).expect("a profile home");
        fs::set_permissions(&profile, fs::Permissions::from_mode(0o755)).expect("open it");
        let store = SessionStore::open(&profile).expect_err("the store refuses").to_string();
        let answer = write(&bench);
        let written = fs::read_dir(&profile).expect("a listing").count();
        println!("RED {writer}: answer={answer:?} mode_after={:o} entries_written={written}", mode(&profile));
        if answer != Err(refused_as(writer, &store, "PrivateStatePermissionsUnsupported")) || written != 0 {
            failures.push(writer);
        }
    }
    assert!(failures.is_empty(), "writers that wrote into an open home: {failures:?}");
}

#[test]
fn red_a_linked_profile_home_is_refused_by_every_writer() {
    let mut failures = Vec::new();
    for (writer, write) in writers() {
        let bench = Bench::new();
        let profile = bench.profile();
        let elsewhere = bench.root.join("elsewhere");
        fs::create_dir(&elsewhere).expect("a directory");
        fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).expect("private");
        std::os::unix::fs::symlink(&elsewhere, &profile).expect("a link");
        let store = SessionStore::open(&profile).expect_err("the store refuses").to_string();
        let answer = write(&bench);
        let written = fs::read_dir(&elsewhere).expect("a listing").count();
        println!("RED {writer}: answer={answer:?} entries_written_where_it_points={written}");
        if answer != Err(refused_as(writer, &store, "DurablePathUnsafe")) || written != 0 {
            failures.push(writer);
        }
    }
    assert!(failures.is_empty(), "writers that wrote through a linked home: {failures:?}");
}

#[test]
fn red_a_store_opened_with_no_home_makes_it_private() {
    let bench = Bench::new();
    let home = bench.root.join("absent").join("home");
    let profile = home.join(".xfx");
    SessionStore::open(&profile).expect("opened");
    for dir in [bench.root.join("absent"), home.clone(), profile.clone()] {
        println!("RED store: {:o} {}", mode(&dir), dir.strip_prefix(&bench.root).unwrap().display());
    }
    assert_eq!((mode(&bench.root.join("absent")), mode(&home)), (0o700, 0o700));
}
