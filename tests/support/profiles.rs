//! Profiles fx v0.0.16 refuses at launch, and their neighbours it starts with.
//!
//! Each was run against the oracle
//! (`.prd/tui-phase5/receipts/profile-refuse-at-launch/probe.sh`,
//! `.prd/tui-phase5/receipts/profile-refuse-typed-keys/probe.sh`,
//! `fx-fixtures.txt`): fx exits 1 with `fx: InvalidProfileConfiguration`
//! (`src/core/app/app_lifecycle.zig:689-703@481104fd`), or with the error its
//! load fails with ([`STOPPED`]).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What stands at `~/.xfx/settings.json`, or at `~/.xfx`, in one fixture.
#[derive(Clone, Copy, Debug)]
pub enum Profile {
    /// The settings file holds exactly these bytes.
    Text(&'static str),
    /// The settings file's `workspaces` entry for the workspace is this JSON.
    Entry(&'static str),
    /// A valid object of exactly this many bytes: 65,536 is fx's ceiling
    /// (`settings_store.zig:17`, `:747`), 65,537 one past it.
    Sized(usize),
    /// A valid object nobody may read.
    Unreadable,
    /// A valid object anyone may write and nobody may read (`0o222`): the
    /// open fails before fx looks at the mode (`settings_store.zig:721-746`).
    WriteOnly,
    /// The file holds these bytes and group may write to it (`0o664`): fx
    /// does not read it (`:744-746`).
    OpenFile(&'static str),
    /// `~/.xfx` is `0o770` and its file, private, holds these bytes: fx does
    /// not read the profile (`:381-383`).
    OpenDir(&'static str),
    /// A valid file with a second hard link elsewhere in the home
    /// (`io.zig:281-283`).
    HardLinked,
    /// The same, nobody may read it: unsafe on fx's look before the open
    /// (`io.zig:218-224`), so not an open that fails.
    HardLinkedUnreadable,
    /// `~/.xfx` itself a regular file (`settings_store.zig:365`, `:376`).
    FileForDir,
    /// A FIFO where the file belongs (`io.zig:278-280`).
    Fifo,
    /// A valid profile in a `~/.xfx` nobody may list (`0o300`): its open
    /// fails (`settings_store.zig:351-367`).
    UnlistableDir,
    /// A valid profile under a HOME nobody may list (`0o300`): fx opens HOME
    /// first (`settings_store.zig:342-348`).
    UnlistableHome,
    /// A `model` of 300 bytes: past xfx's own 200, inside fx's 1024.
    LongModel,
    /// A directory where the file belongs.
    Directory,
    /// A link to a valid file elsewhere in the home.
    LinkedFile,
    /// `~/.xfx` itself a link to a directory holding a valid file.
    LinkedDir,
    /// A valid profile, and a project file holding these bytes.
    Project(&'static str),
}

/// Every launch refuses these: the TUI, the line shell and `ask`.
pub const REJECTED: &[(&str, Profile)] = &[
    (
        "not JSON",
        Profile::Text(r#"{"permission_mode": "auto", "sandbox": "#),
    ),
    ("not an object", Profile::Text("[1, 2]")),
    ("one byte past the ceiling", Profile::Sized(64 * 1024 + 1)),
    ("unreadable", Profile::Unreadable),
    ("write-only, open to group and other", Profile::WriteOnly),
    (
        "a profile directory nobody may list",
        Profile::UnlistableDir,
    ),
    ("a HOME nobody may list", Profile::UnlistableHome),
    (
        "a model with a blank before it",
        Profile::Text(r#"{"model": " openai/gpt-5"}"#),
    ),
    (
        "a model that is not a string",
        Profile::Text(r#"{"model": 5}"#),
    ),
    (
        "workspaces that is not an object",
        Profile::Text(r#"{"workspaces": []}"#),
    ),
    ("an entry that is not an object", Profile::Entry("5")),
    (
        "an entry's model with a blank before it",
        Profile::Entry(r#"{"model": " openai/gpt-5"}"#),
    ),
    (
        "a switch that is not a bool",
        Profile::Text(r#"{"fast_mode": "yes"}"#),
    ),
    (
        "a permission action fx does not read",
        Profile::Text(r#"{"permission": {"bash": "perhaps"}}"#),
    ),
    (
        "an entry's switch that is not a bool",
        Profile::Entry(r#"{"fast_mode": "yes"}"#),
    ),
    (
        "a key twice",
        Profile::Text(r#"{"permission_mode": "auto", "permission_mode": "ask"}"#),
    ),
    (
        "a key twice inside a table",
        Profile::Text(r#"{"permission": {"bash": "allow", "bash": "deny"}}"#),
    ),
];

/// fx's load itself fails on these, and every launch and `status` exit 1
/// with its error's name (`config_runtime.zig:1011-1015`, `:350-355`).
pub const STOPPED: &[(&str, Profile, &str)] = &[
    (
        "a provider nothing defines",
        Profile::Text(r#"{"provider": "not-a-provider"}"#),
        "UnknownConfiguredProvider",
    ),
    (
        "an entry's provider nothing defines",
        Profile::Entry(r#"{"provider": "not-a-provider"}"#),
        "UnknownConfiguredProvider",
    ),
    (
        "a provider that is not a string",
        Profile::Text(r#"{"provider": 5}"#),
        "InvalidProviderType",
    ),
    (
        "a models entry with a blank before it",
        Profile::Text(r#"{"models": {"gateway": " openai/gpt-5"}}"#),
        "InvalidModelValue",
    ),
    (
        "models that is not an object",
        Profile::Text(r#"{"models": 5}"#),
        "InvalidModelType",
    ),
    (
        "a provider its connections do not name",
        Profile::Text(r#"{"provider": "missing", "providers": {}}"#),
        "UnknownConfiguredProvider",
    ),
    (
        "connections that are not an object",
        Profile::Text(r#"{"providers": 5}"#),
        "InvalidObject",
    ),
];

/// Every profile every launch refuses, with the error it is refused with.
pub fn every_launch_refuses() -> impl Iterator<Item = (&'static str, Profile, &'static str)> {
    REJECTED
        .iter()
        .map(|(name, profile)| (*name, *profile, "InvalidProfileConfiguration"))
        .chain(STOPPED.iter().copied())
}

/// Whether `stderr` holds a launch's refusal: `xfx: ` and an error's name.
pub fn refused(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        line.strip_prefix("xfx: ")
            .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphabetic()))
    })
}

/// Only `ask` refuses these (fx's `durable_path_unsafe`); an interactive
/// launch goes on.
pub const UNSAFE: &[(&str, Profile)] = &[
    ("a directory", Profile::Directory),
    ("a linked file", Profile::LinkedFile),
    ("a linked profile", Profile::LinkedDir),
    ("a hard-linked file", Profile::HardLinked),
    (
        "a hard-linked file nobody may read",
        Profile::HardLinkedUnreadable,
    ),
    ("a FIFO", Profile::Fifo),
    (
        "a file where the profile directory belongs",
        Profile::FileForDir,
    ),
];

/// fx starts with these.
pub const STARTED: &[(&str, Profile)] = &[
    (
        "a model with a space inside it",
        Profile::Text(r#"{"model": "openai/gpt 5"}"#),
    ),
    ("a 300-byte model", Profile::LongModel),
    (
        "a retired key before a bad model",
        Profile::Text(r#"{"skill_match_fuzzy": true, "model": " openai/gpt-5"}"#),
    ),
    (
        "a malformed project file",
        Profile::Project(r#"{"max_agent_steps": "#),
    ),
    (
        "a project file with a key twice",
        Profile::Project(r#"{"max_agent_steps": 1, "max_agent_steps": 2}"#),
    ),
    (
        "context limits fx does not read, before a switch that is not a bool",
        Profile::Text(r#"{"context_limits": "x", "fast_mode": "yes"}"#),
    ),
    (
        "skill authorities fx does not read, before notifications it would refuse",
        Profile::Text(r#"{"skill_symlink_authorities": 5, "notifications": 5}"#),
    ),
    (
        "an entry's statusLine that is not an object",
        Profile::Entry(r#"{"statusLine": 5}"#),
    ),
    ("a key only xfx reads", Profile::Text(r#"{"llmux_url": 5}"#)),
    (
        "a models entry with a space inside it",
        Profile::Text(r#"{"models": {"gateway": "openai/gpt 5"}}"#),
    ),
    (
        "a provider in capitals",
        Profile::Text(r#"{"provider": "GATEWAY"}"#),
    ),
    ("exactly the ceiling", Profile::Sized(64 * 1024)),
    (
        "torn JSON group may write",
        Profile::OpenFile(r#"{"permission_mode": "auto", "sandbox": "#),
    ),
    (
        "a bad model group may write",
        Profile::OpenFile(r#"{"model":" a"}"#),
    ),
    (
        "torn JSON in a profile directory group may write",
        Profile::OpenDir(r#"{"permission_mode": "auto", "sandbox": "#),
    ),
    (
        "a valid profile",
        Profile::Text(r#"{"permission_mode": "auto"}"#),
    ),
];

/// Lays `profile` down under `home` for `workspace`. False when it cannot be
/// made here: a reader the mode bits do not stop (root) cannot have an
/// unreadable file or an unlistable directory. [`unlock`] undoes the modes
/// that would keep the sandbox from being removed.
pub fn lay(home: &Path, workspace: &Path, profile: Profile) -> bool {
    let dir = home.join(".xfx");
    let file = dir.join("settings.json");
    let private = |path: &Path, mode: u32| {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set a mode");
    };
    if !matches!(profile, Profile::LinkedDir | Profile::FileForDir) {
        fs::create_dir_all(&dir).expect("the profile dir");
        private(&dir, 0o700);
    }
    let write = |text: &str| {
        fs::write(&file, text).expect("write the profile");
        private(&file, 0o600);
    };
    match profile {
        Profile::Text(text) => write(text),
        Profile::Entry(entry) => {
            let key = serde_json::to_string(&workspace.display().to_string()).expect("a key");
            write(&format!(r#"{{"workspaces": {{{key}: {entry}}}}}"#));
        }
        Profile::Sized(bytes) => {
            let text = format!(r#"{{"pad":"{}"}}"#, "x".repeat(bytes - 10));
            assert_eq!(text.len(), bytes);
            write(&text);
        }
        Profile::Unreadable | Profile::WriteOnly => {
            write(r#"{"permission_mode": "auto"}"#);
            let mode = if matches!(profile, Profile::Unreadable) {
                0o000
            } else {
                0o222
            };
            private(&file, mode);
            if fs::read(&file).is_ok() {
                return false;
            }
        }
        Profile::OpenFile(text) => {
            write(text);
            private(&file, 0o664);
        }
        Profile::OpenDir(text) => {
            write(text);
            private(&dir, 0o770);
        }
        Profile::HardLinked | Profile::HardLinkedUnreadable => {
            write(r#"{"permission_mode": "auto"}"#);
            fs::hard_link(&file, home.join("other.json")).expect("the second link");
            if matches!(profile, Profile::HardLinkedUnreadable) {
                private(&file, 0o000);
                if fs::read(&file).is_ok() {
                    return false;
                }
            }
        }
        Profile::FileForDir => fs::write(&dir, "{}").expect("a file where the directory belongs"),
        Profile::Fifo => {
            let made = xfx::process::status(std::process::Command::new("mkfifo").arg(&file))
                .expect("run mkfifo");
            assert!(made.success(), "a FIFO");
        }
        Profile::UnlistableDir | Profile::UnlistableHome => {
            write(r#"{"permission_mode": "auto"}"#);
            let locked = if matches!(profile, Profile::UnlistableDir) {
                dir.clone()
            } else {
                home.to_path_buf()
            };
            private(&locked, 0o300);
            if fs::read_dir(&locked).is_ok() {
                unlock(home);
                return false;
            }
        }
        Profile::LongModel => write(&format!(r#"{{"model": "openai/{}"}}"#, "m".repeat(293))),
        Profile::Directory => fs::create_dir(&file).expect("a directory"),
        Profile::LinkedFile => {
            let real = home.join("real.json");
            fs::write(&real, r#"{"permission_mode": "auto"}"#).expect("the real file");
            private(&real, 0o600);
            std::os::unix::fs::symlink("../real.json", &file).expect("the link");
        }
        Profile::LinkedDir => {
            let real = home.join("real-profile");
            fs::create_dir_all(&real).expect("the real profile");
            private(&real, 0o700);
            fs::write(real.join("settings.json"), r#"{"permission_mode": "auto"}"#)
                .expect("the real file");
            private(&real.join("settings.json"), 0o600);
            std::os::unix::fs::symlink("real-profile", &dir).expect("the link");
        }
        Profile::Project(text) => {
            write(r#"{"permission_mode": "auto"}"#);
            fs::write(workspace.join(".xfx.json"), text).expect("the project file");
        }
    }
    true
}

/// Gives HOME and `~/.xfx` back to their owner, so the sandbox can be removed.
pub fn unlock(home: &Path) {
    let _ = fs::set_permissions(home, fs::Permissions::from_mode(0o700));
    let dir = home.join(".xfx");
    if fs::symlink_metadata(&dir).is_ok_and(|metadata| metadata.is_dir()) {
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
}

/// Every entry under `root`, not following links, with its size and
/// modification time: what a run made or changed shows here, and nothing is
/// opened, so an unreadable file is listed too. A directory its owner may not
/// list is listed with its mode given back for the look and then put back,
/// which moves no modification time. A directory it cannot list even so is a
/// panic, never an empty listing: an empty home is a listing of its own.
pub fn snapshot(root: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
        let metadata = fs::metadata(dir).expect("a directory to list");
        let mode = metadata.permissions();
        let locked = fs::read_dir(dir).is_err();
        if locked {
            fs::set_permissions(dir, fs::Permissions::from_mode(mode.mode() | 0o500))
                .expect("list a locked directory");
        }
        let entries: Vec<_> = fs::read_dir(dir).expect("list").collect();
        if locked {
            fs::set_permissions(dir, mode).expect("lock it again");
        }
        for entry in entries {
            let path = entry.expect("an entry").path();
            let metadata = fs::symlink_metadata(&path).expect("metadata");
            let name = path
                .strip_prefix(root)
                .expect("under the root")
                .to_path_buf();
            out.push((name, metadata.len(), metadata.modified().ok()));
            if metadata.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}
