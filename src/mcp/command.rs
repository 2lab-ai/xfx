//! `/mcp`'s verbs, in fx v0.0.16's words.
//!
//! A port of `vercel-labs/fx@481104fd src/core/mcp_host/verbs.zig` for the
//! shell surface (`.shell`, `/mcp`): `list`, `show`, `add`, `remove`,
//! `login`, `logout`, `approve` and `reject`, the overview `/mcp help`, `-h`
//! and `--help` print, each verb's usage, and the sentence each verb fx v1
//! had now says about where it went (`parse`, `verbs.zig:60-174`). The line
//! is split as fx splits it (`command_lex.tokenize_argv`,
//! `src/core/shell_command/command_lex.zig:140-226@481104fd`, [`tokenize`]),
//! and what a verb prints is what fx's does: its standard output one neutral
//! notice under `mcp`, its standard error one warning
//! (`VerbJob.run`, `src/core/app/app_mcp_runtime.zig:585-602@481104fd`).
//! `fx` in a sentence is `xfx` here, and `~/.fx/mcp.json` is `~/.xfx/mcp.json`.
//!
//! **In typed order.** fx runs each typed `/mcp` on a thread of its own and
//! reports it when it finishes, so a `show` waiting on a server is reported
//! after a later command that did not wait (`p5-mcp-verbs`). xfx answers them
//! in the order they were typed, on the MCP runtime's one worker thread; see
//! `docs/parity.md`.
//!
//! **Gated, and said so.** Signing in to a remote server opens a browser and
//! stores a token, which is a decision this build has not been given
//! (`docs/parity.md`). `login` makes every check fx makes before it would
//! sign in -- the server exists, it is remote, it is not held back -- and
//! then says [`AUTH_UNAVAILABLE`]; `add slack`, which adds Slack's server to
//! sign in to, says the same and writes nothing. xfx keeps no login, so
//! `logout` of a remote server is fx's `X wasn't signed in.`, which is true.
//!
//! **What reaches the screen.** A server's name, a URL, a command and a flag
//! someone typed are spelled by [`safe`] (fx's `encodeTerminalSafeInline(..,
//! 240)`) wherever they are said back -- fx spells only what came from a
//! file; xfx spells what was typed too. A `--header` or `--env` value is a
//! secret and is never said back: fx's sentences name the server and the
//! file, and nothing else.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::config::{self, Admission, Change, Entry, File, Source, Transport, TrustAction};
use super::runtime::{Connection, McpRuntime, Origin, ServerView};

/// The topic every answer is filed under, usage lines included (`VerbJob.run`
/// pushes both writers to the `mcp` sink, `app_mcp_runtime.zig:595-601`).
pub const TOPIC: &str = "mcp";

/// The sign-in leaf's answer while it is gated (`mcp.zig:154-160@c1db9191`).
pub const AUTH_UNAVAILABLE: &str = "Interactive MCP authentication is unavailable here.";

/// What a line [`tokenize`] refuses says (`runVerbs`,
/// `app_mcp_runtime.zig:525-531@481104fd`).
pub const UNMATCHED: &str =
    "/mcp: that command line has an unmatched quote or a character xfx doesn't take here.";

/// Slack's server, which a bare `add slack` names (`slack.server_url`,
/// `src/core/mcp_host/slack.zig@481104fd`).
const SLACK_URL: &str = "https://mcp.slack.com/mcp";

/// The shell surface's program name (`Surface.prog`, `verbs.zig:28-33`).
const PROG: &str = "/mcp";

/// How long `show` waits for a server that is coming up: its own start's
/// timeout, at most this. fx waits until the try ends, or fx quits; xfx's
/// worker answers every `/mcp` in turn, so the wait has an end.
const SHOW_WAIT_MAX: Duration = Duration::from_secs(60);

/// fx's cut for a name or a line said back (`safe`, `verbs.zig:469-472`).
const SAFE_MAX_BYTES: usize = 240;

/// One of the verbs (`Verb`, `verbs.zig:37`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    List,
    Show,
    Add,
    Remove,
    Login,
    Logout,
    Approve,
    Reject,
}

impl Verb {
    /// `std.meta.stringToEnum(Verb, ..)`: the word exactly.
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "list" => Self::List,
            "show" => Self::Show,
            "add" => Self::Add,
            "remove" => Self::Remove,
            "login" => Self::Login,
            "logout" => Self::Logout,
            "approve" => Self::Approve,
            "reject" => Self::Reject,
            _ => return None,
        })
    }

    /// `@tagName`.
    pub fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Show => "show",
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

/// A line read as a verb (`Command`, `verbs.zig:39-50`), for the shell
/// surface: no `--json`, and a login's `--no-browser` read and not kept,
/// since signing in is gated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help(Option<Verb>),
    List,
    Show(String),
    Add {
        name: String,
        file: File,
        entry: Entry,
    },
    /// `add [--project] [--header K=V]... slack`: Slack's server, to sign in
    /// to. Gated.
    AddSlack,
    Remove {
        name: String,
        file: Option<File>,
    },
    Login(String),
    Logout(String),
    /// `None` is `--all`.
    Approve(Option<String>),
    Reject(String),
}

/// What a line is (`Parsed`, `verbs.zig:52-58`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Command(Command),
    /// A usage mistake: the message, then the verb's usage.
    Usage {
        verb: Option<Verb>,
        message: String,
    },
    /// A verb v1 had, and where it went.
    Moved(String),
}

