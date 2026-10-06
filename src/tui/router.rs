//! Where a submitted line goes once it is known to be a command, **on the TUI
//! surface**.
//!
//! The whole module is one `match`, and it is the *TUI's* only one: what a
//! leading `/` means is [`crate::interactive::classify`]'s to say and what each
//! command *does* is `super::shell`'s to do, and between the two this is the
//! single dispatch point. It is **not** the only dispatch in the crate. The
//! line-oriented shell keeps its own exhaustive `match` inside its read loop
//! (`crate::interactive`'s `run`, the `Submitted::Command` arm), because that
//! loop owns the `async` and the mutable session state its arms reach for --
//! `apply_model(...).await`, the conversation, the model in force -- none of
//! which a `&mut self` trait method on this side could be handed.
//!
//! What keeps the two surfaces from drifting is therefore not a single call
//! site. It is that both parse the same `crate::interactive::SLASH_REGISTRY`
//! and both dispatch through an **exhaustive** `match` on [`Slash`]: another
//! variant stops *both* from compiling -- this one until it grows an arm and
//! [`CommandHandlers`] a method, and the line shell's until it grows an arm of
//! its own -- and `crate::interactive::SLASH_COMMANDS` and the parity ledger
//! stop reconciling until it is advertised. So "a command answered on one
//! surface and forgotten on the other" is a build failure, and this module is
//! where the TUI's half of that is spent.
//!
//! An **alias** never reaches this module as itself. `classify` resolves
//! `/exit` to [`Slash::Quit`] out of `crate::interactive::SLASH_REGISTRY`
//! before anything here runs, which is why the routing has one arm per
//! canonical command rather than one per name the registry answers.

use crate::interactive::{slash_spec, Slash, Submitted};

/// What a front end has to be able to do to be routed to.
///
/// Named after what the user asked for rather than after how this front end
/// answers it: `new_session` is `/new` on both surfaces, and the fact that the
/// TUI answers it by handing work to another thread while the line shell drops
/// a recorder is precisely what the trait exists to keep out of the routing.
pub(crate) trait CommandHandlers {
    fn help(&mut self);
    fn new_session(&mut self);
    fn clear(&mut self);
    /// `/reset`. The same operation as [`Self::new_session`] and
    /// [`Self::clear`] on this surface, as upstream's three are
    /// (`app_commands.zig:667-680`); a method of its own because the trait is
    /// named after what the user asked for.
    fn reset(&mut self);
    /// `/resume`: the session picker. It takes nothing.
    fn resume(&mut self);
    /// `/rename`, with the rest of the line -- already trimmed, and empty when
    /// there was none, which is the one refusal that names the command's own
    /// grammar (`usage: /rename <title>`).
    fn rename(&mut self, argument: &str);
    /// `/model`, with the rest of the line -- already trimmed, and empty when
    /// there was none.
    fn model(&mut self, argument: &str);
    /// `/copy`. It takes nothing: `/copy now` never reaches it ([`classify`]).
    fn copy(&mut self);
    /// `/stats`: what this surface's terminal writer has done so far.
    fn stats(&mut self);
    /// `/status`: the configuration a turn would run under, and the live
    /// conversation's counts.
    fn status(&mut self);
    /// `/statusline`, with the rest of the line -- already trimmed, and empty
    /// when there was none. Empty opens the picker; anything else names the
    /// item to flip, and what an item may be called is
    /// `crate::config::StatusLineItem::parse`'s to say.
    fn statusline(&mut self, argument: &str);
    fn version(&mut self);
    fn quit(&mut self);
    /// `/provider` (or its alias `/setup`), with the rest of the line --
    /// already trimmed, and empty when there was none.
    ///
    /// Bare, it opens the provider picker; with an argument, the argument is
    /// the provider to switch to. It is *not* validated here:
    /// what a provider name may be is `crate::provider::ProviderId::parse`'s to
    /// say, and a router that pre-screened it would be a second place that
    /// decides which backend a prompt can go to.
    fn setup(&mut self, argument: &str);
    /// `/permissions`, with the rest of the line -- already trimmed, and empty
    /// when there was none. What it may be is
    /// `crate::permission::slash::Request::parse`'s to say, not the router's.
    fn permissions(&mut self, argument: &str);
}

