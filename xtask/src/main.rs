//! Binary entry point for Rulery repository automation.

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    match xtask::execute(&xtask::Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
