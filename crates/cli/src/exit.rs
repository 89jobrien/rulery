//! Stable CLI exit status priorities.

/// Stable process exit status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum ExitStatus {
    /// Command succeeded.
    Success = 0,
    /// Diagnostics contain errors.
    DiagnosticsError = 1,
    /// Invocation is invalid.
    InvalidInvocation = 2,
    /// Filesystem or stream I/O failed.
    IoFailure = 3,
    /// Internal invariant failed.
    InternalFailure = 4,
    /// One or more scenarios failed.
    ScenarioFailure = 5,
    /// Analysis or semantic diff was incomplete.
    AnalysisIncomplete = 6,
}

impl ExitStatus {
    /// Returns the process exit code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_the_specification_table() {
        for (status, code) in [
            (ExitStatus::Success, 0),
            (ExitStatus::DiagnosticsError, 1),
            (ExitStatus::InvalidInvocation, 2),
            (ExitStatus::IoFailure, 3),
            (ExitStatus::InternalFailure, 4),
            (ExitStatus::ScenarioFailure, 5),
            (ExitStatus::AnalysisIncomplete, 6),
        ] {
            assert_eq!(status.code(), code);
        }
    }
}