/// What one line submitted **on this surface** is.
///
/// [`crate::interactive::classify`]'s answer -- the same reading of a leading
/// `/` the line shell has, and the same registry -- with upstream's one rule
/// about arguments on top (`command_router.zig:121-133`): a command that takes
/// an argument matches by prefix and gets the rest of the line, and **every
/// other command matches exactly**, trailing spaces and tabs tolerated and
/// nothing else (`matchesCommandToken`, `registry.zig:59-68`). `/version now`
/// is therefore not `/version`: it is a line that names no command, and it is
/// answered as one (`fx-basic-40x120-version-trailing-space.txt`).
///
/// The line is left-trimmed first, as upstream's submit trims it
/// (`input_submit_runtime.zig:707`), so leading blanks are not an argument.
///
/// **This surface's rule and not the line shell's.** `crate::interactive`'s
/// read loop still ignores what follows a command that takes nothing; the TUI
/// is the surface held to upstream's behaviour, and the two classifications
/// differ in exactly this and in the next rule.
///
/// **A slash word that names nothing is a prompt when it is not a lone
/// word.** Upstream refuses an unknown name only when it is the whole line and
/// has no second `/` in it (`shouldRouteUnknownSlashCommand`,
/// `input_submit_runtime.zig:926-934`); `/tmp/x is empty, why?` or
/// `/nosuch please` goes to the model as written.
pub(crate) fn classify(line: &str) -> Submitted {
    let submitted = crate::interactive::classify(line);
    if let Submitted::UnknownCommand { token } = &submitted {
        let trimmed = line.trim();
        if trimmed != token.as_str() || token[1..].contains('/') {
            return Submitted::Prompt(trimmed.to_string());
        }
    }
    match &submitted {
        Submitted::Command { command, .. } if !exact(line, *command) => Submitted::UnknownCommand {
            token: line
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string(),
        },
        _ => submitted,
    }
}

/// Whether `line` names `command` the way upstream accepts it.
fn exact(line: &str, command: Slash) -> bool {
    if slash_spec(command).has_args {
        return true;
    }
    let line = line.trim_start_matches([' ', '\t', '\r', '\n']);
    // The name and nothing after it but spaces and tabs: anything else left
    // once those are gone -- a word, a line break -- is an argument the command
    // does not take.
    !line
        .trim_end_matches([' ', '\t'])
        .contains(char::is_whitespace)
}

