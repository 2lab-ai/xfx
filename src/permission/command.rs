//! What a command would *do*, decided from its text before anything runs.
//!
//! [`classify`] answers one question: can this command be turned into an exact
//! argument vector whose effects are read-only and recoverable? If yes, the
//! answer carries the argv, and execution never involves a shell at all --
//! nothing re-parses the string, so nothing in it can be re-interpreted. If no,
//! the answer names the *effect* that stopped it, so `auto` can refuse with a
//! reason a model can act on and a human can judge
//! (`vercel-labs/fx@580a0c5d src/core/shell_command/command_effect.zig:249-262`,
//! `:306-355`).
//!
//! Three rules make the classification safe rather than merely strict:
//!
//! - **Unknown is not safe.** An executable the grammar does not name is
//!   [`DeniedEffect::UnknownCommand`], never "probably fine".
//! - **Dynamic syntax is never direct.** `$`, backticks, globs, `~`, pipes,
//!   redirections, and control operators all leave the direct route, because
//!   their meaning depends on a shell fxr is not running and on a filesystem
//!   fxr has not inspected.
//! - **Quoting is honored.** `grep ';' file` is a search for a semicolon, not
//!   two commands; and the reverse must also hold, so a quoted operand can
//!   never become an operator.
//!
//! Operands are additionally required to be relative and free of `..`. A direct
//! command runs with its working directory inside an authorized root, so a
//! relative operand cannot name anything the read tools would have refused,
//! while `/etc/passwd` and `../../secrets` plainly could.

use std::fmt;

/// The largest command the classifier will look at
/// (`vercel-labs/fx@580a0c5d src/core/shell_command/command_effect.zig:5`).
const MAX_COMMAND_BYTES: usize = 8 * 1024;

/// What a command was found to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEffect {
    /// Recognized, read-only, and reducible to this exact argument vector.
    DirectReadOnly { argv: Vec<String> },
    /// Not admissible without a review, for this reason.
    Denied(DeniedEffect),
}

/// Why a command is not on the direct route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniedEffect {
    /// It changes the filesystem.
    FilesystemWrite,
    /// It reaches the network.
    NetworkAccess,
    /// It starts a shell or controls other processes.
    ProcessOrSystem,
    /// Its meaning depends on shell expansion.
    DynamicShell,
    /// Its shell syntax could not be parsed at all.
    UnsupportedShell,
    /// The executable is not in the admitted grammar.
    UnknownCommand,
    /// The executable is admitted but these arguments are not.
    UnsupportedArgument,
    /// There was no command.
    Empty,
    /// The command is longer than the classifier will consider.
    TooLong,
}

impl DeniedEffect {
    /// The clause that goes after "is not admitted automatically because".
    ///
    /// Each one names a fact about the command rather than a fact about the
    /// policy, so the model learns what to do differently.
    pub fn describe(self) -> &'static str {
        match self {
            Self::FilesystemWrite => "it changes the filesystem",
            Self::NetworkAccess => "it reaches the network",
            Self::ProcessOrSystem => "it starts a shell or controls other processes",
            Self::DynamicShell => "it uses dynamic shell syntax that fxr will not expand",
            Self::UnsupportedShell => "its shell syntax could not be parsed",
            Self::UnknownCommand => "the command is not recognized by the admitted grammar",
            Self::UnsupportedArgument => "its arguments are outside the admitted grammar",
            Self::Empty => "there is no command",
            Self::TooLong => "it is longer than the classifier will consider",
        }
    }
}

impl fmt::Display for DeniedEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// One lexed word and whether any part of it was quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    value: String,
    quoted: bool,
}

