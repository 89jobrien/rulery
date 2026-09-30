//! Process port for specification conformance.

use std::path::Path;

/// Outcome of one process invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    /// The command exited successfully.
    Success,
    /// The command ran and exited non-zero, reporting this captured output.
    Failed(String),
}

/// Process operations required by conformance checks.
pub trait ProcessRunner {
    /// Runs rustfmt check for one scratch Rust source.
    ///
    /// # Errors
    ///
    /// Returns an error when rustfmt cannot be spawned or waited on; a completed process reports
    /// formatting through the Boolean result.
    fn rustfmt_check(&self, path: &Path) -> Result<bool, String>;

    /// Compiles one manifest, optionally with comma-separated features.
    ///
    /// A command that ran and failed is reported as [`ProcessOutcome::Failed`] carrying its
    /// captured output, so a gate can explain the failure instead of only stating it happened. An
    /// error is reserved for a command that could not be run at all, which is a different problem
    /// from a fixture that does not compile.
    ///
    /// # Errors
    ///
    /// Returns an error when cargo cannot be spawned or waited on.
    fn cargo_check(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String>;
}

/// Host process runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostProcessRunner;

impl ProcessRunner for HostProcessRunner {
    fn rustfmt_check(&self, path: &Path) -> Result<bool, String> {
        std::process::Command::new("rustfmt")
            .args(["--emit", "stdout"])
            .arg(path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| error.to_string())
    }

    fn cargo_check(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String> {
        let mut command = std::process::Command::new("cargo");
        command.args(["check", "--quiet", "--locked", "--manifest-path"]);
        command.arg(manifest);
        if !features.is_empty() {
            command.args(["--features", features]);
        }
        let output = command
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(ProcessOutcome::Success)
        } else {
            Ok(ProcessOutcome::Failed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }
}
