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

    /// Builds and runs one manifest, optionally with comma-separated features.
    ///
    /// Distinct from [`ProcessRunner::cargo_check`] because compiling a fixture proves only that its
    /// types resolve. A gate that must observe behavior needs the binary to have actually run, so
    /// a non-zero exit with no diagnostics is reported as a failure carrying the captured streams
    /// rather than being read as success.
    ///
    /// # Errors
    ///
    /// Returns an error when cargo cannot be spawned or waited on.
    fn cargo_run(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String>;

    /// Reports whether one manifest's workspace is rustfmt-clean.
    ///
    /// Needed because a fixture that is its own workspace is invisible to the parent
    /// `cargo fmt --all`, so without this a declared contract surface could drift out of the
    /// repository's formatting rules unnoticed.
    ///
    /// # Errors
    ///
    /// Returns an error when cargo cannot be spawned or waited on.
    fn cargo_fmt_check(&self, manifest: &Path) -> Result<bool, String>;
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
        Self::cargo(
            &["check", "--quiet", "--locked", "--manifest-path"],
            manifest,
            features,
        )
    }

    fn cargo_run(&self, manifest: &Path, features: &str) -> Result<ProcessOutcome, String> {
        Self::cargo(
            &["run", "--quiet", "--locked", "--manifest-path"],
            manifest,
            features,
        )
    }

    fn cargo_fmt_check(&self, manifest: &Path) -> Result<bool, String> {
        let mut command = std::process::Command::new("cargo");
        command.args(["fmt", "--quiet", "--check", "--manifest-path"]);
        command.arg(manifest);
        command
            .env("CARGO_TERM_COLOR", "never")
            .stdin(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| error.to_string())
    }
}

impl HostProcessRunner {
    /// Runs one cargo subcommand against `manifest`, reporting failure with both streams.
    ///
    /// Both streams are concatenated into the failure report because a fixture that asserts may
    /// write its diagnostics to either one, and a gate that showed only the wrong stream would
    /// report a contract break without saying which contract broke. `CARGO_TERM_COLOR=never` keeps
    /// the captured text free of escape sequences.
    ///
    /// An associated function rather than a method: the runner is a unit struct, so `self` carries
    /// nothing a caller could have varied.
    fn cargo(
        subcommand: &[&str],
        manifest: &Path,
        features: &str,
    ) -> Result<ProcessOutcome, String> {
        let mut command = std::process::Command::new("cargo");
        command.args(subcommand);
        command.arg(manifest);
        if !features.is_empty() {
            command.args(["--features", features]);
        }
        let output = command
            .env("CARGO_TERM_COLOR", "never")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(ProcessOutcome::Success)
        } else {
            let mut report = String::from_utf8_lossy(&output.stderr).into_owned();
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                report.push_str(&stdout);
            }
            Ok(ProcessOutcome::Failed(report))
        }
    }
}