/// Reads the words after `/mcp` (`parse`, `verbs.zig:62-174`), in fx's order:
/// help, the moved verbs and an unknown word; the flags before `--`; then
/// each verb's own checks, a server name's before anything about its URL or
/// command.
pub fn parse(args: &[String]) -> Parsed {
    let p = PROG;
    let usage = |verb: Option<Verb>, message: String| Parsed::Usage { verb, message };
    let Some(first) = args.first() else {
        return Parsed::Command(Command::Help(None));
    };
    if is_help(first) || first == "help" {
        return Parsed::Command(Command::Help(None));
    }
    let Some(verb) = Verb::parse(first) else {
        let moved = |message: String| Parsed::Moved(message);
        return match first.as_str() {
            "auth" => moved(format!("{p} auth is now {p} login NAME.")),
            "trust" => moved(format!(
                "{p} trust is now {p} approve NAME and {p} reject NAME."
            )),
            "path" => moved(format!(
                "{p} path is gone: {p} list shows each server's file."
            )),
            "reload" => moved(format!(
                "{p} reload is gone: xfx reloads after add, remove, approve, and reject."
            )),
            "resource" | "prompt" => moved(format!(
                "{p} {first} is gone: xfx doesn't support MCP resources and prompts yet."
            )),
            other => usage(None, format!("'{}' isn't a {p} command.", typed(other))),
        };
    };

    // Flags come before `--`; everything after it is a stdio server's command.
    let rest = &args[1..];
    let dash = rest
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(rest.len());
    let mut names: Vec<&str> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut pair_flag: Option<&str> = None;
    let (mut project, mut profile, mut all) = (false, false, false);
    let mut i = 0;
    while i < dash {
        let arg = rest[i].as_str();
        if is_help(arg) {
            return Parsed::Command(Command::Help(Some(verb)));
        }
        if !arg.starts_with('-') {
            names.push(arg);
            i += 1;
            continue;
        }
        let allowed = match verb {
            // `--json` is the CLI surface's only.
            Verb::List | Verb::Show => false,
            Verb::Add => matches!(arg, "--project" | "--env" | "--header"),
            Verb::Remove => matches!(arg, "--project" | "--profile"),
            Verb::Login => arg == "--no-browser",
            Verb::Approve => arg == "--all",
            Verb::Logout | Verb::Reject => false,
        };
        if verb == Verb::List && arg == "--connect" {
            return Parsed::Moved(format!("{p} list --connect is now {p} show NAME."));
        }
        if !allowed {
            return usage(
                Some(verb),
                format!("{p} {} has no {} flag.", verb.name(), typed(arg)),
            );
        }
        if arg == "--env" || arg == "--header" {
            if pair_flag.is_some_and(|flag| flag != arg) {
                return usage(
                    Some(verb),
                    "--env is for stdio servers and --header for HTTP ones.".into(),
                );
            }
            pair_flag = Some(arg);
            i += 1;
            let pair = if i < dash { rest[i].as_str() } else { "" };
            // A key is what comes before the first `=`; none, or an empty
            // one, is no pair.
            match pair.find('=') {
                Some(eq) if eq > 0 => {
                    pairs.push((pair[..eq].to_string(), pair[eq + 1..].to_string()))
                }
                _ => return usage(Some(verb), format!("{arg} needs KEY=VALUE.")),
            }
            i += 1;
            continue;
        }
        match arg {
            "--project" => project = true,
            "--profile" => profile = true,
            "--all" => all = true,
            // `--no-browser`: a sign-in is gated, so how it would wait is moot.
            _ => {}
        }
        i += 1;
    }
    if dash < rest.len() && verb != Verb::Add {
        return usage(
            Some(verb),
            format!("{p} {} takes no command after --.", verb.name()),
        );
    }

    match verb {
        Verb::List => {
            if !names.is_empty() {
                return usage(Some(verb), format!("{p} list takes no server name."));
            }
            return Parsed::Command(Command::List);
        }
        Verb::Add => {
            let Some(name) = names.first().copied() else {
                return usage(Some(verb), format!("{p} add needs a NAME."));
            };
            if !config::is_valid_server_name(name) {
                return usage(
                    Some(verb),
                    format!(
                        "'{}' can't be a server name: use letters, digits, '-' and '_'.",
                        typed(name)
                    ),
                );
            }
            let file = if project {
                File::Project
            } else {
                File::Profile
            };
            if dash < rest.len() {
                if names.len() > 1 {
                    return usage(
                        Some(verb),
                        format!("{p} add takes a URL or a command after --, not both."),
                    );
                }
                let Some((program, args)) = rest[dash + 1..].split_first() else {
                    return usage(Some(verb), format!("{p} add needs a command after --."));
                };
                if pair_flag == Some("--header") {
                    return usage(
                        Some(verb),
                        "--header is for HTTP servers; use --env for a command.".into(),
                    );
                }
                return Parsed::Command(Command::Add {
                    name: name.to_string(),
                    file,
                    entry: Entry::Stdio {
                        command: program.clone(),
                        args: args.to_vec(),
                        env: pairs,
                    },
                });
            }
            // fx knows Slack's address, as `add slack` did before.
            let slack = names.len() == 1 && name == "slack";
            let url = if slack {
                SLACK_URL
            } else {
                if names.len() != 2 {
                    return usage(
                        Some(verb),
                        format!("{p} add needs a URL, or -- and a command."),
                    );
                }
                names[1]
            };
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return usage(
                    Some(verb),
                    format!(
                        "'{}' isn't a URL. A stdio server's command goes after --.",
                        typed(url)
                    ),
                );
            }
            if pair_flag == Some("--env") {
                return usage(
                    Some(verb),
                    "--env is for stdio servers; use --header for HTTP.".into(),
                );
            }
            if slack {
                return Parsed::Command(Command::AddSlack);
            }
            return Parsed::Command(Command::Add {
                name: name.to_string(),
                file,
                entry: Entry::Http {
                    url: url.to_string(),
                    headers: pairs,
                },
            });
        }
        Verb::Approve => {
            if all && !names.is_empty() {
                return usage(
                    Some(verb),
                    format!("{p} approve takes a NAME or --all, not both."),
                );
            }
            if all {
                return Parsed::Command(Command::Approve(None));
            }
        }
        _ => {}
    }
    let [name] = names.as_slice() else {
        return usage(
            Some(verb),
            format!("{p} {} needs one server NAME.", verb.name()),
        );
    };
    let name = name.to_string();
    Parsed::Command(match verb {
        Verb::Show => Command::Show(name),
        Verb::Remove => {
            if project && profile {
                return usage(Some(verb), "Use --project or --profile, not both.".into());
            }
            let file = if project {
                Some(File::Project)
            } else if profile {
                Some(File::Profile)
            } else {
                None
            };
            Command::Remove { name, file }
        }
        Verb::Login => Command::Login(name),
        Verb::Logout => Command::Logout(name),
        Verb::Approve => Command::Approve(Some(name)),
        Verb::Reject => Command::Reject(name),
        Verb::List | Verb::Add => unreachable!("answered above"),
    })
}

