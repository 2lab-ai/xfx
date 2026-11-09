//! What a unit test has in place of the process environment.
//!
//! A unit test runs inside the developer's own process: its `HOME` is the
//! developer's home, its variables are the developer's -- the model, the
//! provider's credential, where git's global configuration is. A case that
//! reached [`crate::config::Environment::from_process`] through
//! `Runtime::new` and then reloaded read the real `~/.xfx/settings.json` into
//! memory, and one of its values reached a transcript (2026-11-09).
//!
//! So in this crate's unit tests every read of the environment that can name
//! the home, the profile, an xfx variable, a credential or git's
//! configuration goes through `process_env!` (`src/lib.rs`), which lands
//! here and **fails closed**:
//!
//! - Outside a [`FakeProcess`] it panics, naming the function that read and
//!   the variable it asked for (never a value). A case meant to build its own
//!   environment -- `Runtime::with_environment`, `RuntimeConfig::load_with`,
//!   `GitEnvironment::default` -- and that reached the real one instead fails
//!   on that line instead of passing on whatever the machine running it holds.
//! - Inside one, it reads the fake: the case's own home and its own fake
//!   values. The case for `from_process` itself is the one that needs this;
//!   a case that drives the TUI's worker, whose launch reads the process as a
//!   real launch does, is the other.
//!
//! A panic and not a quietly empty environment: an empty one makes a case
//! that reached the process pass green, and that is the class of bug this
//! is here to stop.
//!
//! A value xfx only hands on to a child -- the `HOME` a command runs with, the
//! names an MCP server is given -- is not read either ([`child_var_os`]), and
//! a child a unit test starts ([`isolate_child`]) inherits the fake process,
//! or nothing, in place of the real one: `PATH` and `TMPDIR` only, so a `git`
//! it runs finds no global configuration of the developer's. These do not
//! panic: nothing of the real process reaches the case or its child, and a
//! child that needed what it was not given fails on its own.
//!
//! None of this is in a build: `process_env!` is `std::env` there.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::env::VarError;
use std::ffi::{OsStr, OsString};
use std::process::Command;
use std::rc::Rc;

thread_local! {
    /// The fake process the case on this thread runs in, if any.
    static FAKE: RefCell<Option<Rc<FakeProcess>>> = const { RefCell::new(None) };
}

/// The variables a child of a unit test is still given from the real
/// process: where programs are, and where temporary files go. Neither names
/// the developer, a credential, or a configuration.
const CHILD_KEEPS: [&str; 2] = ["PATH", "TMPDIR"];

/// A process environment for a case to run in: a home and variables, all
/// the case's own.
#[derive(Clone, Default)]
pub(crate) struct FakeProcess {
    vars: BTreeMap<OsString, OsString>,
}

impl FakeProcess {
    /// A process with no home and no variables.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The same process with `HOME` set to `home`.
    pub(crate) fn home(self, home: impl AsRef<OsStr>) -> Self {
        self.var("HOME", home)
    }

    /// The same process with `name` set to `value`.
    pub(crate) fn var(mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.vars
            .insert(name.as_ref().to_owned(), value.as_ref().to_owned());
        self
    }

    /// Runs `body` with this as the process environment of this thread, and
    /// of every TUI thread it starts ([`carried`]).
    pub(crate) fn run<R>(self, body: impl FnOnce() -> R) -> R {
        let _scope = self.enter();
        body()
    }

    /// This, as the process of the current thread from now on -- for a
    /// thread that is the case's own: libtest runs each case on a thread of
    /// its own that ends with it, as a TUI runtime's thread ends with the
    /// runtime. `tui::worker`'s `Runtime::with_environment` makes its
    /// environment the process this way, so a runtime a case builds from an
    /// explicit environment reads that one and nothing else.
    ///
    /// Two edges: the last one made on a thread is the one in force, and one
    /// made inside a [`Self::run`] or [`Self::enter`] scope ends with it.
    pub(crate) fn for_this_thread(self) {
        drop(FAKE.with(|slot| slot.replace(Some(Rc::new(self)))));
    }

    /// [`Self::run`] for a body that is not a closure -- an `async` case on
    /// its own thread's runtime: this is the process until the scope drops.
    #[must_use = "the fake process ends when the scope is dropped"]
    pub(crate) fn enter(self) -> Scope {
        Scope::enter(Some(Rc::new(self)))
    }
}

