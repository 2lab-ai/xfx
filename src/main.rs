//! The `xfx` executable.
//!
//! This file parses arguments, starts the async runtime, and maps an outcome to
//! an exit code. Every command's behavior lives behind `app::run`, so there is
//! no second place a command can be handled. The one branch that does not go
//! through it is the opt-in TUI, which is not a command: it owns the main
//! thread and blocks on the terminal, so it cannot be reached from inside the
//! runtime at all.

use std::io::Write;
use std::process::ExitCode;

use xfx::app;
use xfx::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::from_process_args();
    // The UI thread must be the process's main thread and must not be inside a
    // runtime: it blocks in `poll(2)` and owns the terminal. The runtime a turn
    // needs is built on the worker's own thread instead.
    #[cfg(unix)]
    if xfx::tui::should_run(&cli) {
        return xfx::tui::run_blocking(cli);
    }

    // A current-thread runtime: xfx drives one turn at a time and never spawns
    // work that outlives the command, so a worker pool would add threads
    // without adding throughput.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&format!("cannot start the async runtime: {err}")),
    };

    match runtime.block_on(app::run(cli)) {
        Ok(code) => code,
        Err(err) => fail(&format!("{err}")),
    }
}

/// Reports a startup or output failure and exits nonzero.
fn fail(message: &str) -> ExitCode {
    // Written directly rather than through `eprintln!` so a closed stderr
    // cannot panic on the way out.
    let _ = writeln!(std::io::stderr(), "xfx: {message}");
    ExitCode::FAILURE
}