fn is_help(arg: &str) -> bool {
    arg == "-h" || arg == "--help"
}

/// The overview's rows (`overview`, `verbs.zig:188-199`).
const OVERVIEW: [(&str, &str); 10] = [
    ("list", "configured servers; never connects"),
    ("show NAME", "connect to NAME: status, version, tools"),
    ("add NAME URL", "add an HTTP server"),
    ("add NAME -- CMD [ARGS...]", "add a stdio server"),
    ("add slack", "add Slack's server"),
    ("remove NAME", "remove a server and its login"),
    ("login NAME", "sign in to an HTTP server"),
    ("logout NAME", "sign out and delete the login"),
    (
        "approve NAME | --all",
        "approve project servers from .mcp.json",
    ),
    ("reject NAME", "reject a project server"),
];

/// One verb's usage, or every verb's (`writeUsage`, `verbs.zig:202-230`),
/// each line ending in a line break.
pub fn usage(verb: Option<Verb>) -> String {
    let p = PROG;
    let Some(verb) = verb else {
        let mut out = format!("usage: {p} COMMAND\n\n");
        for (left, right) in OVERVIEW {
            out.push_str(&format!("  {left:<28}{right}\n"));
        }
        out.push_str(&format!("\nRun '{p} COMMAND -h' for a command's flags.\n"));
        return out;
    };
    match verb {
        Verb::List => format!("usage: {p} list\n"),
        Verb::Show => format!("usage: {p} show NAME\n"),
        Verb::Add => format!(
            "usage: {p} add [--project] [--header K=V]... NAME URL\n       \
             {p} add [--project] [--env K=V]... NAME -- COMMAND [ARGS...]\n       \
             {p} add [--project] slack\n  \
             --project  write ./.mcp.json instead of {}\n",
            File::Profile.label()
        ),
        Verb::Remove => format!("usage: {p} remove [--project | --profile] NAME\n"),
        Verb::Login => format!(
            "usage: {p} login [--no-browser] NAME\n  \
             --no-browser  print the address instead of opening it\n"
        ),
        Verb::Logout => format!("usage: {p} logout NAME\n"),
        Verb::Approve => format!("usage: {p} approve NAME | --all\n"),
        Verb::Reject => format!("usage: {p} reject NAME\n"),
    }
}

/// Splits a `/mcp` line into its words as fx's shell-command lexer does
/// (`tokenize_argv`, `command_lex.zig:140-226@481104fd`): blanks (space, tab,
/// CR, LF) between words; `'…'` taken as written; `"…"` with a backslash
/// quoting only `$`, `` ` ``, `"`, `\` and a line break; a backslash outside
/// quotes quoting the next character; and every unquoted redirection or
/// control operator (`<`, `>>`, `&>`, `|`, `;`, `&&`, `(`, …) a word of its
/// own. `None` for a quote never closed, a backslash at the end, or a NUL.
pub fn tokenize(line: &str) -> Option<Vec<String>> {
    if line.contains('\0') {
        return None;
    }
    let chars: Vec<char> = line.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut value = String::new();
    let mut active = false;
    let (mut single, mut double) = (false, false);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if !single && !double {
            if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                if active {
                    tokens.push(std::mem::take(&mut value));
                }
                active = false;
                i += 1;
                continue;
            }
            if let Some(len) = redirection_len(&chars, i).or_else(|| control_len(&chars, i)) {
                if active {
                    tokens.push(std::mem::take(&mut value));
                }
                tokens.push(chars[i..i + len].iter().collect());
                active = false;
                i += len;
                continue;
            }
        }
        active = true;
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '\\' if !single => {
                let next = *chars.get(i + 1)?;
                // Inside double quotes a backslash quotes only these; before
                // any other character it stays.
                if double && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                    value.push('\\');
                }
                value.push(next);
                i += 2;
                continue;
            }
            _ => value.push(ch),
        }
        i += 1;
    }
    if single || double {
        return None;
    }
    if active {
        tokens.push(value);
    }
    Some(tokens)
}

/// `redirection_operator_len` (`command_lex.zig:268-301`).
fn redirection_len(chars: &[char], start: usize) -> Option<usize> {
    let at = |k: usize| chars.get(start + k).copied();
    match chars[start] {
        '&' if at(1) == Some('>') => Some(if at(2) == Some('>') { 3 } else { 2 }),
        '<' => Some(if at(1) == Some('<') && at(2) == Some('<') {
            3
        } else if matches!(at(1), Some('<' | '&' | '>')) {
            2
        } else {
            1
        }),
        '>' => Some(if matches!(at(1), Some('>' | '|' | '&')) {
            2
        } else {
            1
        }),
        _ => None,
    }
}

