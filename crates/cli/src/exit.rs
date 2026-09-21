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
    /// Analysis or semantic diff was inconclusive.
    AnalysisInconclusive = 6,
}

impl ExitStatus {
    /// Returns the process exit code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}
