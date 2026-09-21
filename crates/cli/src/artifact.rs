//! Read-only artifact command orchestration.

use crate::{BasicFormat, Command, CommandOutput, ExitStatus, OutputFormat, RenderFormat};

/// Semantic command outcome before stream handling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactCondition {
    /// Successful artifact.
    Success,
    /// Diagnostics, conflict, invalid evidence, or projection loss.
    DiagnosticsError,
    /// Scenario assertion failure.
    ScenarioFailure,
    /// Analysis budget exhaustion.
    Inconclusive,
}

/// Warning confidence for deny-warnings promotion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarningConfidence {
    /// Sound proof.
    Proven,
    /// Replayable witness.
    Witnessed,
    /// Heuristic only.
    Heuristic,
    /// Inconclusive.
    Inconclusive,
    /// Advice rather than warning.
    Advice,
}

/// Warning promotion input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WarningFinding {
    /// Whether suppression removed the warning.
    pub suppressed: bool,
    /// Evidence confidence.
    pub confidence: WarningConfidence,
}

/// Fully rendered artifact execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactExecution {
    /// Exactly one artifact, except documented scenario arrays.
    pub artifact: Vec<u8>,
    /// Human-mode diagnostic text.
    pub human_stderr: Vec<u8>,
    /// Semantic result.
    pub condition: ArtifactCondition,
    /// Warnings available for optional promotion.
    pub warnings: Vec<WarningFinding>,
}

/// Application and stream ports for read-only artifact commands.
pub trait ArtifactPorts {
    /// Executes and renders one command without writing project files.
    ///
    /// # Errors
    ///
    /// Returns a message when execution fails before an artifact is available.
    fn execute(&self, command: &Command) -> Result<ArtifactExecution, String>;
    /// Writes the complete stdout artifact.
    ///
    /// # Errors
    ///
    /// Returns a message when the output stream rejects the artifact.
    fn write_stdout(&self, bytes: &[u8]) -> Result<(), String>;
}

/// Runs test, explain, analyze, diff, or render with exact output/exit policy.
#[must_use]
pub fn run_artifact_command<P: ArtifactPorts>(command: &Command, ports: &P) -> CommandOutput {
    let execution = match ports.execute(command) {
        Ok(execution) => execution,
        Err(error) => {
            return CommandOutput {
                stdout: Vec::new(),
                stderr: format!("{error}\n").into_bytes(),
                status: ExitStatus::IoFailure,
            };
        }
    };
    let promoted_warning = deny_warnings(command)
        && execution.warnings.iter().any(|warning| {
            !warning.suppressed
                && matches!(
                    warning.confidence,
                    WarningConfidence::Proven | WarningConfidence::Witnessed
                )
        });
    let mut status = if promoted_warning {
        ExitStatus::DiagnosticsError
    } else {
        match execution.condition {
            ArtifactCondition::Success => ExitStatus::Success,
            ArtifactCondition::DiagnosticsError => ExitStatus::DiagnosticsError,
            ArtifactCondition::ScenarioFailure => ExitStatus::ScenarioFailure,
            ArtifactCondition::Inconclusive => ExitStatus::AnalysisInconclusive,
        }
    };
    if ports.write_stdout(&execution.artifact).is_err() && status == ExitStatus::Success {
        status = ExitStatus::IoFailure;
    }
    CommandOutput {
        stdout: execution.artifact,
        stderr: if is_machine(command) {
            Vec::new()
        } else {
            execution.human_stderr
        },
        status,
    }
}

fn is_machine(command: &Command) -> bool {
    match command {
        Command::Analyze { format, .. } | Command::Check { format, .. } => {
            *format != OutputFormat::Human
        }
        Command::Test { format, .. }
        | Command::Explain { format, .. }
        | Command::Diff { format, .. } => *format != BasicFormat::Human,
        Command::Render { format, .. } => *format == RenderFormat::Json,
        Command::Init { .. } | Command::Fmt { .. } | Command::Lock { .. } => false,
    }
}

