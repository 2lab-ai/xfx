//! The `fxr` executable.
//!
//! This file parses arguments and maps an outcome to an exit code. All behavior
//! lives behind `app::run`, so there is no second place a command can be handled.

use std::io::Write;
use std::process::ExitCode;

use fxr::app;
use fxr::cli::Cli;

fn main() -> ExitCode {
    match app::run(Cli::from_process_args()) {
        Ok(code) => code,
        Err(err) => {
            // Written directly rather than through `eprintln!` so a closed
            // stderr cannot panic on the way out.
            let _ = writeln!(std::io::stderr(), "fxr: {err}");
            ExitCode::FAILURE
        }
    }
}