/// Decides what `command` would do.
pub fn classify(command: &str) -> CommandEffect {
    let trimmed = command.trim_matches([' ', '\t']);
    if trimmed.is_empty() {
        return CommandEffect::Denied(DeniedEffect::Empty);
    }
    if trimmed.len() > MAX_COMMAND_BYTES {
        return CommandEffect::Denied(DeniedEffect::TooLong);
    }

    let words = match lex(trimmed) {
        Ok(words) => words,
        Err(effect) => return CommandEffect::Denied(effect),
    };
    if words.is_empty() {
        return CommandEffect::Denied(DeniedEffect::Empty);
    }

    // `VAR=value cmd` is an assignment prefix: it is the shell, not the command,
    // that would apply it, and fxr is not running one.
    if !words[0].quoted && words[0].value.contains('=') {
        return CommandEffect::Denied(DeniedEffect::DynamicShell);
    }

    match plan(&words) {
        Ok(()) => CommandEffect::DirectReadOnly {
            argv: words.into_iter().map(|word| word.value).collect(),
        },
        Err(effect) => CommandEffect::Denied(effect),
    }
}

// ---------------------------------------------------------------------------
// lexing
// ---------------------------------------------------------------------------

/// Splits `command` into words, refusing anything a shell would reinterpret.
///
/// This is not a shell parser and does not try to be one. It accepts the subset
/// of POSIX word syntax whose meaning is a pure function of the string --
/// literal words, `'...'`, and `"..."` without expansions -- and rejects
/// everything else by naming the effect it would have had.
fn lex(command: &str) -> Result<Vec<Word>, DeniedEffect> {
    let bytes = command.as_bytes();
    let mut words: Vec<Word> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' => {
                if started {
                    words.push(Word {
                        value: std::mem::take(&mut current),
                        quoted,
                    });
                    started = false;
                    quoted = false;
                }
                index += 1;
            }
            b'\'' => {
                started = true;
                quoted = true;
                index += 1;
                let start = index;
                while index < bytes.len() && bytes[index] != b'\'' {
                    if bytes[index] == b'\n' || bytes[index] == b'\r' || bytes[index] == 0 {
                        return Err(DeniedEffect::UnsupportedShell);
                    }
                    index += 1;
                }
                if index >= bytes.len() {
                    return Err(DeniedEffect::UnsupportedShell);
                }
                current.push_str(&command[start..index]);
                index += 1;
            }
            b'"' => {
                started = true;
                quoted = true;
                index += 1;
                let start = index;
                while index < bytes.len() && bytes[index] != b'"' {
                    match bytes[index] {
                        // A shell would expand these inside double quotes, so
                        // the string fxr sees is not the string that would run.
                        b'$' | b'`' => return Err(DeniedEffect::DynamicShell),
                        b'\\' | b'\n' | b'\r' | 0 => return Err(DeniedEffect::UnsupportedShell),
                        _ => index += 1,
                    }
                }
                if index >= bytes.len() {
                    return Err(DeniedEffect::UnsupportedShell);
                }
                current.push_str(&command[start..index]);
                index += 1;
            }
            b'$' | b'`' | b'*' | b'?' | b'[' | b'~' => return Err(DeniedEffect::DynamicShell),
            b'>' => return Err(DeniedEffect::FilesystemWrite),
            b'&' => {
                // `&>file` is a redirection; a bare `&` and `&&` are control
                // operators. Both leave the single-command grammar.
                return Err(if bytes.get(index + 1) == Some(&b'>') {
                    DeniedEffect::FilesystemWrite
                } else {
                    DeniedEffect::UnsupportedShell
                });
            }
            b'\\' | b'#' | b';' | b'(' | b')' | b'{' | b'}' | b'|' | b'<' | b'\n' | b'\r' | 0 => {
                return Err(DeniedEffect::UnsupportedShell)
            }
            _ => {
                started = true;
                let start = index;
                while index < bytes.len() && is_plain(bytes[index]) {
                    index += 1;
                }
                current.push_str(&command[start..index]);
            }
        }
    }
    if started {
        words.push(Word {
            value: current,
            quoted,
        });
    }
    Ok(words)
}

/// Whether a byte can appear in a word with no shell meaning at all.
fn is_plain(byte: u8) -> bool {
    !matches!(
        byte,
        b' ' | b'\t'
            | b'\''
            | b'"'
            | b'$'
            | b'`'
            | b'*'
            | b'?'
            | b'['
            | b'~'
            | b'>'
            | b'<'
            | b'&'
            | b'|'
            | b';'
            | b'('
            | b')'
            | b'{'
            | b'}'
            | b'\\'
            | b'#'
            | b'\n'
            | b'\r'
            | 0
    )
}

