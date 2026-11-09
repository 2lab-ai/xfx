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
//! - A runtime a case builds with `Runtime::with_environment` lays its
//!   environment over the fake process of its thread
//!   ([`FakeProcess::over_this_thread`]): what the environment holds wins,
//!   and the thread's fake process, if any, fills the rest. With none, an
//!   environment with no home and no variable is the empty process.
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

/// What a unit test still takes from the real process, by name: where
/// programs are, and where temporary files go. Neither names the developer, a
/// credential, or a configuration. The one place this module reads the real
/// process, and what a child of a unit test is given of it.
fn kept() -> [(&'static str, Option<OsString>); 2] {
    [
        ("PATH", env_exception!(var_os("PATH"))),
        ("TMPDIR", env_exception!(var_os("TMPDIR"))),
    ]
}

/// The real process's `PATH`: where a case finds `git` and the rest.
pub(crate) fn path() -> OsString {
    let [(_, path), _] = kept();
    path.unwrap_or_default()
}

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

    /// This, laid over the process of the current thread from now on -- for
    /// a thread that is the case's own: libtest runs each case on a thread of
    /// its own that ends with it, as a TUI runtime's thread ends with the
    /// runtime. `tui::worker`'s `Runtime::with_environment` lays its
    /// environment over the thread's process this way.
    ///
    /// **Precedence:** what this holds wins -- its `HOME` and each variable
    /// it sets; the fake process already on the thread fills the rest. So a
    /// runtime a case builds from an explicit environment reads that, and a
    /// worker launched in a fake process keeps what an `Environment` does not
    /// hold -- `XFX_GATEWAY_URL`, `GIT_CONFIG_*`, `ZDOTDIR`. With no fake
    /// process on the thread, this alone is the process.
    ///
    /// Two edges: the last one laid on a thread is the one in force, and one
    /// laid inside a [`Self::run`] or [`Self::enter`] scope ends with it.
    pub(crate) fn over_this_thread(self) {
        FAKE.with(|slot| {
            let merged = match slot.borrow().as_deref() {
                Some(under) => {
                    let mut vars = under.vars.clone();
                    vars.extend(self.vars);
                    FakeProcess { vars }
                }
                None => self,
            };
            drop(slot.replace(Some(Rc::new(merged))));
        });
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
        None => kept()
            .into_iter()
            .find(|(kept, _)| OsStr::new(kept) == name)
            .and_then(|(_, value)| value),
    }
}

/// `std::env::var(name)`, as [`child_var_os`].
pub(crate) fn child_var(name: impl AsRef<OsStr>) -> Result<String, VarError> {
    match child_var_os(name) {
        Some(value) => value.into_string().map_err(VarError::NotUnicode),
        None => Err(VarError::NotPresent),
    }
}

/// `environment` -- a command's, as `permission::authority` builds it -- with
/// `GIT_CONFIG_GLOBAL=/dev/null` when its `HOME` is empty or missing: a unit
/// test that gave no home of its own.
///
/// Such a command is handed `HOME=""`, and git then looks for `/.gitconfig`
/// and `/.config/git/config`. Isolation excludes the global configuration
/// instead of counting on those files not existing. A case that gives a home
/// on purpose keeps it, and git reads what that home holds. The system
/// configuration stays as a build reads it: it is the machine's, not the
/// person's. In name order, as the rest of the environment is.
pub(crate) fn without_a_global_git_configuration(
    mut environment: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let homeless = !environment
        .iter()
        .any(|(name, value)| name == "HOME" && !value.is_empty());
    if homeless
        && !environment
            .iter()
            .any(|(name, _)| name == "GIT_CONFIG_GLOBAL")
    {
        environment.push(("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string()));
        environment.sort();
    }
    environment
}

/// The proxies a client built under `crate::gateway::bearer_rules` takes in a
/// unit test: the case's fake process's, never the real process's.
///
/// reqwest reads `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY`
/// (either case) from the process when a client is built, and a proxy URL can
/// carry a user and password. So this starts from `no_proxy()` and adds the
/// fake's, by scheme as reqwest would, the upper-case name first: with no
/// fake proxy, a client goes direct. (The rule after this one -- no proxy at
/// all for anything but https -- still applies.)
///
/// One exception: a copy of the test binary that `crate::test_net` started
/// keeps reqwest's own reading, because proving that reading is what that
/// copy is for, and its environment is one its parent built from nothing.
pub(crate) fn proxies(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    if crate::test_net::server_in_this_copy().is_some() {
        return builder;
    }
    let mut builder = builder.no_proxy();
    let Some(fake) = FAKE.with(|slot| slot.borrow().clone()) else {
        return builder;
    };
    let named = |upper: &str, lower: &str| {
        [upper, lower]
            .into_iter()
            .find_map(|name| fake.vars.get(OsStr::new(name)))
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let exempt = named("NO_PROXY", "no_proxy")
        .as_deref()
        .and_then(reqwest::NoProxy::from_string);
    type Scheme = fn(String) -> reqwest::Result<reqwest::Proxy>;
    let schemes: [(&str, &str, Scheme); 3] = [
        ("HTTPS_PROXY", "https_proxy", reqwest::Proxy::https),
        ("HTTP_PROXY", "http_proxy", reqwest::Proxy::http),
        ("ALL_PROXY", "all_proxy", reqwest::Proxy::all),
    ];
    for (upper, lower, proxy) in schemes {
        if let Some(Ok(proxy)) = named(upper, lower).map(proxy) {
            builder = builder.proxy(proxy.no_proxy(exempt.clone()));
        }
    }
    builder
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
    for (name, value) in kept() {
        if let Some(value) = value {
            command.env(name, value);
        }
    }
    if let Some(fake) = FAKE.with(|slot| slot.borrow().clone()) {
        command.envs(&fake.vars);
    }
}

mod tests;