/// Calls the one handler `submitted` names, or no handler at all.
///
/// **Anything that is not a command is silence here, not a default.** A blank
/// line, a prompt and a name the registry does not carry are all things this
/// module has nothing to say about, and a wildcard arm that "helpfully" fell
/// back to one of the commands would be a way for a typo to run one.
pub(crate) fn route(submitted: &Submitted, handlers: &mut impl CommandHandlers) {
    let Submitted::Command { command, argument } = submitted else {
        return;
    };
    match command {
        Slash::Help => handlers.help(),
        Slash::New => handlers.new_session(),
        Slash::Clear => handlers.clear(),
        Slash::Reset => handlers.reset(),
        Slash::Resume => handlers.resume(),
        Slash::Rename => handlers.rename(argument),
        Slash::Model => handlers.model(argument),
        Slash::Setup => handlers.setup(argument),
        Slash::Permissions => handlers.permissions(argument),
        Slash::Copy => handlers.copy(),
        Slash::Stats => handlers.stats(),
        Slash::Status => handlers.status(),
        Slash::Statusline => handlers.statusline(argument),
        Slash::Version => handlers.version(),
        Slash::Quit => handlers.quit(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_unknown_slash_word_with_more_on_the_line_is_a_prompt() {
        // The TUI's classifier, not the line shell's this module imports below.
        use super::classify;
        for line in ["/nosuch please", "/tmp/x is empty, why?", "/usr/bin"] {
            assert!(
                matches!(classify(line), Submitted::Prompt(ref text) if text == line),
                "{line:?}"
            );
        }
        assert!(matches!(
            classify("/nosuch"),
            Submitted::UnknownCommand { ref token } if token == "/nosuch"
        ));
        assert!(matches!(
            classify("/version now"),
            Submitted::UnknownCommand { .. }
        ));
    }
    use super::*;

    use crate::interactive::{classify, SLASH_COMMANDS};

    /// A handler that writes down what it was asked to do, and nothing else.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Recorded {
        calls: Vec<String>,
    }

    impl CommandHandlers for Recorded {
        fn help(&mut self) {
            self.calls.push("help".to_string());
        }
        fn new_session(&mut self) {
            self.calls.push("new".to_string());
        }
        fn clear(&mut self) {
            self.calls.push("clear".to_string());
        }
        fn reset(&mut self) {
            self.calls.push("reset".to_string());
        }
        fn resume(&mut self) {
            self.calls.push("resume".to_string());
        }
        fn rename(&mut self, argument: &str) {
            self.calls.push(format!("rename {argument}"));
        }
        fn model(&mut self, argument: &str) {
            self.calls.push(format!("model {argument}"));
        }
        fn copy(&mut self) {
            self.calls.push("copy".to_string());
        }
        fn stats(&mut self) {
            self.calls.push("stats".to_string());
        }
        fn status(&mut self) {
            self.calls.push("status".to_string());
        }
        fn statusline(&mut self, argument: &str) {
            self.calls.push(format!("statusline {argument}"));
        }
        fn version(&mut self) {
            self.calls.push("version".to_string());
        }
        fn quit(&mut self) {
            self.calls.push("quit".to_string());
        }
        fn setup(&mut self, argument: &str) {
            self.calls.push(format!("setup {argument}"));
        }
        fn permissions(&mut self, argument: &str) {
            self.calls.push(format!("permissions {argument}"));
        }
    }

    fn routed(line: &str) -> Vec<String> {
        let mut handlers = Recorded::default();
        route(&classify(line), &mut handlers);
        handlers.calls
    }

    #[test]
    fn every_canonical_name_reaches_exactly_one_handler() {
        let expected = [
            ("/help", "help"),
            ("/new", "new"),
            ("/clear", "clear"),
            ("/reset", "reset"),
            ("/resume", "resume"),
            ("/rename", "rename "),
            ("/model", "model "),
            ("/copy", "copy"),
            ("/stats", "stats"),
            ("/status", "status"),
            ("/statusline", "statusline "),
            ("/version", "version"),
            ("/quit", "quit"),
            ("/provider", "setup "),
            ("/permissions", "permissions "),
        ];
        assert_eq!(expected.len(), SLASH_COMMANDS.len());
        for (name, call) in expected {
            assert!(SLASH_COMMANDS.contains(&name), "{name} is not canonical");
            assert_eq!(routed(name), vec![call.to_string()], "{name}");
        }
    }

    #[test]
    fn an_alias_reaches_the_handler_its_command_does() {
        assert_eq!(routed("/exit"), vec!["quit".to_string()]);
        assert_eq!(routed("/setup"), vec!["setup ".to_string()]);
        assert_eq!(
            classify("/exit"),
            Submitted::Command {
                command: Slash::Quit,
                argument: String::new()
            }
        );
    }

    #[test]
    fn the_rest_of_the_line_reaches_the_one_handler_that_takes_it() {
        assert_eq!(
            routed("/model acme/model-9"),
            vec!["model acme/model-9".to_string()]
        );
        assert_eq!(routed("/setup llmux"), vec!["setup llmux".to_string()]);
        // Trimmed, as upstream's payload is (`command_router.zig:121-133`):
        // `/rename   My Title  ` names `My Title`.
        assert_eq!(
            routed("/rename   My Title  "),
            vec!["rename My Title".to_string()]
        );
        // Every argument form reaches the one handler, trimmed; what each
        // means is the handler's question.
        for (line, call) in [
            ("/permissions auto", "permissions auto"),
            ("/permissions   full-access  ", "permissions full-access"),
            ("/permissions full access", "permissions full access"),
            ("/permissions reset", "permissions reset"),
            ("/permissions bogus", "permissions bogus"),
        ] {
            assert_eq!(routed(line), vec![call.to_string()], "{line:?}");
            assert_eq!(super::classify(line), classify(line), "{line:?}");
        }
        // Upstream's `accepts_payload` (`commands.zig:494`): the argument is
        // the item, and whether it names one is the handler's question.
        assert_eq!(
            routed("/statusline context"),
            vec!["statusline context".to_string()]
        );
        assert_eq!(
            super::classify("/statusline  bogus words "),
            Submitted::Command {
                command: Slash::Statusline,
                argument: "bogus words".to_string()
            }
        );
    }

    /// Advertising without a handler is what this module exists to make
    /// impossible, and the claim is made over the **registry** rather than over
    /// a list written here: a name added to `SLASH_COMMANDS` that nobody routed
    /// would leave `routed` empty, and a name routed to two handlers would leave
    /// it with two entries.
    #[test]
    fn no_advertised_name_is_advertised_without_a_handler() {
        for name in SLASH_COMMANDS {
            let calls = routed(name);
            assert_eq!(
                calls.len(),
                1,
                "{name} reached {} handlers rather than exactly one",
                calls.len()
            );
        }
    }

    #[test]
    fn every_registry_name_and_alias_reaches_a_handler() {
        use crate::interactive::SLASH_REGISTRY;
        for spec in SLASH_REGISTRY {
            for name in std::iter::once(&spec.name).chain(spec.aliases.iter()) {
                assert_eq!(routed(name).len(), 1, "{name} routes to no handler");
            }
        }
    }

    #[test]
    fn a_command_that_takes_nothing_is_matched_exactly_on_this_surface() {
        // `super::classify` throughout: the bare name in this module is the
        // line shell's, imported above.
        // `fx-basic-40x120-version-trailing-space.txt`: `/version ` runs,
        // `/version now` and `/help extra` are unknown commands
        // (`registry.zig:59-68`).
        for line in [
            "/version",
            "/version ",
            "/version \t ",
            "  /version",
            "/exit  ",
            "/copy ",
            "/stats ",
            "/status\t",
        ] {
            assert!(
                matches!(super::classify(line), Submitted::Command { .. }),
                "{line:?} did not run"
            );
        }
        for line in [
            "/version now",
            "/help extra",
            "/version\tnow",
            "/version\nnow",
            "/version\n",
            "/quit please",
            "/exit now",
            "/copy now",
            "/copy\tall",
            // `fx-stats-40x120-extra.txt`: `/stats extra` is an unknown
            // command, and `/status` takes nothing either (`commands.zig:473,475`
            // gives neither row `accepts_payload`).
            "/stats extra",
            "/status now",
            "/status\tall",
        ] {
            assert!(
                matches!(super::classify(line), Submitted::UnknownCommand { .. }),
                "{line:?} ran a command that takes nothing"
            );
        }
        // And a command that takes something still gets it, trimmed.
        assert_eq!(
            super::classify("/model   acme/model-9  "),
            Submitted::Command {
                command: Slash::Model,
                argument: "acme/model-9".to_string()
            }
        );
        assert!(matches!(
            super::classify("/setup"),
            Submitted::Command {
                command: Slash::Setup,
                ..
            }
        ));
        // Nothing else about the line shell's reading changes.
        assert_eq!(super::classify("/nonesuch"), classify("/nonesuch"));
        assert_eq!(super::classify("a/b"), classify("a/b"));
        assert_eq!(super::classify("   "), Submitted::Blank);
    }

    #[test]
    fn the_line_shell_keeps_its_own_reading_of_an_argument() {
        // The TUI is the surface held to upstream here; `crate::interactive`'s
        // own classifier, which the line shell reads, still takes the command.
        assert!(matches!(
            classify("/version now"),
            Submitted::Command {
                command: Slash::Version,
                ..
            }
        ));
    }

    #[test]
    fn nothing_that_is_not_a_command_calls_a_handler() {
        for line in ["", "   ", "/nonesuch", "/HELP", "ask me something", "a/b"] {
            assert_eq!(routed(line), Vec::<String>::new(), "{line}");
        }
    }
}
