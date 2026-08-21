//! The `fxr` executable.
//!
//! This file parses arguments, starts the async runtime, and maps an outcome to
//! an exit code. All behavior lives behind `app::run`, so there is no second
//! place a command can be handled.

use std::io::Write;
use std::process::ExitCode;

use fxr::app;
use fxr::cli::Cli;

fn main() -> ExitCode {
    // A current-thread runtime: fxr drives one turn at a time and never spawns
    // work that outlives the command, so a worker pool would add threads
    // without adding throughput.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return fail(&format!("cannot start the async runtime: {err}")),
    };

    match runtime.block_on(app::run(Cli::from_process_args())) {
        Ok(code) => code,
        Err(err) => fail(&format!("{err}")),
    }
}

/// Reports a startup or output failure and exits nonzero.
fn fail(message: &str) -> ExitCode {
    // Written directly rather than through `eprintln!` so a closed stderr
    // cannot panic on the way out.
    let _ = writeln!(std::io::stderr(), "fxr: {message}");
    ExitCode::FAILURE
}