fn deny_warnings(command: &Command) -> bool {
    match command {
        Command::Check { deny_warnings, .. }
        | Command::Analyze { deny_warnings, .. }
        | Command::Test { deny_warnings, .. }
        | Command::Explain { deny_warnings, .. }
        | Command::Diff { deny_warnings, .. }
        | Command::Render { deny_warnings, .. } => *deny_warnings,
        Command::Init { .. } | Command::Fmt { .. } | Command::Lock { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    use rulery::contracts::DecisionId;

    use super::*;

    #[test]
    fn artifact_commands_match_output_and_exit_matrix() {
        let test_command = Command::Test {
            path: PathBuf::from("."),
            frozen: false,
            deny_warnings: false,
            filter: Some("smoke".to_owned()),
            format: BasicFormat::Json,
        };
        let ports = FakePorts::new(
            ArtifactCondition::ScenarioFailure,
            b"[{\"schema\":\"rulery.scenario-result/v1\"}]".to_vec(),
        );
        let test = run_artifact_command(&test_command, &ports);
        assert_eq!(test.status, ExitStatus::ScenarioFailure);
        assert!(test.stdout.starts_with(b"[") && test.stdout.ends_with(b"]"));
        assert!(test.stderr.is_empty());
        assert!(ports.seen.borrow()[0].contains("smoke"));

        let explain = Command::Explain {
            path: PathBuf::from("."),
            decision: DecisionId::new("decision.main").expect("decision"),
            facts: PathBuf::from("facts.json"),
            at: Some("2026-09-18T00:00:00Z".to_owned()),
            frozen: false,
            deny_warnings: false,
            format: BasicFormat::Human,
        };
        assert_eq!(
            run_artifact_command(
                &explain,
                &FakePorts::new(ArtifactCondition::DiagnosticsError, b"conflict".to_vec())
            )
            .status,
            ExitStatus::DiagnosticsError
        );

        let analyze = Command::Analyze {
            path: PathBuf::from("."),
            frozen: false,
            deny_warnings: false,
            max_states: 0,
            max_witnesses: 1_000,
            format: OutputFormat::Json,
        };
        assert_eq!(
            run_artifact_command(
                &analyze,
                &FakePorts::new(ArtifactCondition::Inconclusive, b"{}".to_vec())
            )
            .status,
            ExitStatus::AnalysisInconclusive
        );
        let diff = Command::Diff {
            before: PathBuf::from("a"),
            after: PathBuf::from("b"),
            decision: None,
            max_states: 0,
            max_witnesses: 1_000,
            frozen: false,
            deny_warnings: false,
            format: BasicFormat::Human,
        };
        assert_eq!(
            run_artifact_command(
                &diff,
                &FakePorts::new(ArtifactCondition::Inconclusive, b"diff".to_vec())
            )
            .status,
            ExitStatus::AnalysisInconclusive
        );
        let render = Command::Render {
            path: PathBuf::from("."),
            format: RenderFormat::DecisionTable,
            decision: Some(DecisionId::new("decision.main").expect("decision")),
            frozen: false,
            deny_warnings: false,
        };
        assert_eq!(
            run_artifact_command(
                &render,
                &FakePorts::new(ArtifactCondition::DiagnosticsError, b"loss".to_vec())
            )
            .status,
            ExitStatus::DiagnosticsError
        );

        let broken = FakePorts::new(ArtifactCondition::Success, b"artifact".to_vec());
        broken.fail_stdout.set(true);
        assert_eq!(
            run_artifact_command(&render, &broken).status,
            ExitStatus::IoFailure
        );
        let prior = FakePorts::new(ArtifactCondition::DiagnosticsError, b"artifact".to_vec());
        prior.fail_stdout.set(true);
        assert_eq!(
            run_artifact_command(&render, &prior).status,
            ExitStatus::DiagnosticsError
        );

        let warning_command = Command::Analyze {
            path: PathBuf::from("."),
            frozen: false,
            deny_warnings: true,
            max_states: 100,
            max_witnesses: 100,
            format: OutputFormat::Human,
        };
        let warnings = FakePorts::new(ArtifactCondition::Success, b"analysis".to_vec());
        warnings.warnings.borrow_mut().extend([
            WarningFinding {
                suppressed: true,
                confidence: WarningConfidence::Proven,
            },
            WarningFinding {
                suppressed: false,
                confidence: WarningConfidence::Heuristic,
            },
            WarningFinding {
                suppressed: false,
                confidence: WarningConfidence::Witnessed,
            },
        ]);
        assert_eq!(
            run_artifact_command(&warning_command, &warnings).status,
            ExitStatus::DiagnosticsError
        );

        let fatal = FakePorts::new(ArtifactCondition::Success, Vec::new());
        fatal.fail_execute.set(true);
        let fatal_output = run_artifact_command(&analyze, &fatal);
        assert_eq!(fatal_output.status, ExitStatus::IoFailure);
        assert!(!fatal_output.stderr.is_empty());
    }

    struct FakePorts {
        condition: ArtifactCondition,
        artifact: Vec<u8>,
        warnings: RefCell<Vec<WarningFinding>>,
        fail_execute: Cell<bool>,
        fail_stdout: Cell<bool>,
        seen: RefCell<Vec<String>>,
    }
    impl FakePorts {
        fn new(condition: ArtifactCondition, artifact: Vec<u8>) -> Self {
            Self {
                condition,
                artifact,
                warnings: RefCell::new(Vec::new()),
                fail_execute: Cell::new(false),
                fail_stdout: Cell::new(false),
                seen: RefCell::new(Vec::new()),
            }
        }
    }
    impl ArtifactPorts for FakePorts {
        fn execute(&self, command: &Command) -> Result<ArtifactExecution, String> {
            self.seen.borrow_mut().push(format!("{command:?}"));
            if self.fail_execute.get() {
                return Err("pre-artifact failure".to_owned());
            }
            Ok(ArtifactExecution {
                artifact: self.artifact.clone(),
                human_stderr: b"diagnostic\n".to_vec(),
                condition: self.condition,
                warnings: self.warnings.borrow().clone(),
            })
        }
        fn write_stdout(&self, _: &[u8]) -> Result<(), String> {
            if self.fail_stdout.get() {
                Err("broken pipe".to_owned())
            } else {
                Ok(())
            }
        }
    }
}
