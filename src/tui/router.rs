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
//! and both dispatch through an **exhaustive** `match` on [`Slash`]: a seventh
//! variant stops *both* from compiling -- this one until it grows an arm and
//! [`CommandHandlers`] a method, and the line shell's until it grows an arm of
//! its own -- and `crate::interactive::SLASH_COMMANDS` and the parity ledger
//! stop reconciling until it is advertised. So "a command answered on one
//! surface and forgotten on the other" is a build failure, and this module is
//! where the TUI's half of that is spent.
//!
//! An **alias** never reaches this module as itself. `classify` resolves
//! `/exit` to [`Slash::Quit`] out of `crate::interactive::SLASH_REGISTRY`
//! before anything here runs, which is why the routing has six arms rather than
//! six plus however many other names the registry has grown.

use crate::interactive::{Slash, Submitted};

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
    /// `/model`, with the rest of the line -- already trimmed, and empty when
    /// there was none.
    fn model(&mut self, argument: &str);
    fn version(&mut self);
    fn quit(&mut self);
}

/// Calls the one handler `submitted` names, or no handler at all.
///
/// **Anything that is not a command is silence here, not a default.** A blank
/// line, a prompt and a name that is not one of the six are all things this
/// module has nothing to say about, and a wildcard arm that "helpfully" fell
/// back to one of the six would be a way for a typo to run a command.
pub(crate) fn route(submitted: &Submitted, handlers: &mut impl CommandHandlers) {
    let Submitted::Command { command, argument } = submitted else {
        return;
    };
    match command {
        Slash::Help => handlers.help(),
        Slash::New => handlers.new_session(),
        Slash::Clear => handlers.clear(),
        Slash::Model => handlers.model(argument),
        Slash::Version => handlers.version(),
        Slash::Quit => handlers.quit(),
    }
}

#[cfg(test)]
mod tests {
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
        fn model(&mut self, argument: &str) {
            self.calls.push(format!("model {argument}"));
        }
        fn version(&mut self) {
            self.calls.push("version".to_string());
        }
        fn quit(&mut self) {
            self.calls.push("quit".to_string());
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
            ("/model", "model "),
            ("/version", "version"),
            ("/quit", "quit"),
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
    }

    #[test]
    fn nothing_that_is_not_a_command_calls_a_handler() {
        for line in ["", "   ", "/nonesuch", "/HELP", "ask me something", "a/b"] {
            assert_eq!(routed(line), Vec::<String>::new(), "{line}");
        }
    }
}