// ---------------------------------------------------------------------------
// the admitted grammar
// ---------------------------------------------------------------------------

/// Executables whose whole purpose is to change the filesystem
/// (`command_effect.zig` `isFilesystemMutation`).
const FILESYSTEM_WRITERS: &[&str] = &[
    "touch", "rm", "mkdir", "rmdir", "mv", "cp", "install", "chmod", "chown", "chgrp", "ln",
    "truncate", "dd", "tee", "shred", "sed", "patch",
];

/// Executables that reach the network (`isNetworkCommand`).
const NETWORK_COMMANDS: &[&str] = &[
    "curl", "wget", "ssh", "scp", "sftp", "rsync", "nc", "netcat", "ftp", "telnet",
];

/// Executables that start shells or control processes (`isProcessOrSystemCommand`).
const PROCESS_COMMANDS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "fish",
    "cmd",
    "powershell",
    "pwsh",
    "env",
    "xargs",
    "eval",
    "exec",
    "sudo",
    "doas",
    "su",
    "kill",
    "pkill",
    "killall",
    "nohup",
    "systemctl",
    "launchctl",
    "open",
];

/// What one executable accepts.
struct Grammar {
    /// Flags accepted on their own.
    flags: &'static [&'static str],
    /// Flags that consume the following word, or carry digits directly.
    valued: &'static [&'static str],
    /// Whether non-flag words are accepted at all.
    operands: bool,
    /// The fewest non-flag words that make the command meaningful.
    min_operands: usize,
}

const NO_ARGS: Grammar = Grammar {
    flags: &[],
    valued: &[],
    operands: false,
    min_operands: 0,
};