/// `control_operator_len` (`command_lex.zig:303-310`).
fn control_len(chars: &[char], start: usize) -> Option<usize> {
    let doubled = |c: char| chars.get(start + 1) == Some(&c);
    match chars[start] {
        ';' | '(' | ')' | '{' | '}' => Some(1),
        '|' => Some(if doubled('|') { 2 } else { 1 }),
        '&' => Some(if doubled('&') { 2 } else { 1 }),
        _ => None,
    }
}

/// A name, a URL or a line said back, made safe for a terminal row as fx
/// makes it (`encodeTerminalSafeInline(.., 240)`, `text_utils.zig:397-417
/// @481104fd`): every run of ASCII blanks one space and none at either end,
/// then each character as [`crate::mcp::terminal_safe`] spells it -- a
/// control, an invisible or a bidirectional code point written out -- and
/// cut at a whole token to 240 bytes, ending `...`.
pub fn safe(raw: &str) -> String {
    safe_within(raw, SAFE_MAX_BYTES)
}

/// [`safe`] with its cut at `max` bytes: fx's own menu rows spell what they
/// show with `encodeTerminalSafeInline(.., 4096)` and cut it to the row
/// (`appendTerminalSafeSingleLine`, `src/ui/footer/row_text.zig:127-134
/// @481104fd`), which `/mcp`'s menu does with this.
pub(crate) fn safe_within(raw: &str, max: usize) -> String {
    const MARKER: &str = "...";
    let max = max.max(MARKER.len());
    let flat = raw
        .split([' ', '\t', '\n', '\r', '\x0b', '\x0c'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let content_limit = max - MARKER.len();
    let mut out = String::new();
    let mut boundary = 0;
    let mut buf = [0u8; 4];
    for character in flat.chars() {
        let token = super::terminal_safe(character.encode_utf8(&mut buf));
        if out.len() + token.len() > max {
            out.truncate(boundary);
            out.push_str(MARKER);
            return out;
        }
        out.push_str(&token);
        if out.len() <= content_limit {
            boundary = out.len();
        }
    }
    out
}

/// A word someone typed, said back as far as its first `=` and no further --
/// what follows one is a value, and a value can be a secret (`--env=TOKEN=…`,
/// `--header=Authorization=…`, a name typed where a pair was meant) -- and
/// spelled by [`safe`]. A name `/mcp add` takes has no `=`, so one is said
/// whole; a profile written by hand can hold one with it, and is cut too.
pub(crate) fn typed(word: &str) -> String {
    safe(word.find('=').map_or(word, |eq| &word[..=eq]))
}

/// How many cells a spelled text takes (`terminalSafeVisibleWidth`).
fn width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// A stdio server's command and arguments as one line, quoting what a shell
/// would split (`commandLine`, `src/core/mcp_host/config.zig:67-83@481104fd`).
pub fn command_line(command: &str, args: &[String]) -> String {
    let mut out = String::new();
    for (k, word) in std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .enumerate()
    {
        if k > 0 {
            out.push(' ');
        }
        let plain =
            !word.is_empty() && !word.contains([' ', '\t', '\n', '\'', '"', '$', '`', '\\']);
        if plain {
            out.push_str(word);
            continue;
        }
        out.push('\'');
        out.push_str(&word.replace('\'', "'\\''"));
        out.push('\'');
    }
    out
}

/// A server's status in words, as `list`, `show` and the completion's
/// descriptions say it (`statusText`, `verbs.zig:369-388@481104fd`): what
/// holds it back first -- switched off, waiting for approval, rejected, a
/// transport xfx does not connect -- then a login it needs, then how its
/// connection is. `enabled` is its configuration's.
pub fn status_text(view: &ServerView, enabled: bool) -> String {
    if !enabled {
        return "disabled".into();
    }
    match view.admission {
        Some(Admission::Pending) => return "waiting for approval".into(),
        Some(Admission::Rejected) => return "rejected".into(),
        _ => {}
    }
    if view.transport == Transport::Sse {
        return "not supported".into();
    }
    if view.needs_auth {
        return "needs login".into();
    }
    match view.connection {
        Connection::Ready => {
            let tools = view.tool_count.unwrap_or(view.tools_held);
            format!("ready, {tools} tool{}", if tools == 1 { "" } else { "s" })
        }
        Connection::Connecting => "connecting".into(),
        Connection::Retrying => "retrying".into(),
        Connection::Failed => "failed".into(),
        Connection::Disabled => "disabled".into(),
        // Not started, and no login to be signed in with.
        Connection::Disconnected => "not started".into(),
    }
}

/// What a server is held back by, if anything: fx's `held` (`heldFor`,
/// `src/core/mcp_host/config.zig:189-198@481104fd`), which a sign-in refuses.
fn held(view: &ServerView, enabled: bool) -> bool {
    !enabled
        || matches!(
            view.admission,
            Some(Admission::Pending | Admission::Rejected)
        )
        || view.transport == Transport::Sse
}

/// The completions `/mcp ` offers, each the whole line and what the menu
/// says beside it (`verb_completions` and `buildCompletion`,
/// `app_mcp_runtime.zig:459-515@481104fd`): the eight verbs, then for each
/// server whose name a command can carry, `show` and `remove`, `login` and
/// `logout` for a remote one, `approve` and `reject` for a project one, its
/// status the description.
pub fn completions(runtime: &McpRuntime) -> Vec<(String, String)> {
    const VERBS: [(&str, &str); 8] = [
        ("/mcp list", "configured servers"),
        ("/mcp show", "connect to a server and list its tools"),
        ("/mcp add", "add a server"),
        ("/mcp remove", "remove a server and its login"),
        ("/mcp login", "sign in to an HTTP server"),
        ("/mcp logout", "sign out of an HTTP server"),
        ("/mcp approve", "approve a project server"),
        ("/mcp reject", "reject a project server"),
    ];
    let mut out: Vec<(String, String)> = VERBS
        .iter()
        .map(|(full, description)| (full.to_string(), description.to_string()))
        .collect();
    for (view, enabled) in servers(runtime) {
        if !config::is_valid_server_name(&view.name) {
            continue;
        }
        let description = status_text(&view, enabled);
        let remote = view.transport != Transport::Stdio;
        let project = view.source == Source::Workspace;
        for (verb, applies) in [
            ("show", true),
            ("remove", true),
            ("login", remote),
            ("logout", remote),
            ("approve", project),
            ("reject", project),
        ] {
            if applies {
                out.push((format!("/mcp {verb} {}", view.name), description.clone()));
            }
        }
    }
    out
}

/// What a verb printed: its standard output and its standard error, and
/// whether it changed the config, so the servers reload once both are said.
#[derive(Debug, Default)]
struct Said {
    out: String,
    err: String,
    changed: bool,
}

/// Runs `/mcp <rest>` and says what it did: its output one neutral notice,
/// its errors one warning, in that order, and then -- after a change to
/// either file -- the reload that puts the change in force, in place and
/// silent unless the files can't be read again.
pub fn handle(runtime: &Arc<McpRuntime>, rest: &str) {
    let Some(args) = tokenize(rest) else {
        return runtime.notice(true, UNMATCHED);
    };
    report(runtime, run(runtime, &args), true);
}

/// `/mcp`'s menu's Remove: `name` from `file`, the ones its question named,
/// as the typed `remove NAME --profile|--project` removes them -- never
/// parsed again, so a name that reads as a flag is a name -- and only its
/// warnings said: the menu shows its result in the list
/// (`.warnings_only`, `VerbJob.run`, `app_mcp_runtime.zig:545-560`,
/// `:609-614@481104fd`).
pub fn remove_chosen(runtime: &Arc<McpRuntime>, name: &str, file: File) {
    let mut said = Said::default();
    remove(runtime, name, Some(file), &mut said);
    report(runtime, said, false);
}

/// Says what a verb did -- its output (unless only its warnings are
/// wanted) one neutral notice, its errors one warning, in that order -- and
/// then, after a change to either file, the reload that puts the change in
/// force, in place and silent unless the files can't be read again.
fn report(runtime: &Arc<McpRuntime>, said: Said, output: bool) {
    for (text, warning) in [(&said.out, false), (&said.err, true)] {
        let text = text.trim_end_matches('\n');
        if !text.is_empty() && (warning || output) {
            runtime.notice(warning, text);
        }
    }
    if said.changed {
        runtime.reload(Origin::Transcript);
    }
}

/// `runParsed` (`verbs.zig:273-297`).
fn run(runtime: &Arc<McpRuntime>, args: &[String]) -> Said {
    let mut said = Said::default();
    let command = match parse(args) {
        Parsed::Command(command) => command,
        Parsed::Usage { verb, message } => {
            said.err = format!("{message}\n{}", usage(verb));
            return said;
        }
        Parsed::Moved(message) => {
            said.err = format!("{message}\n");
            return said;
        }
    };
    match command {
        Command::Help(verb) => said.out = usage(verb),
        Command::List => list(runtime, &mut said),
        Command::Show(name) => show(runtime, &name, &mut said),
        Command::Add { name, file, entry } => add(runtime, &name, file, &entry, &mut said),
        Command::AddSlack => said.err = format!("{AUTH_UNAVAILABLE}\n"),
        Command::Remove { name, file } => remove(runtime, &name, file, &mut said),
        Command::Login(name) => login(runtime, &name, &mut said),
        Command::Logout(name) => logout(runtime, &name, &mut said),
        Command::Approve(name) => trust(runtime, name.map_or(Trust::All, Trust::Approve)),
        Command::Reject(name) => trust(runtime, Trust::Reject(name)),
    }
    said
}

/// The two files a verb edits, or `None` with no home to keep a profile in
/// (`paths`, `verbs.zig:331-336`).
fn paths(runtime: &McpRuntime) -> Option<(PathBuf, PathBuf)> {
    let profile = runtime.paths().profile.clone()?;
    Some((profile, config::project_path(&runtime.paths().workspace)))
}

/// fx's `run` catch for `HomeNotSet` (`verbs.zig:262-266`, `describe`, `:313-327`).
fn no_home(said: &mut Said) {
    said.err = format!("{PROG}: HOME isn't set\n");
}

/// What loading the config skipped, so a server never vanishes without a
/// reason (`writeNotes`, `verbs.zig:462-467`): each note a line of the
/// warning. A config with no server still carries its notes.
fn notes(runtime: &McpRuntime, said: &mut Said) {
    for note in runtime.issues() {
        said.err.push_str(&format!("note: {note}\n"));
    }
}

/// Each configured server and the `enabled` its configuration says.
fn servers(runtime: &McpRuntime) -> Vec<(ServerView, bool)> {
    runtime
        .views()
        .into_iter()
        .map(|view| {
            let enabled = runtime
                .server(&view.name)
                .is_none_or(|server| server.config.enabled);
            (view, enabled)
        })
        .collect()
}

/// The file a server came from (`sourceFile`, `verbs.zig:345-347`).
fn file_of(source: Source) -> File {
    match source {
        Source::Profile => File::Profile,
        Source::Workspace => File::Project,
    }
}

/// `list` (`verbs.zig:349-366`, `writeList`, `:408-426`): the table, every
/// column but the last as wide as its widest cell and two more.
fn list(runtime: &McpRuntime, said: &mut Said) {
    notes(runtime, said);
    let servers = servers(runtime);
    if servers.is_empty() {
        said.out = format!("No MCP servers configured. Add one with: {PROG} add NAME URL\n");
        return;
    }
    let mut rows: Vec<[String; 4]> = vec![[
        "NAME".into(),
        "SOURCE".into(),
        "TRANSPORT".into(),
        "STATUS".into(),
    ]];
    for (view, enabled) in &servers {
        rows.push([
            typed(&view.name),
            file_of(view.source).label().to_string(),
            view.transport.tag().to_string(),
            status_text(view, *enabled),
        ]);
    }
    let mut widths = [0usize; 3];
    for row in &rows {
        for (k, cell) in row.iter().take(3).enumerate() {
            widths[k] = widths[k].max(width(cell));
        }
    }
    for row in &rows {
        for (k, cell) in row.iter().take(3).enumerate() {
            said.out.push_str(cell);
            said.out.push_str(&" ".repeat(widths[k] - width(cell) + 2));
        }
        said.out.push_str(&row[3]);
        said.out.push('\n');
    }
}

/// `missing` (`verbs.zig:557-560`).
fn missing(name: &str, said: &mut Said) {
    said.err.push_str(&format!(
        "No MCP server named {}. {PROG} list shows them.\n",
        typed(name)
    ));
}

/// Waits while `name` is coming up -- a try under way -- as fx's `show`
/// waits for the server's list (`host.runtime.tools`, `verbs.zig:485-491`),
/// for at most its own start's timeout and [`SHOW_WAIT_MAX`]. A server the
/// session stopped meanwhile is gone from the runtime, which ends the wait.
fn settle(runtime: &McpRuntime, name: &str) {
    let Some(server) = runtime.server(name) else {
        return;
    };
    let deadline = Instant::now() + server.config.startup_timeout().min(SHOW_WAIT_MAX);
    drop(server);
    while Instant::now() < deadline
        && runtime
            .views()
            .iter()
            .any(|view| view.name == name && view.connection == Connection::Connecting)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `show NAME` (`verbs.zig:483-556`): connected first, then its facts, one
/// a row, and its tools, each with its description's first line.
fn show(runtime: &McpRuntime, name: &str, said: &mut Said) {
    notes(runtime, said);
    if runtime.server(name).is_none() {
        return missing(name, said);
    }
    settle(runtime, name);
    // A reload may have taken it out since: the runtime takes changes in place.
    let Some(server) = runtime.server(name) else {
        return missing(name, said);
    };
    let Some(view) = runtime.views().into_iter().find(|view| view.name == name) else {
        return missing(name, said);
    };
    let config = &server.config;
    let out = &mut said.out;
    out.push_str(&format!("{}\n", typed(name)));
    out.push_str(&format!(
        "  status     {}\n",
        status_text(&view, config.enabled)
    ));
    if let Some(version) = &view.protocol_version {
        out.push_str(&format!("  version    {}\n", safe(version)));
    }
    out.push_str(&format!("  transport  {}\n", config.transport.tag()));
    match config.transport {
        Transport::Stdio => {
            let (command, args) = config.shown_command();
            out.push_str(&format!(
                "  command    {}\n",
                safe(&command_line(command, args))
            ))
        }
        Transport::Http | Transport::Sse => {
            out.push_str(&format!("  url        {}\n", safe(config.shown_url())))
        }
    }
    out.push_str(&format!(
        "  source     {}\n",
        file_of(config.source).label()
    ));
    if let Some(failure) = &view.failure {
        out.push_str(&format!("  error      {}\n", safe(failure)));
    }
    if view.needs_auth {
        out.push_str(&format!("  next       {PROG} login {}\n", typed(name)));
    }
    if config.enabled && view.admission == Some(Admission::Pending) {
        out.push_str(&format!("  next       {PROG} approve {}\n", typed(name)));
    }
    let tools = server.tools();
    if !tools.is_empty() {
        out.push_str(&format!("  tools      {}\n", tools.len()));
        let names: Vec<String> = tools.iter().map(|tool| safe(&tool.name)).collect();
        let widest = names.iter().map(|name| width(name)).max().unwrap_or(0);
        for (tool, shown) in tools.iter().zip(&names) {
            out.push_str(&format!("    {shown}"));
            let description = first_line(&tool.description);
            if description.is_empty() {
                out.push('\n');
                continue;
            }
            out.push_str(&" ".repeat(widest - width(shown) + 2));
            out.push_str(&safe(description));
            out.push('\n');
        }
    }
}

/// A tool's description, first line only, blanks trimmed (`toolDescription`,
/// `verbs.zig:568-575`).
pub(crate) fn first_line(description: &str) -> &str {
    description
        .split('\n')
        .next()
        .unwrap_or("")
        .trim_matches([' ', '\t', '\r'])
}

/// Whether the file at `path` defines `name` (`defines`, `verbs.zig:593-597`).
fn defines(file: File, path: &Path, name: &str) -> Result<bool, config::EditError> {
    Ok(config::read_config(path)?.is_some_and(|text| config::defines(file, &text, name)))
}

/// `add` (`verbs.zig:577-592`).
fn add(runtime: &McpRuntime, name: &str, file: File, entry: &Entry, said: &mut Said) {
    let Some((profile, project)) = paths(runtime) else {
        return no_home(said);
    };
    let path = match file {
        File::Profile => &profile,
        File::Project => &project,
    };
    // xfx's reader skips a `.mcp.json` entry with an `Authorization` header --
    // a repository may not plant a credential -- so one is never written
    // there: it would be a token in a shared file that xfx then ignores.
    // Matched as the reader matches it (`parse_server_entry`, ignoring case).
    if file == File::Project {
        if let Entry::Http { headers, .. } = entry {
            if headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("authorization"))
            {
                said.err = format!(
                    "Couldn't add {name} to {project_file}: xfx won't send an Authorization header from {project_file}; add the server without --project to keep it in {}.\n",
                    File::Profile.label(),
                    project_file = File::Project.label(),
                );
                return;
            }
        }
    }
    // And nothing the reader would skip: the reader's own rules over what
    // would be written -- a header name with a blank in it (` Authorization`
    // too), a reserved one, a control in a value, a URL that isn't one it
    // connects to -- refused before the file is touched.
    if let Some(why) = config::unreadable(file, name, entry) {
        said.err = format!("Couldn't add {name} to {}: {why}.\n", file.label());
        return;
    }
    let change = Change::Add {
        name: name.to_string(),
        entry: entry.clone(),
    };
    if let Err(err) = config::update(&profile, file, path, &change) {
        said.err = format!("Couldn't add {name} to {}: {}.\n", file.label(), err.why());
        return;
    }
    said.changed = true;
    said.out = format!("Added {name} to {}.\n", file.label());
    let (other, other_path) = match file {
        File::Profile => (File::Project, &project),
        File::Project => (File::Profile, &profile),
    };
    match defines(other, other_path, name) {
        Ok(true) => said.out.push_str(&format!(
            "{name} is also in {}; the one in {} is used.\n",
            other.label(),
            File::Profile.label()
        )),
        Ok(false) => {}
        Err(err) => said.err = format!("{PROG}: {}\n", err.why()),
    }
}

/// `remove` (`verbs.zig:599-629`).
fn remove(runtime: &McpRuntime, name: &str, only: Option<File>, said: &mut Said) {
    let Some((profile, project)) = paths(runtime) else {
        return no_home(said);
    };
    let shown = typed(name);
    let look = |file: File, path: &Path| -> Result<bool, config::EditError> {
        if only.is_some_and(|only| only != file) {
            return Ok(false);
        }
        defines(file, path, name)
    };
    let (in_profile, in_project) =
        match (look(File::Profile, &profile), look(File::Project, &project)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(err), _) | (_, Err(err)) => {
                said.err = format!("{PROG}: {}\n", err.why());
                return;
            }
        };
    if in_profile && in_project {
        said.err = format!(
            "{shown} is in both {} and {}. Add --profile or --project.\n",
            File::Profile.label(),
            File::Project.label()
        );
        return;
    }
    let not_found = |said: &mut Said| {
        let both = format!("{} or {}", File::Profile.label(), File::Project.label());
        let place = only.map_or(both, |file| file.label().to_string());
        said.err = format!("No MCP server named {shown} in {place}.\n");
    };
    if !in_profile && !in_project {
        return not_found(said);
    }
    let (file, path) = if in_profile {
        (File::Profile, &profile)
    } else {
        (File::Project, &project)
    };
    match config::update(&profile, file, path, &Change::Remove(name.to_string())) {
        Err(err) => {
            said.err = format!(
                "Couldn't remove {shown} from {}: {}.\n",
                file.label(),
                err.why()
            );
        }
        // Gone between the look and the edit, which takes the lock: nothing
        // was written, so nothing was removed. Untested: the two read the
        // same file through the same parser, so only another writer between
        // them reaches this, and nothing here can hold one there.
        Ok(config::Updated::Missing) => not_found(said),
        Ok(config::Updated::Added | config::Updated::Removed { .. }) => {
            said.changed = true;
            // xfx keeps no login, so there is none to delete with it.
            said.out = format!("Removed {shown} from {}.\n", file.label());
        }
    }
}

/// The server `name` and whether its configuration enables it, or `None`.
fn find(runtime: &McpRuntime, name: &str) -> Option<(ServerView, bool)> {
    servers(runtime)
        .into_iter()
        .find(|(view, _)| view.name == name)
}

/// `login NAME` (`verbs.zig:765-806`): every refusal fx makes before it
/// would sign in, and then the gate.
fn login(runtime: &McpRuntime, name: &str, said: &mut Said) {
    let Some((view, enabled)) = find(runtime, name) else {
        return missing(name, said);
    };
    let shown = typed(name);
    if view.transport == Transport::Stdio {
        said.err = format!("{shown} runs a command, so it has no login.\n");
        return;
    }
    if held(&view, enabled) {
        said.err = format!("{shown} is {}", status_text(&view, enabled));
        if enabled && view.admission == Some(Admission::Pending) {
            said.err.push_str(&format!(": {PROG} approve {shown}"));
        }
        said.err.push_str(".\n");
        return;
    }
    said.err = format!("{AUTH_UNAVAILABLE}\n");
}

/// `logout NAME` (`verbs.zig:689-716`). xfx saves no login and holds none,
/// so a remote server is one that wasn't signed in.
fn logout(runtime: &McpRuntime, name: &str, said: &mut Said) {
    let Some((view, _)) = find(runtime, name) else {
        return missing(name, said);
    };
    let shown = typed(name);
    if view.transport == Transport::Stdio {
        said.err = format!("{shown} runs a command, so it has no login.\n");
        return;
    }
    said.out = format!("{shown} wasn't signed in.\n");
}

/// A trust choice typed as a verb.
enum Trust {
    Approve(String),
    All,
    Reject(String),
}

/// `approve` and `reject` (`trust`, `verbs.zig:631-680`): the project
/// servers the choice is about -- the one named, or for `--all` every one
/// not rejected -- each said with what it runs or reaches, so the person
/// sees what they trust. The choice is kept under the settings keys the
/// menu's choices keep it under, the entry the runtime last read approved,
/// and the servers reload once it is.
fn trust(runtime: &Arc<McpRuntime>, choice: Trust) {
    let project: Vec<Arc<super::runtime::Server>> = runtime
        .views()
        .into_iter()
        .filter(|view| view.source == Source::Workspace)
        .filter(|view| match &choice {
            Trust::Approve(name) | Trust::Reject(name) => &view.name == name,
            Trust::All => view.admission != Some(Admission::Rejected),
        })
        .filter_map(|view| runtime.server(&view.name))
        .collect();
    if project.is_empty() {
        if let Trust::Approve(name) | Trust::Reject(name) = &choice {
            match unparsed(runtime, &choice, name) {
                Unparsed::Choice(action, body) => {
                    return runtime.run(super::runtime::Job::Trust { action, body })
                }
                Unparsed::Changed => {
                    return runtime.notice(
                        true,
                        format!(
                            "{name} changed in {} since xfx read it; nothing was approved.",
                            File::Project.label()
                        ),
                    )
                }
                Unparsed::None => {}
            }
        }
        let sentence = match &choice {
            Trust::Approve(name) | Trust::Reject(name) => format!(
                "No project server named {} in {}.",
                typed(name),
                File::Project.label()
            ),
            Trust::All => "No project servers are waiting for approval.".into(),
        };
        return runtime.notice(true, sentence);
    }
    let body: Vec<String> = project
        .iter()
        .map(|server| {
            let config = &server.config;
            let shown = typed(&config.name);
            if matches!(choice, Trust::Reject(_)) {
                return format!("Rejected {shown}. xfx won't start it in this project.");
            }
            let (how, target) = match config.transport {
                Transport::Stdio => {
                    let (command, args) = config.shown_command();
                    ("runs", command_line(command, args))
                }
                Transport::Http | Transport::Sse => ("connects to", config.shown_url().to_string()),
            };
            format!("Approved {shown}, which {how}: {}", safe(&target))
        })
        .collect();
    let action = match choice {
        Trust::Approve(name) => TrustAction::Approve(name),
        Trust::All => TrustAction::ApproveAll,
        Trust::Reject(name) => TrustAction::Reject(name),
    };
    runtime.run(super::runtime::Job::Trust {
        action,
        body: body.join("\n"),
    });
}

/// What a typed choice about a `.mcp.json` entry with no server comes to.
enum Unparsed {
    /// Not an entry the runtime read, or a profile server's name, whose wins:
    /// fx's `No project server named …`.
    None,
    /// The entry the runtime read is not the one in the file now: nothing is
    /// approved, since what would run is not what xfx read.
    Changed,
    /// The choice, and the sentence that announces it.
    Choice(TrustAction, String),
}

/// A choice about a `.mcp.json` entry the runtime holds no server for,
/// because it parses only once it is expanded (`"type": "${T:-stdio}"`,
/// `"url": "${URL}"`) -- which only an approved entry is.
///
/// **Only an entry the runtime read**, and an approval only while the file
/// still holds that entry: its digest now must be the one the last read took
/// ([`McpRuntime::loaded_digest`]), as the parsed path binds the loaded
/// digest. An entry rewritten since -- by the model's own edit, a pull -- or
/// one the runtime never read is not approved and not started. The sentence
/// shows what the approved entry runs or reaches. fx has no such path -- its
/// `approve` finds only servers that parse (`verbs.zig:631-646`) -- and xfx
/// keeps it from c1db9191's `trust approve <name>`. A rejection needs no
/// binding: it starts nothing.
fn unparsed(runtime: &McpRuntime, choice: &Trust, name: &str) -> Unparsed {
    if runtime.server(name).is_some() {
        return Unparsed::None;
    }
    let Some(loaded) = runtime.loaded_digest(name) else {
        return Unparsed::None;
    };
    if matches!(choice, Trust::Reject(_)) {
        return Unparsed::Choice(
            TrustAction::Reject(name.to_string()),
            format!("Rejected {name}. xfx won't start it in this project."),
        );
    }
    let Some(entry) = config::project_entry(&runtime.paths().workspace, name)
        .filter(|entry| config::entry_digest(entry) == loaded)
    else {
        return Unparsed::Changed;
    };
    let strings = |value: Option<&serde_json::Value>| -> Option<Vec<String>> {
        value?
            .as_array()?
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect()
    };
    let command = match entry.get("command") {
        Some(serde_json::Value::String(command)) => Some(command_line(
            command,
            &strings(entry.get("args")).unwrap_or_default(),
        )),
        Some(serde_json::Value::Array(_)) => strings(entry.get("command"))
            .and_then(|words| words.split_first().map(|(c, a)| command_line(c, a))),
        _ => None,
    };
    let url = entry.get("url").and_then(serde_json::Value::as_str);
    // What it runs or reaches; both when its type, which an expansion
    // decides, could make it either. Never its `env` or `headers`: a value
    // there can be a secret. An entry with neither is no server, expanded or
    // not.
    let what = match (command, url) {
        (Some(command), None) => format!("which runs: {}", safe(&command)),
        (None, Some(url)) => format!("which connects to: {}", safe(url)),
        (Some(command), Some(url)) => format!(
            "which runs: {}, or connects to: {}",
            safe(&command),
            safe(url)
        ),
        (None, None) => return Unparsed::None,
    };
    Unparsed::Choice(
        TrustAction::ApproveShown {
            name: name.to_string(),
            digest: loaded,
        },
        format!("Approved {name}, {what}"),
    )
}

#[cfg(test)]
mod tests;
