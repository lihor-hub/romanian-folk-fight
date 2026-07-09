//! Root `cargo xtask` dispatcher: focused Rust verification gates for
//! romanian-folk-fight. See `xtask/README.md` for the artifact-directory
//! convention and the pattern for adding new command groups.

mod commands;
mod process;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match commands::dispatch(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(commands::DispatchError::Usage(message)) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        Err(commands::DispatchError::Step(failure)) => {
            eprintln!("\ncargo xtask: stopped at first failure -> {failure}");
            ExitCode::FAILURE
        }
    }
}
