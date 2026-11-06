//! Every file the user's git reads as configuration, so that auto writes none
//! of them ([`super::links`]).
//!
//! An included file *is* the configuration that includes it: a
//! `filter.<driver>.clean`, a `core.fsmonitor`, or a `hook.<name>.command`
//! (git-hook(1): a hook no attribute selects) written there runs on the
//! user's next git command. Two sources, joined:
//!
//! - **git's own answer.** `git config --list --show-origin --includes -z`,
//!   run in each authorized root with the configuration-selecting variables
//!   of xfx's own environment (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`,
//!   `GIT_CONFIG_NOSYSTEM`, `XDG_CONFIG_HOME` -- what the user's git reads) and
//!   the profile's home. Every `file:` origin -- the system file, an
//!   environment-selected global, the repository's `.git/config` and what any
//!   of them includes -- is configuration. The command reads configuration and
//!   nothing else: no hook, monitor, filter, pager, helper or signing program
//!   runs (measured on git 2.54 with each one armed). It cannot see an
//!   include that is not there yet, one whose condition is false today, or a
//!   file with no keys in it, and it blocks on a FIFO, so it runs under a
//!   deadline.
//! - **the static walk.** From git's origins and the files git reads by
//!   default -- `~/.gitconfig`, `~/.config/git/config`,
//!   `$XDG_CONFIG_HOME/git/config`, the two environment-selected files, and
//!   the `config` and `config.worktree` of every git directory of each root
//!   (its repository's, a linked worktree's common one, every submodule's) --
//!   every `include.path` and `includeIf.<condition>.path` is followed,
//!   whatever the condition, recursively. A visit is keyed by the file *and*
//!   the directory its relative includes resolve against, so two hard links
//!   to one file in two directories are two visits.
//!
//! Every failure is a reason to ask, never to clear: git missing or failing
//! (a `safe.directory` refusal included -- a repository owned by another
//! user makes auto ask for every write in it), output it cannot parse, an
//! origin it does not recognize, a configuration xfx cannot read or cannot
//! parse byte for byte as git does, an include chain deeper than
//! [`MAX_INCLUDE_DEPTH`] or wider than [`MAX_INCLUDES_PER_SEED`] below one
//! seed, more than [`MAX_GIT_CONFIG_VISITS`] visits in all.
//!
//! **The bar.** Writing an include, or the file it names, is something auto
//! could do; that is a blocker. Some cases here are preconditions only the
//! user can author -- a hard-linked configuration, an include path no parser
//! but git's reads the same way. Those fail closed because failing closed is
//! cheap, not because auto can reach them.

