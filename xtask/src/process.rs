//! Process port for specification conformance.

use std::path::Path;

/// Process operations required by conformance checks.
pub trait ProcessRunner {
    /// Runs rustfmt check for one scratch Rust source.
    ///
    /// # Errors
    ///
    /// Returns an error when rustfmt cannot be spawned or waited on; a completed process reports
    /// formatting through the Boolean result.
    fn rustfmt_check(&self, path: &Path) -> Result<bool, String>;
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
}
