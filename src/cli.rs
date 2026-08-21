//! The command grammar. It decides what fxr accepts; it never decides what a
//! command does.
//!
//! The command set is closed on purpose. Advertisement is a promise: a name that
//! appears in `--help` must have a handler behind it, so a command is added here
//! only in the same change that implements it. Upstream's much larger command
//! union (`vercel-labs/fx@580a0c5d src/core/cli/cli_surface.zig:58-84`) is
//! reconciled row by row in `docs/parity.md`, not mirrored here.

use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{ArgAction, ColorChoice, CommandFactory, Parser, Subcommand};

use crate::config::PermissionMode;
use crate::session::Selector;

/// Every command name the parser accepts, including clap's built-in `help`.
///
/// `scripts/check-no-stubs.sh` reconciles this list against `docs/parity.md`, and
/// [`parser_command_names`] proves it cannot drift from the real parser.
///
/// The layout is pinned with `rustfmt::skip` because the script reads this
/// declaration textually, matching `pub const NAME: &[&str] = &[` and then the
/// entries. It has to work without building the crate -- a broken build must not
/// be able to hide a broken promise -- so a reflow that split the opening line
/// would silently disable the check rather than fail it.
#[rustfmt::skip]
pub const ADVERTISED_COMMANDS: &[&str] = &[
    "ask",
    "doctor",
    "help",
    "session",
    "sessions",
    "status",
];

/// Runtime surfaces reached without naming a subcommand.
///
/// The interactive shell is a real command with a parity row and a handler, but
/// it has no name to type: it is what a bare `fxr` runs. Declaring it here is
/// what lets `scripts/check-no-stubs.sh` hold every `implemented` command row to
/// an advertised surface in *both* directions -- otherwise "implemented" could
/// be claimed for a command the binary does not actually reach.
///
/// The same `rustfmt::skip` reasoning as [`ADVERTISED_COMMANDS`] applies: the
/// script reads this declaration textually.
#[rustfmt::skip]
pub const ADVERTISED_ENTRYPOINTS: &[&str] = &[
    "interactive",
];

/// A parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub command: Command,
}

/// What the user asked for, including the outcomes that are not a command.
///
/// Parse failures are represented rather than printed here, so that
/// [`crate::app::run`] stays the single place that maps an outcome to an exit
/// code and a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the interactive shell. What a bare `fxr` means.
    Interactive,
    /// Print a navigation page. `page` is the exact text clap rendered, so
    /// `fxr help`, `fxr --help`, and `fxr status --help` each show their own.
    Help { page: String },
    /// Print the version.
    Version,
    /// Run one streamed model turn against the configured provider.
    Ask {
        /// The already-joined user prompt. Never blank; a blank one is turned
        /// into [`Command::Rejected`] rather than sent.
        prompt: String,
        /// Emit JSONL turn events instead of plain assistant text.
        json: bool,
        /// Do not record this turn in a session.
        no_save: bool,
        /// Directories the turn's read tools may reach outside the workspace,
        /// in the order the user named them. Empty means workspace-only.
        add_dirs: Vec<PathBuf>,
        /// The permission mode this invocation asked for, if it asked. `None`
        /// leaves the configured mode alone; the flags are an override for one
        /// run, not a way to rewrite settings.
        mode: Option<PermissionMode>,
        /// The session to continue, if the invocation named one. `None` starts
        /// a new one.
        resume: Option<Selector>,
    },
    /// Report resolved configuration and credentials.
    Status { json: bool },
    /// Run local diagnostics.
    Doctor { json: bool },
    /// List saved sessions, newest first.
    Sessions {
        json: bool,
        /// Include sessions bound to other workspaces.
        all: bool,
        /// How many to show. `0` means the store's default.
        limit: usize,
    },
    /// Show one saved session.
    Session { json: bool, selector: Selector },
    /// The invocation was rejected; `message` is the exact diagnostic to show.
    Rejected { message: String },
}

impl Cli {
    /// Parses the real process arguments.
    pub fn from_process_args() -> Self {
        Self::from_args(std::env::args_os())
    }

    /// Parses an explicit argument list, including `argv[0]`.
    pub fn from_args<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let command = match RawCli::try_parse_from(args) {
            Ok(raw) => raw.into_command(),
            Err(err) => match err.kind() {
                // `--help`, `-h`, and the `help` subcommand all land here.
                ErrorKind::DisplayHelp => Command::Help {
                    page: err.render().to_string(),
                },
                _ => Command::Rejected {
                    message: err.render().to_string(),
                },
            },
        };
        Self { command }
    }
}

