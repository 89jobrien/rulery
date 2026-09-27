//! Library support for parsing and orchestrating Rulery commands.

#![forbid(unsafe_code)]

mod args;
mod artifact;
mod canonical;
mod commands;
mod dispatch;
mod error;
mod exit;
mod facts;
mod findings;
mod output;
mod report;
mod runtime;
mod scaffold;

pub use args::{BasicFormat, Cli, Command, OutputFormat, RenderFormat};
pub use artifact::{
    ArtifactCondition, ArtifactExecution, ArtifactPorts, WarningConfidence, WarningFinding,
    run_artifact_command,
};
pub use commands::{CommandPorts, CompileResult, FormatResult, run_write_command};
pub use dispatch::{Format, Produced, run, run_with};
pub use error::{DiagnosticFailure, HostError};
pub use exit::ExitStatus;
pub use findings::Findings;
pub use output::CommandOutput;
pub use runtime::HostWorkflow;