/// Checks `words` (the arguments, without the executable) against `grammar`.
fn check(words: &[Word], grammar: &Grammar) -> Result<(), DeniedEffect> {
    let mut operands = 0usize;
    let mut index = 0usize;
    let mut after_separator = false;
    while index < words.len() {
        let word = &words[index];
        let value = word.value.as_str();

        if !after_separator && !word.quoted && value == "--" {
            // Everything past `--` belongs to the program being run, not to the
            // command line fxr is judging.
            after_separator = true;
            index += 1;
            continue;
        }

        if !after_separator && !word.quoted && value.starts_with('-') && value.len() > 1 {
            if grammar.flags.contains(&value) {
                index += 1;
                continue;
            }
            let mut valued: Option<&'static str> = None;
            for flag in grammar.valued {
                if value == *flag || fused_value(value, flag) {
                    valued = Some(flag);
                    break;
                }
            }
            if let Some(flag) = valued {
                if value == flag {
                    // The value is the next word, whatever it is.
                    if index + 1 >= words.len() {
                        return Err(DeniedEffect::UnsupportedArgument);
                    }
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }
            return Err(DeniedEffect::UnsupportedArgument);
        }

        if !grammar.operands && !after_separator {
            return Err(DeniedEffect::UnsupportedArgument);
        }
        if !after_separator {
            operand_is_reachable(value)?;
            operands += 1;
        }
        index += 1;
    }
    if operands < grammar.min_operands {
        return Err(DeniedEffect::UnsupportedArgument);
    }
    Ok(())
}

/// Whether `value` is `flag` with its argument fused on, as in `-n20`.
fn fused_value(value: &str, flag: &str) -> bool {
    flag.len() == 2
        && value.len() > 2
        && value.starts_with(flag)
        && value[2..].chars().all(|c| c.is_ascii_digit())
}

/// Refuses an operand that could name something outside the working directory.
///
/// A direct command's cwd is an authorized root, so a relative operand with no
/// `..` component resolves inside it. This is the only reason the terminal tool
/// is bounded by the same roots the read tools are; there is no OS sandbox
/// underneath it (design, "Risks and controls").
fn operand_is_reachable(value: &str) -> Result<(), DeniedEffect> {
    if value.starts_with('/') {
        return Err(DeniedEffect::UnsupportedArgument);
    }
    if value.split('/').any(|component| component == "..") {
        return Err(DeniedEffect::UnsupportedArgument);
    }
    Ok(())
}

/// Decides whether the whole word list is an admitted read-only command.
fn plan(words: &[Word]) -> Result<(), DeniedEffect> {
    let executable = words[0].value.as_str();
    let arguments = &words[1..];

    if FILESYSTEM_WRITERS.contains(&executable) {
        return Err(DeniedEffect::FilesystemWrite);
    }
    if NETWORK_COMMANDS.contains(&executable) {
        return Err(DeniedEffect::NetworkAccess);
    }
    if PROCESS_COMMANDS.contains(&executable) {
        return Err(DeniedEffect::ProcessOrSystem);
    }
    // A path-qualified executable is not the executable the grammar named; it
    // is whatever is at that path right now.
    if executable.contains('/') {
        return Err(DeniedEffect::UnknownCommand);
    }

    match executable {
        "pwd" => check(
            arguments,
            &Grammar {
                flags: &["-P", "-L"],
                ..NO_ARGS
            },
        ),
        "ls" => check(
            arguments,
            &Grammar {
                flags: &[
                    "-l",
                    "-a",
                    "-A",
                    "-h",
                    "-1",
                    "-R",
                    "-t",
                    "-r",
                    "-S",
                    "-F",
                    "-la",
                    "-al",
                    "-lh",
                    "-lha",
                    "-hl",
                    "-ltr",
                    "--color=never",
                ],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "cat" => check(
            arguments,
            &Grammar {
                flags: &["-n", "-b", "-e", "-s"],
                valued: &[],
                operands: true,
                min_operands: 1,
            },
        ),
        "head" | "tail" => check(
            arguments,
            &Grammar {
                flags: &[],
                valued: &["-n", "-c"],
                operands: true,
                min_operands: 1,
            },
        ),
        "wc" => check(
            arguments,
            &Grammar {
                flags: &["-l", "-w", "-c", "-m"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "which" | "file" | "basename" | "dirname" => check(
            arguments,
            &Grammar {
                flags: &[],
                valued: &[],
                operands: true,
                min_operands: 1,
            },
        ),
        "echo" => check(
            arguments,
            &Grammar {
                flags: &["-n"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "grep" => check(
            arguments,
            &Grammar {
                flags: &[
                    "-n",
                    "-i",
                    "-r",
                    "-R",
                    "-l",
                    "-L",
                    "-c",
                    "-w",
                    "-x",
                    "-F",
                    "-E",
                    "-v",
                    "-H",
                    "-h",
                    "--color=never",
                ],
                valued: &["-e", "-A", "-B", "-C", "-m"],
                operands: true,
                min_operands: 1,
            },
        ),
        "node" | "deno" | "rustc" | "python3" => {
            // A bare interpreter is an interactive session, not a read, so the
            // version flag is not optional here the way a flag usually is.
            if arguments.is_empty() {
                return Err(DeniedEffect::UnsupportedArgument);
            }
            check(
                arguments,
                &Grammar {
                    flags: &["-v", "-V", "--version"],
                    ..NO_ARGS
                },
            )
        }
        "git" => plan_git(arguments),
        "cargo" => plan_cargo(arguments),
        _ => Err(DeniedEffect::UnknownCommand),
    }
}

/// The read-only `git` subcommands, each with the flags it may carry.
///
/// Deliberately small. `git branch` is read-only and `git branch -D` is not, so
/// the flag list is the boundary rather than the subcommand name.
fn plan_git(arguments: &[Word]) -> Result<(), DeniedEffect> {
    let Some(subcommand) = arguments.first() else {
        return Err(DeniedEffect::UnsupportedArgument);
    };
    let rest = &arguments[1..];
    match subcommand.value.as_str() {
        "status" => check(
            rest,
            &Grammar {
                flags: &["-s", "--short", "--porcelain", "-b", "--branch", "--long"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "diff" => check(
            rest,
            &Grammar {
                flags: &[
                    "--stat",
                    "--cached",
                    "--staged",
                    "--name-only",
                    "--name-status",
                    "--numstat",
                    "--no-color",
                ],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "log" => check(
            rest,
            &Grammar {
                flags: &[
                    "--oneline",
                    "--graph",
                    "--stat",
                    "--name-only",
                    "--decorate",
                    "--no-color",
                    "--all",
                ],
                valued: &["-n", "--max-count"],
                operands: true,
                min_operands: 0,
            },
        ),
        "show" => check(
            rest,
            &Grammar {
                flags: &["--stat", "--name-only", "--no-color", "--oneline"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "rev-parse" => check(
            rest,
            &Grammar {
                flags: &[
                    "--abbrev-ref",
                    "--short",
                    "--show-toplevel",
                    "--git-dir",
                    "--verify",
                    "--is-inside-work-tree",
                ],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "ls-files" => check(
            rest,
            &Grammar {
                flags: &["--cached", "--others", "--modified", "--exclude-standard"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        "branch" => check(
            rest,
            &Grammar {
                flags: &["--list", "-a", "-r", "--show-current", "--no-color"],
                ..NO_ARGS
            },
        ),
        "remote" => check(
            rest,
            &Grammar {
                flags: &["-v", "--verbose"],
                ..NO_ARGS
            },
        ),
        "describe" => check(
            rest,
            &Grammar {
                flags: &["--tags", "--always", "--dirty"],
                valued: &[],
                operands: true,
                min_operands: 0,
            },
        ),
        _ => Err(DeniedEffect::UnsupportedArgument),
    }
}

/// The `cargo` subcommands that read or test, each with the flags it may carry.
///
/// `cargo test` and `cargo build` write into `target/`, which is a build
/// artifact directory the project already declares disposable, and they do not
/// publish, install, or fetch credentials. `cargo publish`, `cargo install`, and
/// `cargo run` are absent for the opposite reasons.
fn plan_cargo(arguments: &[Word]) -> Result<(), DeniedEffect> {
    let Some(first) = arguments.first() else {
        return Err(DeniedEffect::UnsupportedArgument);
    };
    if first.value.starts_with('-') {
        return check(
            arguments,
            &Grammar {
                flags: &["--version", "-V", "--list", "--offline", "--locked"],
                ..NO_ARGS
            },
        );
    }

    let rest = &arguments[1..];
    let build_like = Grammar {
        flags: &[
            "--all",
            "--workspace",
            "--lib",
            "--bins",
            "--tests",
            "--benches",
            "--examples",
            "--all-targets",
            "--all-features",
            "--no-default-features",
            "--quiet",
            "-q",
            "--verbose",
            "-v",
            "--offline",
            "--locked",
            "--frozen",
            "--release",
            "--no-run",
            "--no-fail-fast",
            "--message-format=short",
        ],
        valued: &[
            "-p",
            "--package",
            "--test",
            "--bin",
            "--example",
            "--features",
            "--target",
            "--jobs",
            "-j",
        ],
        operands: true,
        min_operands: 0,
    };

    match first.value.as_str() {
        "test" | "check" | "build" | "clippy" | "bench" => check(rest, &build_like),
        "fmt" => check(
            rest,
            &Grammar {
                flags: &["--check", "--all", "--quiet", "-q", "--verbose"],
                ..NO_ARGS
            },
        ),
        "tree" => check(
            rest,
            &Grammar {
                flags: &["--offline", "--locked", "--no-dedupe", "--all-features"],
                valued: &["-p", "--package", "--depth"],
                operands: false,
                min_operands: 0,
            },
        ),
        "metadata" => check(
            rest,
            &Grammar {
                flags: &["--offline", "--locked", "--no-deps"],
                valued: &["--format-version"],
                operands: false,
                min_operands: 0,
            },
        ),
        _ => Err(DeniedEffect::UnsupportedArgument),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(command: &str) -> Vec<String> {
        match classify(command) {
            CommandEffect::DirectReadOnly { argv } => argv,
            other => panic!("`{command}` was not admitted: {other:?}"),
        }
    }

    fn denied(command: &str) -> DeniedEffect {
        match classify(command) {
            CommandEffect::Denied(effect) => effect,
            other => panic!("`{command}` was admitted: {other:?}"),
        }
    }

    #[test]
    fn quoting_survives_lexing_in_both_directions() {
        assert_eq!(argv("echo 'a b'"), ["echo", "a b"]);
        assert_eq!(argv("echo \"a b\""), ["echo", "a b"]);
        // Adjacent quoted and bare pieces are one word, as a shell would join
        // them.
        assert_eq!(argv("echo a'b'c"), ["echo", "abc"]);
        assert_eq!(argv("echo ''"), ["echo", ""]);
    }

    #[test]
    fn an_unterminated_quote_is_refused_rather_than_completed() {
        assert_eq!(denied("echo 'a"), DeniedEffect::UnsupportedShell);
        assert_eq!(denied("echo \"a"), DeniedEffect::UnsupportedShell);
    }

    #[test]
    fn a_redirection_is_reported_as_the_write_it_is() {
        assert_eq!(denied("echo hi > out"), DeniedEffect::FilesystemWrite);
        assert_eq!(denied("echo hi &> out"), DeniedEffect::FilesystemWrite);
        assert_eq!(denied("echo hi & echo bye"), DeniedEffect::UnsupportedShell);
    }

    #[test]
    fn an_operand_that_could_leave_the_working_directory_is_refused() {
        assert_eq!(denied("cat /etc/passwd"), DeniedEffect::UnsupportedArgument);
        assert_eq!(
            denied("cat ../outside.txt"),
            DeniedEffect::UnsupportedArgument
        );
        assert_eq!(
            denied("cat a/../../b.txt"),
            DeniedEffect::UnsupportedArgument
        );
        // A `..` inside a name is not a parent component.
        assert_eq!(argv("cat a..b.txt"), ["cat", "a..b.txt"]);
    }

    #[test]
    fn a_path_qualified_executable_is_not_the_named_executable() {
        assert_eq!(denied("/bin/ls"), DeniedEffect::UnknownCommand);
        assert_eq!(denied("./script"), DeniedEffect::UnknownCommand);
    }

    #[test]
    fn a_valued_flag_consumes_its_value_in_both_spellings() {
        assert_eq!(argv("head -n 20 a.txt"), ["head", "-n", "20", "a.txt"]);
        assert_eq!(argv("head -n20 a.txt"), ["head", "-n20", "a.txt"]);
        // A valued flag with nothing after it is not a command.
        assert_eq!(denied("head -n"), DeniedEffect::UnsupportedArgument);
    }

    #[test]
    fn a_double_dash_hands_the_rest_to_the_program_being_run() {
        assert_eq!(
            argv("cargo test -- --nocapture"),
            ["cargo", "test", "--", "--nocapture"]
        );
    }

    #[test]
    fn a_command_that_needs_an_operand_is_not_admitted_without_one() {
        assert_eq!(denied("cat"), DeniedEffect::UnsupportedArgument);
        assert_eq!(denied("which"), DeniedEffect::UnsupportedArgument);
        assert_eq!(denied("node"), DeniedEffect::UnsupportedArgument);
    }

    #[test]
    fn every_denial_explains_itself_in_its_own_words() {
        // The reason reaches the model, so two denials that read the same would
        // teach it the same lesson for two different problems.
        let all = [
            DeniedEffect::FilesystemWrite,
            DeniedEffect::NetworkAccess,
            DeniedEffect::ProcessOrSystem,
            DeniedEffect::DynamicShell,
            DeniedEffect::UnsupportedShell,
            DeniedEffect::UnknownCommand,
            DeniedEffect::UnsupportedArgument,
            DeniedEffect::Empty,
            DeniedEffect::TooLong,
        ];
        let mut reasons: Vec<&str> = all.iter().map(|effect| effect.describe()).collect();
        let count = reasons.len();
        reasons.sort_unstable();
        reasons.dedup();
        assert_eq!(reasons.len(), count);
        for effect in all {
            assert!(!effect.describe().is_empty());
            assert_eq!(effect.to_string(), effect.describe());
        }
    }

    #[test]
    fn a_command_longer_than_the_bound_is_refused_before_it_is_lexed() {
        let long = format!("echo {}", "a".repeat(MAX_COMMAND_BYTES));
        assert_eq!(
            classify(&long),
            CommandEffect::Denied(DeniedEffect::TooLong)
        );
    }
}
