//! Library support for parsing and orchestrating Rulery commands.

#![forbid(unsafe_code)]

mod args;
mod artifact;
mod commands;
mod exit;
mod output;

pub use args::{BasicFormat, Cli, Command, OutputFormat, RenderFormat};
pub use artifact::{
    ArtifactCondition, ArtifactExecution, ArtifactPorts, WarningConfidence, WarningFinding,
    run_artifact_command,
};
pub use commands::{CommandPorts, CompileResult, FormatResult, run_write_command};
pub use exit::ExitStatus;
pub use output::CommandOutput;