/// Puts the fake process that was in place back, however the body ends.
pub(crate) struct Scope(Option<Rc<FakeProcess>>);

impl Scope {
    fn enter(fake: Option<Rc<FakeProcess>>) -> Self {
        Self(FAKE.with(|slot| slot.replace(fake)))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.0.take();
        FAKE.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// `body`, run on another thread in the fake process this one is in.
///
/// `tui::signals::spawn_unsignalled` -- the TUI's one way to start a thread --
/// wraps every body in this in a unit test, so a worker a case starts reads
/// what the case gave it, or panics as the case itself would.
pub(crate) fn carried<F, T>(body: F) -> impl FnOnce() -> T + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let fake: Option<FakeProcess> = FAKE.with(|slot| slot.borrow().as_deref().cloned());
    move || {
        let _scope = Scope::enter(fake.map(Rc::new));
        body()
    }
}

/// `std::env::var_os(name)`, as `who` would see it in a unit test.
pub(crate) fn var_os(who: &'static str, name: impl AsRef<OsStr>) -> Option<OsString> {
    let name = name.as_ref();
    let fake = FAKE.with(|slot| slot.borrow().clone());
    match fake {
        Some(fake) => fake.vars.get(name).cloned(),
        None => refuse(who, name),
    }
}

/// `std::env::var(name)`, as `who` would see it in a unit test.
pub(crate) fn var(who: &'static str, name: impl AsRef<OsStr>) -> Result<String, VarError> {
    match var_os(who, name) {
        Some(value) => value.into_string().map_err(VarError::NotUnicode),
        None => Err(VarError::NotPresent),
    }
}

/// A lookup `who` keeps and asks later, perhaps on a thread of its own:
/// [`var`], over the fake process the thread that made it was in.
///
/// Taken when it is made, because the threads that ask it (an MCP server's
/// reload and connection) are not started by the case.
pub(crate) fn lookup(who: &'static str) -> impl Fn(&str) -> Option<String> + Send + Sync + 'static {
    let fake: Option<FakeProcess> = FAKE.with(|slot| slot.borrow().as_deref().cloned());
    move |name: &str| match &fake {
        Some(fake) => fake
            .vars
            .get(OsStr::new(name))
            .and_then(|value| value.to_str())
            .map(str::to_string),
        None => refuse(who, OsStr::new(name)),
    }
}

/// `std::env::var_os(name)` for a value xfx only hands on to a child: the
/// fake process's, as [`isolate_child`] would give it, and otherwise `PATH`
/// and `TMPDIR` from the real process and nothing else.
pub(crate) fn child_var_os(name: impl AsRef<OsStr>) -> Option<OsString> {
    let name = name.as_ref();
    let fake = FAKE.with(|slot| slot.borrow().clone());
    match fake.and_then(|fake| fake.vars.get(name).cloned()) {
        Some(value) => Some(value),
        None if CHILD_KEEPS.iter().any(|kept| OsStr::new(kept) == name) => std::env::var_os(name),
        None => None,
    }
}

/// `std::env::var(name)`, as [`child_var_os`].
pub(crate) fn child_var(name: impl AsRef<OsStr>) -> Result<String, VarError> {
    match child_var_os(name) {
        Some(value) => value.into_string().map_err(VarError::NotUnicode),
        None => Err(VarError::NotPresent),
    }
}

/// The panic: the function and the variable's name, and what to do instead.
fn refuse(who: &str, name: &OsStr) -> ! {
    panic!(
        "{who} read the real process environment (`{}`) in a unit test. \
         Give the case its own: Runtime::with_environment, \
         RuntimeConfig::load_with or GitEnvironment::default over a temp home \
         -- or, to test {who} itself, crate::test_env::FakeProcess::run",
        name.to_string_lossy()
    )
}

/// Makes `command`'s child inherit the fake process this thread is in, or
/// nothing, instead of the real one: `PATH` and `TMPDIR` come from the real
/// process, unless the fake names them.
///
/// Called by `crate::process::command` before anything else is set, so what
/// that function removes ([`crate::config::CREDENTIAL_VARIABLES`]) and what a
/// caller sets afterwards apply to the fake as they would to the real one.
pub(crate) fn isolate_child(command: &mut Command) {
    command.env_clear();
    for name in CHILD_KEEPS {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if let Some(fake) = FAKE.with(|slot| slot.borrow().clone()) {
        command.envs(&fake.vars);
    }
}

mod tests;