use std::ffi::{OsStr, OsString};
use std::io::{ErrorKind, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The deepest include chain followed below one seed: git's own limit
/// (`MAX_INCLUDE_DEPTH` in `config.c`), past which git itself refuses the
/// configuration.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 10;

/// The most files one seed's includes reach. Per seed, so that a
/// superproject with many submodules -- each module's `config` a seed of its
/// own -- does not exhaust a bound meant for one configuration's includes.
pub(crate) const MAX_INCLUDES_PER_SEED: usize = 64;

/// The most configuration visits one check makes in all, seeds included.
pub(crate) const MAX_GIT_CONFIG_VISITS: usize = 4096;

/// The most bytes of one configuration file read for its includes.
const MAX_GIT_CONFIG_BYTES: u64 = 1024 * 1024;

/// The most bytes of git's answer read.
const MAX_GIT_ANSWER_BYTES: usize = 4 * 1024 * 1024;

/// How long git's answer may take: it reads files, so anything near this is
/// a FIFO or a hung filesystem.
const GIT_ANSWER_DEADLINE: Duration = Duration::from_secs(5);

/// The variables of xfx's own environment that choose which files the
/// user's git reads as configuration.
const SELECTING_VARIABLES: &[&str] = &[
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_CONFIG_SYSTEM",
    "XDG_CONFIG_HOME",
];

/// What the user's git is told about where its configuration is, as the front
/// end read it from the process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitEnvironment(Vec<(OsString, OsString)>);

impl GitEnvironment {
    /// The configuration-selecting variables of this process's environment.
    pub fn from_process() -> Self {
        Self(
            SELECTING_VARIABLES
                .iter()
                .filter_map(|name| {
                    std::env::var_os(name).map(|value| (OsString::from(name), value))
                })
                .collect(),
        )
    }

    /// The same environment with `name` set to `value`; for a test.
    pub fn with(mut self, name: &str, value: impl Into<OsString>) -> Self {
        self.0.retain(|(set, _)| set != name);
        self.0.push((name.into(), value.into()));
        self
    }

    fn get(&self, name: &str) -> Option<&OsStr> {
        self.0
            .iter()
            .find(|(set, _)| set == name)
            .map(|(_, value)| value.as_os_str())
    }
}

/// Every configuration file the user's git reads for `roots`, existing or
/// not, each once: git's answer, then the static walk.
pub(crate) fn configuration_files(
    roots: &[PathBuf],
    home: Option<&Path>,
    xdg_config_home: Option<&Path>,
    environment: &GitEnvironment,
) -> Result<Vec<PathBuf>, String> {
    let mut seeds: Vec<PathBuf> = Vec::new();
    for root in roots {
        // The only seed for what git alone knows of: the system file, and an
        // environment-selected `XDG_CONFIG_HOME` the front end did not also
        // name as a base.
        seeds.extend(git_answer(root, home, environment)?);
    }
    if let Some(home) = home {
        seeds.push(home.join(".gitconfig"));
        seeds.push(home.join(".config/git/config"));
    }
    if let Some(config) = xdg_config_home {
        seeds.push(config.join("git/config"));
    }
    for name in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        if let Some(value) = environment.get(name).filter(|value| !value.is_empty()) {
            seeds.extend(expand(value.as_bytes(), Path::new("/"), home)?);
        }
    }
    for root in roots {
        for git_dir in super::git_attributes::repository_dirs(root)? {
            seeds.push(git_dir.join("config"));
            seeds.push(git_dir.join("config.worktree"));
        }
    }
    let mut walk = StaticWalk {
        home,
        visited: Vec::new(),
        found: Vec::new(),
    };
    for seed in seeds {
        let mut includes = MAX_INCLUDES_PER_SEED;
        walk.visit(&seed, 0, &mut includes)?;
    }
    Ok(walk.found)
}

/// Runs `git <args>` in `cwd` with exactly `environment`, under the
/// deadline: its exit status and standard output.
pub(crate) fn run_git(
    cwd: &Path,
    args: &[&str],
    environment: &[(OsString, OsString)],
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    let path = environment
        .iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let git = std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .ok_or("git is not on PATH, so what it would do is unknown")?;
    let mut command = crate::process::command(&git);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", &path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = crate::process::spawn(&mut command)
        .map_err(|err| format!("git could not be started: {err}"))?;
    let mut stdout = child.stdout.take().ok_or("git's output was not captured")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        (&mut stdout)
            .take(MAX_GIT_ANSWER_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < GIT_ANSWER_DEADLINE => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "git did not finish reading its configuration in {} s",
                    GIT_ANSWER_DEADLINE.as_secs()
                ));
            }
            Err(err) => return Err(format!("git could not be waited for: {err}")),
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "git's output could not be read".to_string())?
        .map_err(|err| format!("git's output could not be read: {err}"))?;
    if bytes.len() > MAX_GIT_ANSWER_BYTES {
        return Err("git's answer is larger than xfx reads".to_string());
    }
    Ok((status, bytes))
}

