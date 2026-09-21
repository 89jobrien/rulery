//! Rulery command-line entry point.

use clap::Parser;

fn main() {
    let _ = rulery_cli::Cli::parse();
}