/// The command names the parser actually accepts, read back from clap.
///
/// This is the mechanical source of truth for "what does fxr advertise".
pub fn parser_command_names() -> Vec<String> {
    let mut command = RawCli::command();
    // `help` is generated during the build pass, so an unbuilt command would
    // under-report the real surface.
    command.build();
    command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect()
}

/// The rendered top-level navigation page.
pub fn help_text() -> String {
    RawCli::command().render_help().to_string()
}

/// The subcommand is optional at the parser level so that `fxr --version`
/// resolves before any command is required. A missing command is turned into an
/// explicit rejection in [`RawCli::into_command`] rather than silently doing
/// nothing.
#[derive(Debug, Parser)]
#[command(
    name = "fxr",
    bin_name = "fxr",
    about = "Unofficial Rust port of the fx terminal coding agent",
    disable_version_flag = true,
    color = ColorChoice::Never
)]
struct RawCli {
    /// Print the version and exit
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue, global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<RawCommand>,
}

impl RawCli {
    fn into_command(self) -> Command {
        // `--version` is accepted anywhere, matching upstream's `-v`/`--version`
        // aliases (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:441-452`), and
        // it outranks whatever command accompanied it.
        if self.version {
            return Command::Version;
        }
        match self.command {
            Some(RawCommand::Ask {
                prompt,
                json,
                no_save,
                add_dir,
                auto,
                yolo,
                resume,
                resume_id,
            }) => {
                // Words are rejoined with a single space: the shell already
                // split them, and preserving the original spacing would need
                // the raw command line, which fxr does not have.
                let prompt = prompt.join(" ").trim().to_string();
                if prompt.is_empty() {
                    return Command::Rejected {
                        message: "fxr ask: the prompt is empty; give fxr something to ask"
                            .to_string(),
                    };
                }
                // clap already rejects `--auto --yolo` together, so at most one
                // is set here; the `else` order is not a precedence rule.
                let mode = if yolo {
                    Some(PermissionMode::Yolo)
                } else if auto {
                    Some(PermissionMode::Auto)
                } else {
                    None
                };
                let resume = match resume_selector(resume.as_deref(), resume_id.as_deref()) {
                    Ok(resume) => resume,
                    Err(message) => return Command::Rejected { message },
                };
                Command::Ask {
                    prompt,
                    json,
                    no_save,
                    add_dirs: add_dir,
                    mode,
                    resume,
                }
            }
            Some(RawCommand::Status { json }) => Command::Status { json },
            Some(RawCommand::Doctor { json }) => Command::Doctor { json },
            Some(RawCommand::Sessions { json, all, limit }) => Command::Sessions {
                json,
                all,
                limit: limit.unwrap_or(0),
            },
            Some(RawCommand::Session { json, selector, id }) => {
                match resume_selector(selector.as_deref(), id.as_deref()) {
                    Ok(Some(selector)) => Command::Session { json, selector },
                    Ok(None) => Command::Rejected {
                        message:
                            "fxr session: name a session, or `last` for the most recent one in \
                              this workspace"
                                .to_string(),
                    },
                    Err(message) => Command::Rejected { message },
                }
            }
            // A bare `fxr` starts the shell, as upstream does
            // (`vercel-labs/fx@580a0c5d src/core/cli/cli_surface.zig:443`).
            // Whether this terminal can host one is not a grammar question, so
            // it is decided by the handler rather than here.
            None => Command::Interactive,
        }
    }
}

/// Turns the two spellings of "which session" into one answer.
///
/// `--resume <last|id>` is the convenient form and `--resume-id <id>` is the
/// exact one, which is the only way to name a session that is literally called
/// `last`. clap already refuses both at once, so at most one is set here.
fn resume_selector(
    positional: Option<&str>,
    exact: Option<&str>,
) -> Result<Option<Selector>, String> {
    let parsed = match (positional, exact) {
        (Some(raw), _) => Selector::parse(raw),
        (None, Some(raw)) => crate::session::SessionId::parse(raw).map(Selector::Id),
        (None, None) => return Ok(None),
    };
    parsed.map(Some).map_err(|err| format!("fxr: {err}"))
}