/// The variables every query of git's configuration runs with: no pager, no
/// trace, a fixed locale.
fn query_environment(home: Option<&Path>) -> Vec<(OsString, OsString)> {
    let mut environment: Vec<(OsString, OsString)> = [
        ("LANG", "C"),
        ("LC_ALL", "C"),
        ("GIT_PAGER", "cat"),
        ("PAGER", "cat"),
        ("GIT_TRACE2", "0"),
        ("GIT_TRACE2_EVENT", "0"),
        ("GIT_TRACE2_PERF", "0"),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect();
    if let Some(home) = home {
        environment.push(("HOME".into(), home.as_os_str().to_owned()));
    }
    environment
}

/// The `file:` origins git reports for `root`, absolute.
fn git_answer(
    root: &Path,
    home: Option<&Path>,
    environment: &GitEnvironment,
) -> Result<Vec<PathBuf>, String> {
    let mut variables = query_environment(home);
    variables.extend(environment.0.iter().cloned());
    let (status, bytes) = run_git(
        root,
        &["config", "--list", "--show-origin", "--includes", "-z"],
        &variables,
    )?;
    if !status.success() {
        return Err(format!(
            "git could not list its configuration in `{}` ({status})",
            root.display()
        ));
    }
    origins(&bytes, root)
}

/// The `file:` origins in git's `--show-origin -z` output: `origin NUL
/// key LF value NUL`, again and again. A relative path is relative to git's
/// working directory.
fn origins(bytes: &[u8], cwd: &Path) -> Result<Vec<PathBuf>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let body = bytes
        .strip_suffix(b"\0")
        .ok_or("git's configuration listing did not end with a NUL")?;
    let fields: Vec<&[u8]> = body.split(|byte| *byte == 0).collect();
    if !fields.len().is_multiple_of(2) {
        return Err("git's configuration listing ended inside an entry".to_string());
    }
    let mut found = Vec::new();
    for pair in fields.chunks(2) {
        let origin = pair[0];
        if let Some(path) = origin.strip_prefix(b"file:") {
            if path.is_empty() {
                return Err("git named a configuration file with no path".to_string());
            }
            let path = Path::new(OsStr::from_bytes(path));
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            if !found.contains(&path) {
                found.push(path);
            }
        } else if !(origin.starts_with(b"command line:") || origin.starts_with(b"blob:")) {
            return Err(format!(
                "git named a configuration origin xfx does not know: `{}`",
                String::from_utf8_lossy(origin)
            ));
        }
    }
    Ok(found)
}

/// Whether the configuration the plan's own git reads in `cwd` defines a
/// hook -- `hook.<name>.command` -- which `core.hooksPath` does not switch off
/// and which `git diff` runs on the index write it makes even with optional
/// locks off (measured on git 2.54).
pub(crate) fn configured_hooks(
    cwd: &Path,
    environment: &[(String, String)],
) -> Result<bool, String> {
    let environment: Vec<(OsString, OsString)> = environment
        .iter()
        .map(|(name, value)| (name.into(), value.into()))
        .chain(
            query_environment(None)
                .into_iter()
                .filter(|(name, _)| name != "LANG" && name != "LC_ALL"),
        )
        .collect();
    let (status, _) = run_git(
        cwd,
        &[
            "config",
            "--includes",
            "-z",
            "--get-regexp",
            r"^hook\..*\.command$",
        ],
        &environment,
    )?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!("git could not read its hooks ({status})")),
    }
}

struct StaticWalk<'a> {
    home: Option<&'a Path>,
    /// (the file, the directory its relative includes resolve against).
    visited: Vec<((u64, u64), PathBuf)>,
    found: Vec<PathBuf>,
}

