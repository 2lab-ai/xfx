//! The command grammar. It decides what fxr accepts; it never decides what a
//! command does.
//!
//! The command set is closed on purpose. Advertisement is a promise: a name that
//! appears in `--help` must have a handler behind it, so a command is added here
//! only in the same change that implements it. Upstream's much larger command
//! union (`vercel-labs/fx@580a0c5d src/core/cli/cli_surface.zig:58-84`) is
//! reconciled row by row in `docs/parity.md`, not mirrored here.

use clap::error::ErrorKind;
use clap::{ArgAction, ColorChoice, CommandFactory, Parser, Subcommand};

/// Every command name the parser accepts, including clap's built-in `help`.
///
/// `scripts/check-no-stubs.sh` reconciles this list against `docs/parity.md`, and
/// [`parser_command_names`] proves it cannot drift from the real parser.
pub const ADVERTISED_COMMANDS: &[&str] = &["ask", "doctor", "help", "status"];

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
    },
    /// Report resolved configuration and credentials.
    Status { json: bool },
    /// Run local diagnostics.
    Doctor { json: bool },
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
                Command::Ask {
                    prompt,
                    json,
                    no_save,
                }
            }
            Some(RawCommand::Status { json }) => Command::Status { json },
            Some(RawCommand::Doctor { json }) => Command::Doctor { json },
            // The interactive shell is not part of this release slice, so a bare
            // invocation has nothing to run and says so instead of succeeding.
            None => Command::Rejected {
                message: format!("fxr: no command given\n\n{}", help_text()),
            },
        }
    }
}

#[derive(Debug, Subcommand)]
enum RawCommand {
    /// Ask the model one question and stream the answer
    ///
    /// The permission-mode and resume flags from upstream's `ask` are not here
    /// yet; they arrive with the tool and session slices (`docs/parity.md`).
    Ask {
        /// Emit one JSON event per line instead of plain text
        #[arg(long)]
        json: bool,
        // The parenthetical is a fact about this build, not a caveat about the
        // flag. `ask` has no session store yet, so the flag's guarantee already
        // holds and the default is not yet different from it; help that said
        // only "do not record this turn in a session" would let a reader infer
        // that the default does record one. The flag's meaning does not change
        // when sessions arrive -- only the parenthetical goes away
        // (`docs/parity.md`, `ask --no-save`).
        /// Do not record this turn in a session (this release records none either way)
        #[arg(long = "no-save")]
        no_save: bool,
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
            }
        );
        assert_eq!(
            parse(&["ask", "--json", "--no-save", "hi"]),
            Command::Ask {
                prompt: "hi".to_string(),
                json: true,
                no_save: true,
            }
        );
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
    fn ask_does_not_advertise_a_deferred_flag() {
        // Advertisement is a promise. `--auto`, `--yolo`, and `--resume` need
        // permission modes and sessions, which this release does not have.
        for args in [
            vec!["ask", "--auto", "hi"],
            vec!["ask", "--yolo", "hi"],
            vec!["ask", "--resume", "last", "hi"],
            vec!["ask", "--quiet", "hi"],
        ] {
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
    fn a_bare_invocation_is_rejected_with_usage() {
        let Command::Rejected { message } = parse(&[]) else {
            panic!("a bare invocation must be rejected until the shell exists");
        };
        assert!(message.contains("Usage:"), "{message}");
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
