//! Rulery command-line entry point.
//!
//! The process follows the normative output contract: exactly one artifact is written to stdout,
//! human-mode diagnostics are written to stderr, and the exit status is the first matching row of
//! the exit-condition matrix. A broken stdout is `IoFailure` unless a higher-priority exit was
//! already determined and fully emitted.

use std::io::{self, Write};
use std::process;

use clap::Parser;

use rulery_cli::{Cli, CommandOutput, ExitStatus, run};

/// Writes one complete stream, reporting failure instead of panicking on a closed pipe.
fn write_stream(target: &mut dyn Write, bytes: &[u8]) -> Result<(), String> {
    target
        .write_all(bytes)
        .and_then(|()| target.flush())
        .map_err(|error| error.to_string())
}

/// Writes the command artifact to stdout and human diagnostics to stderr.
fn emit(output: &CommandOutput) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    write_stream(&mut stdout, &output.stdout)?;
    let mut stderr = io::stderr().lock();
    write_stream(&mut stderr, &output.stderr)
}

fn main() {
    let cli = Cli::parse();
    let output = run(&cli.command);
    let emitted = emit(&output);
    // A broken stdout is only `IoFailure` when no higher-priority row already determined the exit
    // and fully emitted its artifact, so a write failure never overrides a decided status.
    let status = match (output.status, emitted) {
        (ExitStatus::Success, Err(_)) => ExitStatus::IoFailure,
        (status, _) => status,
    };
    process::exit(status.code());
}
