//! Command output ownership.

use crate::ExitStatus;

/// Complete command output without direct stream ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    /// Standard output bytes.
    pub stdout: Vec<u8>,
    /// Standard error bytes.
    pub stderr: Vec<u8>,
    /// Stable exit status.
    pub status: ExitStatus,
}

impl CommandOutput {
    pub(crate) fn new(
        status: ExitStatus,
        stdout: impl Into<Vec<u8>>,
        stderr: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: stderr.into(),
            status,
        }
    }
}
