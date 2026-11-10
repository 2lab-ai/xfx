//! `env_exception!`: the one way to read the real process environment
//! outside `process_env!`, in a build and in a test alike.
//!
//! Alone in its file so that `scripts/check-env-reads.py --self-test` can
//! compile this very definition into its fixtures.

/// A read of the real process environment, by a name written as a literal:
/// `env_exception!(var("COLORTERM"))`, `env_exception!(var_os("PATH"))`, or
/// the first of several that is set and not empty,
/// `env_exception!(first_set_os(["TMPDIR", "TEMP", "TMP"]))`; the standard
/// library's temporary directory, which reads `TMPDIR`,
/// `env_exception!(temp_dir())`; or the names
/// alone, never a value, `env_exception!(names())`, for the one place that
/// must know nothing else is set (`tests/support/isolated.rs`).
///
/// Each expands to the call itself, in the one arm of a `match ()` that
/// carries the allow (`clippy.toml` refuses every other read), so a build
/// does what the plain call does. Names are literals, never a constant or a
/// variable: what an invocation reads is written where it is, and
/// `scripts/check-env-reads.py` holds every invocation to its file and its
/// names, and every name to a list of what may be read and why.
#[macro_export]
#[doc(hidden)]
macro_rules! env_exception {
    (var($name:literal)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: env_exception! -- a name scripts/check-env-reads.py lists"
            )]
            () => ::std::env::var($name),
        }
    };
    (var_os($name:literal)) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: env_exception! -- a name scripts/check-env-reads.py lists"
            )]
            () => ::std::env::var_os($name),
        }
    };
    (temp_dir()) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: env_exception! -- TMPDIR, through the standard library"
            )]
            () => ::std::env::temp_dir(),
        }
    };
    (names()) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: env_exception! -- the names alone, never a value"
            )]
            () => ::std::env::vars_os().map(|(name, _)| name),
        }
    };
    (first_set_os([$($name:literal),+ $(,)?])) => {
        match () {
            #[allow(
                clippy::disallowed_methods,
                reason = "env: env_exception! -- names scripts/check-env-reads.py lists"
            )]
            () => [$($name),+]
                .iter()
                .find_map(|name| ::std::env::var_os(name).filter(|value| !value.is_empty())),
        }
    };
}