#[derive(Debug, Subcommand)]
enum RawCommand {
    /// Ask the model one question and stream the answer
    Ask {
        /// Emit one JSON event per line instead of plain text
        #[arg(long)]
        json: bool,
        // Now load-bearing: the default writes `~/.fxr/sessions/<id>/`, and
        // this flag means nothing is created there at all -- not an empty
        // directory, not a manifest. It conflicts with the resume flags because
        // continuing a conversation you refuse to record would silently fork
        // its history.
        /// Do not record this turn in a session
        #[arg(long = "no-save", conflicts_with_all = ["resume", "resume_id"])]
        no_save: bool,
        /// Continue the most recent session of this workspace, or the one named
        #[arg(long, value_name = "last|ID", conflicts_with = "resume_id")]
        resume: Option<String>,
        /// Continue exactly this session, even if it is called `last`
        #[arg(long = "resume-id", value_name = "ID")]
        resume_id: Option<String>,
        // Repeatable, and off by default. Without it the read tools see the
        // workspace and nothing else; upstream spells the same authority
        // `--add-dir` (`vercel-labs/fx@580a0c5d src/core/cli/cli_surface.zig:391-415`).
        // It is an `ask` flag rather than a global one because it only means
        // something for a turn -- `status` and `doctor` read no files.
        /// Let this turn's read tools also read PATH (repeatable)
        #[arg(long = "add-dir", value_name = "PATH", action = ArgAction::Append)]
        add_dir: Vec<PathBuf>,
        // The two mode flags are `conflicts_with` rather than an enum-valued
        // option because that is the spelling upstream uses
        // (`cli_surface.zig:61`), and because `--yolo` should be a word a user
        // has to type deliberately rather than a value they could mistype into.
        /// Run bounded reversible workspace changes and read-only commands without asking
        #[arg(long, conflicts_with = "yolo")]
        auto: bool,
        /// Skip every permission check for this run (prints a warning)
        #[arg(long)]
        yolo: bool,
        /// The question. Everything after `--`, and everything after the first
        /// prompt word, is prompt text rather than a flag. A leading `-` before
        /// the prompt is still an unknown flag, so a typo is reported instead
        /// of silently becoming part of the question.
        #[arg(trailing_var_arg = true, num_args = 1..)]
        prompt: Vec<String>,
    },
    /// Report the resolved model, credential source, and workspace
    Status {
        /// Emit one JSON document instead of text
        #[arg(long)]
        json: bool,
    },
    /// Check the local installation and report every problem it finds
    Doctor {
        /// Emit one JSON document instead of text
        #[arg(long)]
        json: bool,
    },
    /// List saved sessions, newest first
    Sessions {
        /// Emit one JSON document instead of text
        #[arg(long)]
        json: bool,
        /// Include sessions belonging to other workspaces
        #[arg(long)]
        all: bool,
        /// Show at most this many
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show one saved session, including its turns
    Session {
        /// Emit one JSON document instead of text
        #[arg(long)]
        json: bool,
        /// Which session: `last`, or an id
        #[arg(value_name = "last|ID", conflicts_with = "id")]
        selector: Option<String>,
        /// Exactly this session, even if it is called `last`
        #[arg(long, value_name = "ID")]
        id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Command {
        let mut argv = vec!["fxr"];
        argv.extend_from_slice(args);
        Cli::from_args(argv).command
    }

    #[test]
    fn the_parser_accepts_exactly_the_advertised_commands() {
        let mut names = parser_command_names();
        names.sort();
        let mut advertised: Vec<String> = ADVERTISED_COMMANDS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        advertised.sort();
        assert_eq!(names, advertised);
    }

    #[test]
    fn help_aliases_all_resolve_to_the_help_command() {
        for args in [vec!["help"], vec!["--help"], vec!["-h"]] {
            assert!(
                matches!(parse(&args), Command::Help { .. }),
                "{args:?} must render help"
            );
        }
    }

    #[test]
    fn subcommand_help_renders_that_subcommands_page() {
        let Command::Help { page } = parse(&["status", "--help"]) else {
            panic!("`status --help` must render help");
        };
        assert!(page.contains("--json"), "{page}");
    }

    #[test]
    fn version_aliases_all_resolve_to_the_version_command() {
        for args in [vec!["--version"], vec!["-v"]] {
            assert_eq!(parse(&args), Command::Version, "{args:?}");
        }
    }

    #[test]
    fn version_outranks_a_command() {
        assert_eq!(parse(&["status", "--version"]), Command::Version);
    }

    #[test]
    fn status_and_doctor_carry_only_the_json_flag() {
        assert_eq!(parse(&["status"]), Command::Status { json: false });
        assert_eq!(parse(&["status", "--json"]), Command::Status { json: true });
        assert_eq!(parse(&["doctor"]), Command::Doctor { json: false });
        assert_eq!(parse(&["doctor", "--json"]), Command::Doctor { json: true });
    }

    #[test]
    fn ask_joins_its_prompt_words_and_carries_only_the_implemented_flags() {
        assert_eq!(
            parse(&["ask", "explain", "this", "code"]),
            Command::Ask {
                prompt: "explain this code".to_string(),
                json: false,
                no_save: false,
                add_dirs: Vec::new(),
                mode: None,
                resume: None,
            }
        );
        assert_eq!(
            parse(&["ask", "--json", "--no-save", "hi"]),
            Command::Ask {
                prompt: "hi".to_string(),
                json: true,
                no_save: true,
                add_dirs: Vec::new(),
                mode: None,
                resume: None,
            }
        );
    }

    #[test]
    fn add_dir_is_repeatable_and_keeps_the_order_it_was_given() {
        assert_eq!(
            parse(&["ask", "--add-dir", "/one", "--add-dir=/two", "hi"]),
            Command::Ask {
                prompt: "hi".to_string(),
                json: false,
                no_save: false,
                add_dirs: vec![PathBuf::from("/one"), PathBuf::from("/two")],
                mode: None,
                resume: None,
            }
        );
    }

    /// The session an `ask` invocation asked to continue.
    fn resume_of(args: &[&str]) -> Option<Selector> {
        match parse(args) {
            Command::Ask { resume, .. } => resume,
            other => panic!("{args:?} must be an ask: {other:?}"),
        }
    }

    #[test]
    fn resume_names_the_latest_session_or_an_exact_one() {
        assert_eq!(resume_of(&["ask", "hi"]), None);
        assert_eq!(
            resume_of(&["ask", "--resume", "last", "hi"]),
            Some(Selector::Last)
        );
        assert_eq!(
            resume_of(&["ask", "--resume", "abc-1", "hi"]),
            Some(Selector::Id(
                crate::session::SessionId::parse("abc-1").unwrap()
            ))
        );
        // `--resume-id` is the escape hatch for a session literally named
        // `last`, so it never resolves to the keyword.
        assert_eq!(
            resume_of(&["ask", "--resume-id", "last", "hi"]),
            Some(Selector::Id(
                crate::session::SessionId::parse("last").unwrap()
            ))
        );
    }

    #[test]
    fn an_unsafe_resume_id_is_rejected_before_it_becomes_a_path() {
        for args in [
            vec!["ask", "--resume", "../escape", "hi"],
            vec!["ask", "--resume-id", "a/b", "hi"],
            vec!["session", "../escape"],
            vec!["session", "--id", ".."],
        ] {
            let Command::Rejected { message } = parse(&args) else {
                panic!("{args:?} must be rejected");
            };
            assert!(message.contains("session id"), "{args:?}: {message}");
        }
    }

    #[test]
    fn resuming_a_session_and_refusing_to_save_it_is_contradictory() {
        for args in [
            vec!["ask", "--resume", "last", "--no-save", "hi"],
            vec!["ask", "--resume-id", "abc", "--no-save", "hi"],
            // Two ways of naming the session at once is also a contradiction.
            vec!["ask", "--resume", "last", "--resume-id", "abc", "hi"],
        ] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn sessions_lists_with_an_optional_scope_and_bound() {
        assert_eq!(
            parse(&["sessions"]),
            Command::Sessions {
                json: false,
                all: false,
                limit: 0
            }
        );
        assert_eq!(
            parse(&["sessions", "--json", "--all", "--limit", "5"]),
            Command::Sessions {
                json: true,
                all: true,
                limit: 5
            }
        );
        // A limit has to be a number; a typo is a usage error, not a default.
        assert!(matches!(
            parse(&["sessions", "--limit", "many"]),
            Command::Rejected { .. }
        ));
    }

    #[test]
    fn session_needs_exactly_one_way_of_naming_its_session() {
        assert_eq!(
            parse(&["session", "last"]),
            Command::Session {
                json: false,
                selector: Selector::Last
            }
        );
        assert_eq!(
            parse(&["session", "--id", "abc", "--json"]),
            Command::Session {
                json: true,
                selector: Selector::Id(crate::session::SessionId::parse("abc").unwrap())
            }
        );
        for args in [
            vec!["session"],
            vec!["session", "last", "--id", "abc"],
            vec!["session", "one", "two"],
        ] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn add_dir_requires_a_directory_and_belongs_to_ask_alone() {
        // A missing value is a usage error rather than an empty authority.
        assert!(matches!(
            parse(&["ask", "--add-dir"]),
            Command::Rejected { .. }
        ));
        // `status` and `doctor` read no files, so the flag would promise
        // something they cannot do.
        for args in [
            vec!["status", "--add-dir", "/one"],
            vec!["doctor", "--add-dir", "/one"],
        ] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn ask_rejects_a_missing_or_blank_prompt() {
        for args in [vec!["ask"], vec!["ask", "--json"], vec!["ask", " \t "]] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn the_permission_flags_select_a_mode_and_cannot_be_combined() {
        let mode = |args: &[&str]| match parse(args) {
            Command::Ask { mode, .. } => mode,
            other => panic!("{args:?} must be an ask: {other:?}"),
        };
        assert_eq!(mode(&["ask", "hi"]), None);
        assert_eq!(mode(&["ask", "--auto", "hi"]), Some(PermissionMode::Auto));
        assert_eq!(mode(&["ask", "--yolo", "hi"]), Some(PermissionMode::Yolo));
        // Two modes at once has no honest meaning, and picking one silently
        // would make `--yolo --auto` and `--auto --yolo` mean different things.
        assert!(matches!(
            parse(&["ask", "--auto", "--yolo", "hi"]),
            Command::Rejected { .. }
        ));
        // The mode flags belong to `ask`; `status` and `doctor` change nothing.
        for args in [vec!["status", "--auto"], vec!["doctor", "--yolo"]] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn ask_does_not_advertise_a_deferred_flag() {
        // Advertisement is a promise. `--quiet` suppresses streamed output,
        // which fxr does not have a second output mode for yet.
        for args in [vec!["ask", "--quiet", "hi"], vec!["ask", "--acp", "hi"]] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected while the flag is deferred"
            );
        }
    }

    #[test]
    fn an_unknown_name_is_rejected_and_named_in_the_diagnostic() {
        let Command::Rejected { message } = parse(&["bogus"]) else {
            panic!("an unknown name must be rejected");
        };
        assert!(message.contains("bogus"), "{message}");
    }

    #[test]
    fn a_bare_invocation_asks_for_the_shell() {
        assert_eq!(parse(&[]), Command::Interactive);
    }

    #[test]
    fn the_shell_has_no_name_to_type() {
        // It is reached by giving no command at all, so `fxr interactive` is an
        // unknown name rather than a second spelling of the same thing.
        for name in ADVERTISED_ENTRYPOINTS {
            assert!(
                matches!(parse(&[name]), Command::Rejected { .. }),
                "`{name}` must not be a subcommand"
            );
        }
    }

    #[test]
    fn extra_arguments_and_unknown_flags_are_rejected() {
        for args in [
            vec!["status", "extra"],
            vec!["status", "--bogus"],
            vec!["doctor", "extra"],
            vec!["doctor", "--bogus"],
        ] {
            assert!(
                matches!(parse(&args), Command::Rejected { .. }),
                "{args:?} must be rejected"
            );
        }
    }

    #[test]
    fn every_deferred_upstream_command_is_rejected() {
        for name in [
            "acp",
            "pr",
            "issue",
            "login",
            "logout",
            "setup",
            "permissions",
            "models",
            "provider",
            "background",
            "teams",
            "credits",
            "usage",
            "upgrade",
            "replay",
            "workspace",
            // Upstream's `resume_session` command. fxr resumes through
            // `ask --resume`, so the bare name promises nothing.
            "resume",
        ] {
            assert!(
                matches!(parse(&[name]), Command::Rejected { .. }),
                "`{name}` must not be accepted"
            );
        }
    }

    #[test]
    fn the_help_page_lists_every_advertised_command() {
        let help = help_text();
        for name in ADVERTISED_COMMANDS {
            assert!(help.contains(name), "help omits `{name}`: {help}");
        }
    }

    #[test]
    fn clap_definitions_are_internally_consistent() {
        RawCli::command().debug_assert();
    }
}