impl StaticWalk<'_> {
    /// `config` is configuration; so is everything it includes. `depth` is
    /// how many includes led here from the seed, and `includes` how many more
    /// files that seed's includes may reach.
    fn visit(&mut self, config: &Path, depth: usize, includes: &mut usize) -> Result<(), String> {
        if !self.found.iter().any(|found| found == config) {
            self.found.push(config.to_path_buf());
        }
        let name = config.display();
        if depth > MAX_INCLUDE_DEPTH {
            return Err(format!(
                "`{name}` is more than {MAX_INCLUDE_DEPTH} includes deep"
            ));
        }
        if depth > 0 {
            *includes = includes.checked_sub(1).ok_or_else(|| {
                format!("one configuration includes more than {MAX_INCLUDES_PER_SEED} files")
            })?;
        }
        let metadata = match std::fs::metadata(config) {
            Ok(metadata) => metadata,
            Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                return Ok(())
            }
            Err(err) => return Err(format!("`{name}` cannot be read: {err}")),
        };
        if !metadata.is_file() {
            // `/dev/null` is what `GIT_CONFIG_GLOBAL=/dev/null` names, and git
            // reads it as empty. Anything else that is not a regular file --
            // a FIFO above all -- is not something to open and wait on.
            if config == Path::new("/dev/null") {
                return Ok(());
            }
            return Err(format!("`{name}` is not a regular file"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(config)
            .and_then(|file| file.take(MAX_GIT_CONFIG_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|err| format!("`{name}` cannot be read: {err}"))?;
        if bytes.len() as u64 > MAX_GIT_CONFIG_BYTES {
            return Err(format!(
                "`{name}` is larger than {MAX_GIT_CONFIG_BYTES} bytes"
            ));
        }
        let paths = include_paths(&bytes).map_err(|why| format!("`{name}`: {why}"))?;
        // Relative includes resolve against the directory git opened the
        // file from; a link's real directory is read too, at no cost.
        let mut directories = vec![config
            .parent()
            .ok_or_else(|| format!("`{name}` has no directory"))?
            .to_path_buf()];
        if let Some(real) = std::fs::canonicalize(config)
            .ok()
            .and_then(|real| real.parent().map(Path::to_path_buf))
        {
            if !directories.contains(&real) {
                directories.push(real);
            }
        }
        for directory in directories {
            let key = {
                use std::os::unix::fs::MetadataExt;
                ((metadata.dev(), metadata.ino()), directory.clone())
            };
            if self.visited.contains(&key) {
                continue;
            }
            self.visited.push(key);
            if self.visited.len() > MAX_GIT_CONFIG_VISITS {
                return Err(format!(
                    "the configuration includes more than {MAX_GIT_CONFIG_VISITS} files"
                ));
            }
            for raw in &paths {
                for path in expand(raw, &directory, self.home)? {
                    self.visit(&path, depth + 1, includes)?;
                }
            }
        }
        Ok(())
    }
}

/// Where an include path points, as git expands it: `~/` from the home
/// directory, a relative path from `directory`.
fn expand(raw: &[u8], directory: &Path, home: Option<&Path>) -> Result<Vec<PathBuf>, String> {
    let shown = String::from_utf8_lossy(raw);
    if raw == b"~" || raw.starts_with(b"~/") {
        let home =
            home.ok_or_else(|| format!("`{shown}` names a home directory xfx was not given"))?;
        let rest = raw[1..].strip_prefix(b"/").unwrap_or(&raw[1..]);
        return Ok(vec![home.join(OsStr::from_bytes(rest))]);
    }
    if raw.starts_with(b"~") || raw.starts_with(b"%(") {
        return Err(format!("`{shown}` is not a path xfx expands"));
    }
    let path = Path::new(OsStr::from_bytes(raw));
    Ok(vec![if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    }])
}

/// Every `include.path` and `includeIf.<condition>.path` value in `text`, in
/// order, as the exact bytes git reads.
///
/// Git's grammar (`config.c`: `git_parse_source`, `get_base_var`,
/// `get_value`, `parse_value`), byte for byte: whitespace is git's `isspace`
/// -- space, tab, CR, LF, nothing else -- a UTF-8 byte-order mark is skipped,
/// and every line git rejects is an error here too. One thing is refused that
/// git accepts: unquoted whitespace other than a space *inside* a value,
/// which git versions have kept and turned into spaces differently, so no
/// single reading is git's.
pub(crate) fn include_paths(text: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let text = text.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(text);
    let mut parser = Parser {
        bytes: text,
        at: 0,
        line: 1,
    };
    // The section's lower-case name, and whether it has a subsection.
    let mut section: Option<(Vec<u8>, bool)> = None;
    let mut paths = Vec::new();
    while let Some(next) = parser.next() {
        match next {
            b'\n' | b' ' | b'\t' | b'\r' => {}
            b'#' | b';' => parser.skip_line(),
            b'[' => section = Some(parser.header()?),
            c if c.is_ascii_alphabetic() => {
                let (key, value) = parser.variable(c)?;
                // `include.path`, and `includeIf.<condition>.path`: git's two
                // include keys, and no other spelling of them.
                let including = match &section {
                    Some((name, false)) => name == b"include",
                    Some((name, true)) => name == b"includeif",
                    None => false,
                };
                if including && key == b"path" {
                    let (value, ambiguous) =
                        value.ok_or_else(|| parser.error("an include `path` with no value"))?;
                    if ambiguous {
                        return Err(parser.error(
                            "unquoted whitespace other than a space inside an include path, which git versions read differently",
                        ));
                    }
                    paths.push(value);
                }
            }
            other => {
                return Err(parser.error(&format!("a line cannot start with byte {other:#04x}")))
            }
        }
    }
    Ok(paths)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    line: usize,
}

fn is_git_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

impl Parser<'_> {
    /// Git's `get_next_char`: CR LF is LF.
    fn next(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        if byte == b'\r' && self.bytes.get(self.at) == Some(&b'\n') {
            self.at += 1;
            self.line += 1;
            return Some(b'\n');
        }
        if byte == b'\n' {
            self.line += 1;
        }
        Some(byte)
    }

    /// The end of the text reads as a LF, as git's does.
    fn next_or_newline(&mut self) -> u8 {
        self.next().unwrap_or(b'\n')
    }

    fn error(&self, what: &str) -> String {
        format!("line {}: {what}", self.line)
    }

    fn skip_line(&mut self) {
        while let Some(byte) = self.next() {
            if byte == b'\n' {
                break;
            }
        }
    }

    /// `[name]`, `[name "sub"]` or `[name.sub]`, after the `[`.
    fn header(&mut self) -> Result<(Vec<u8>, bool), String> {
        let mut name = Vec::new();
        loop {
            let Some(byte) = self.next() else {
                return Err(self.error("a section header with no `]`"));
            };
            match byte {
                b']' => {
                    let dotted = name.contains(&b'.');
                    return Ok((name, dotted));
                }
                b if is_git_space(b) => break,
                b if is_key_byte(b) || b == b'.' => name.push(b.to_ascii_lowercase()),
                _ => return Err(self.error("a malformed section header")),
            }
        }
        // `get_extended_base_var`: blanks, then a quoted subsection.
        let mut byte = b' ';
        while is_git_space(byte) {
            if byte == b'\n' {
                return Err(self.error("an incomplete section header"));
            }
            byte = self.next_or_newline();
        }
        if byte != b'"' {
            return Err(self.error("a subsection that is not quoted"));
        }
        loop {
            match self.next_or_newline() {
                b'\n' => return Err(self.error("an incomplete section header")),
                b'"' => break,
                b'\\' => {
                    let escaped = self.next_or_newline();
                    if escaped == b'\n' {
                        return Err(self.error("an incomplete section header"));
                    }
                }
                _ => {}
            }
        }
        if self.next_or_newline() != b']' {
            return Err(self.error("a malformed section header"));
        }
        Ok((name, true))
    }

    /// A variable line, after its first byte: the lower-case key, and the
    /// value when there is one, with whether git versions read it two ways.
    #[allow(clippy::type_complexity)]
    fn variable(&mut self, first: u8) -> Result<(Vec<u8>, Option<(Vec<u8>, bool)>), String> {
        let mut key = vec![first.to_ascii_lowercase()];
        let mut byte = self.next_or_newline();
        while is_key_byte(byte) {
            key.push(byte.to_ascii_lowercase());
            byte = self.next_or_newline();
        }
        while byte == b' ' || byte == b'\t' {
            byte = self.next_or_newline();
        }
        if byte == b'\n' {
            return Ok((key, None));
        }
        if byte != b'=' {
            return Err(self.error("a key followed by something other than `=`"));
        }
        Ok((key, Some(self.value()?)))
    }

    /// Git's `parse_value`, and whether unquoted whitespace other than a space
    /// sits inside the value -- which git versions have kept and turned into
    /// spaces differently.
    fn value(&mut self) -> Result<(Vec<u8>, bool), String> {
        let mut value = Vec::new();
        let mut ambiguous = false;
        let mut quoted = false;
        let mut comment = false;
        // Unquoted blanks not yet known to be inside the value.
        let mut blanks: Vec<u8> = Vec::new();
        loop {
            let byte = self.next_or_newline();
            if byte == b'\n' {
                if quoted {
                    return Err(self.error("an unterminated quote"));
                }
                return Ok((value, ambiguous));
            }
            if comment {
                continue;
            }
            if is_git_space(byte) && !quoted {
                if !value.is_empty() {
                    blanks.push(byte);
                }
                continue;
            }
            if !quoted && (byte == b';' || byte == b'#') {
                comment = true;
                continue;
            }
            if !blanks.is_empty() {
                ambiguous |= blanks.iter().any(|blank| *blank != b' ');
                value.append(&mut blanks);
            }
            match byte {
                b'\\' => match self.next_or_newline() {
                    b'\n' => {}
                    b't' => value.push(b'\t'),
                    b'b' => value.push(0x08),
                    b'n' => value.push(b'\n'),
                    b'\\' => value.push(b'\\'),
                    b'"' => value.push(b'"'),
                    _ => return Err(self.error("an unknown escape")),
                },
                b'"' => quoted = !quoted,
                0 => return Err(self.error("a NUL byte in a value")),
                other => value.push(other),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(text: &[u8]) -> Vec<Vec<u8>> {
        include_paths(text).expect("parses")
    }

    #[test]
    fn every_include_and_include_if_path_is_read_as_git_reads_it() {
        // `include.sub.path` and a bare `includeIf.path` are not include keys.
        let text = b"\xEF\xBB\xBF# a comment
[user]
\tname = Someone ; trailing
\tpath = not/an/include
[include]
\tpath = ~/.gitconfig.local
[includeIf \"gitdir:~/work/\"]
\tPATH = \"work dir/inc\" # quoted, with a space
[Include] path = same-line
[include.sub]
\tpath = not-an-include-key
[includeIf]
\tpath = nor-this
[include]
\tpath = con\\
tinued
[include]
\tpath = a  b \t
[include]\r\n\tpath = crlf\r\n";
        assert_eq!(
            paths(text),
            [
                &b"~/.gitconfig.local"[..],
                b"work dir/inc",
                b"same-line",
                b"continued",
                b"a  b",
                b"crlf"
            ]
        );
    }

    #[test]
    fn bytes_are_kept_as_bytes() {
        // Not UTF-8, and not whitespace to git: kept exactly.
        assert_eq!(
            paths(b"[include]\n\tpath = caf\xE9\xA0/x\n"),
            [b"caf\xE9\xA0/x".to_vec()]
        );
        // A tab inside another key's value is not this parser's concern.
        assert_eq!(
            paths(b"[core]\n\teditor = vim \\\n\t -f\n[include]\n\tpath = x\n"),
            [b"x".to_vec()]
        );
        // U+3000, IDEOGRAPHIC SPACE: whitespace to Unicode, not to git.
        assert_eq!(
            paths("[include]\n\tpath = a\u{3000}b\n".as_bytes()),
            ["a\u{3000}b".as_bytes().to_vec()]
        );
    }

    #[test]
    fn what_git_would_reject_or_read_two_ways_is_not_guessed_at() {
        for text in [
            &b"[include]\n\tpath = \"open\n"[..],
            b"[include\n",
            b"[include]\n\tpath = a\\q\n",
            b"[include]\n\t= value\n",
            b"[include]\n\tpath\n",
            b"[include]\n\tpath = a\tb\n",
            b"[include]\n\tpath # comment\n",
            b"[include]\n\xC3\xA9 = x\n",
        ] {
            assert!(
                include_paths(text).is_err(),
                "{:?}",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn this_parser_and_git_agree_wherever_this_parser_answers() {
        // The differential: each text through `git config --file`, which
        // reads it with git's own parser. Where this parser answers, the
        // answer is git's, byte for byte; where it refuses, auto asks.
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join("git"))
            .find(|git| git.is_file())
            .expect("git on PATH: this test runs the real one");
        let dir = tempfile::tempdir().expect("a dir");
        let cases: &[&[u8]] = &[
            b"[include]\n\tpath = plain\n",
            b"[include]\n\tpath = \"quoted  twice\" \n",
            b"[include]\n\tpath = caf\xE9\n",
            "[include]\n\tpath = a\u{3000}b\n".as_bytes(),
            b"[include]\n\tpath = a\tb\n",
            b"[include]\n\tpath = esc\\\"aped\\\\x\n",
            b"[includeIf \"gitdir:/x/\"]\n\tpath = cond\n[include.sub]\n\tpath = no\n",
            b"[include]\n\tpath = one\n\tpath = two ; c\n",
            b"\xEF\xBB\xBF[include]\r\n\tpath = bom\r\n",
            b"[include]\n\tpath = con\\\ntinued\n",
        ];
        let mut answered = 0;
        for (index, text) in cases.iter().enumerate() {
            let file = dir.path().join(format!("case{index}"));
            std::fs::write(&file, text).expect("a case");
            let output = std::process::Command::new(&git)
                .args(["config", "--file"])
                .arg(&file)
                .args(["--no-includes", "-z", "--get-regexp"])
                .arg(r"^(include\.path|includeif\..*\.path)$")
                .env_clear()
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .expect("git runs");
            let Ok(ours) = include_paths(text) else {
                continue;
            };
            assert!(
                output.status.success(),
                "case {index}: git refused what this parser read"
            );
            let theirs: Vec<Vec<u8>> = output
                .stdout
                .split(|byte| *byte == 0)
                .filter(|record| !record.is_empty())
                .map(|record| {
                    let newline = record
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .unwrap_or(record.len());
                    record.get(newline + 1..).unwrap_or_default().to_vec()
                })
                .collect();
            assert_eq!(ours, theirs, "case {index}");
            answered += 1;
        }
        assert!(answered >= 8, "only {answered} cases were answered");
    }

    #[test]
    fn includes_are_bounded_by_depth_and_by_width_per_seed() {
        let home = tempfile::tempdir().expect("a home");
        let home = home.path().canonicalize().expect("canonical");
        let files = || configuration_files(&[], Some(&home), None, &GitEnvironment::default());
        // Eleven deep: one past git's own limit.
        std::fs::write(home.join(".gitconfig"), "[include]\n\tpath = d1\n").expect("a");
        for depth in 1..=11 {
            std::fs::write(
                home.join(format!("d{depth}")),
                format!("[include]\n\tpath = d{}\n", depth + 1),
            )
            .expect("a link in the chain");
        }
        let deep = files().expect_err("too deep");
        assert!(deep.contains("includes deep"), "{deep}");
        // Ten deep is git's own limit, and fine.
        std::fs::write(home.join("d10"), "[user]\n\tname = u\n").expect("the end");
        assert!(files().is_ok());
        // Seventy files from one configuration is past the width bound.
        let wide: String = (0..70)
            .map(|index| format!("[include]\n\tpath = w{index}\n"))
            .collect();
        std::fs::write(home.join(".gitconfig"), wide).expect("wide");
        let wide = files().expect_err("too wide");
        assert!(wide.contains("more than 64 files"), "{wide}");
    }

    #[test]
    fn origins_are_read_from_gits_listing_and_an_unknown_one_asks() {
        let cwd = Path::new("/w");
        let listing =
            b"file:/etc/gitconfig\0core.x\n1\0file:.git/config\0core.y\n2\0command line:\0a.b\nc\0";
        assert_eq!(
            origins(listing, cwd).expect("parses"),
            [
                PathBuf::from("/etc/gitconfig"),
                PathBuf::from("/w/.git/config")
            ]
        );
        assert!(origins(b"standard input:\0a.b\nc\0", cwd).is_err());
        assert!(origins(b"file:/x\0", cwd).is_err());
        assert!(origins(b"file:/x\0a.b\nc", cwd).is_err());
    }
}
