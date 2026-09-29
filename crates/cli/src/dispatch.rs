//! Command dispatch and the normative exit-condition matrix.
//!
//! The matrix is applied as an ordered decision procedure rather than as a priority sort, because
//! the specification states the rule directly: the CLI returns the first matching row in table
//! order. Rows 1-3 are decided from the classified failure, so they are settled before any artifact
//! is produced. Row 5 then precedes rows 6-9, and rows 4, 5, and 8 all resolve to the same exit
//! code, so folding them into one condition cannot change any observed result.

use std::path::Path;

use rulery::analysis::AnalysisOptions;
use rulery::contracts::DecisionId;
use rulery::scenarios::ScenarioStatus;

use crate::runtime::{self, Compiled, HostWorkflow};
use crate::{
    ArtifactCondition, BasicFormat, Command, CommandOutput, DiagnosticFailure, ExitStatus,
    Findings, HostError, OutputFormat, RenderFormat, WarningConfidence, WarningFinding, report,
};

/// Output format selected for one invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Human report.
    Human,
    /// Strict JSON envelope, or the documented scenario-result array.
    Json,
    /// One SARIF 2.1.0 log.
    Sarif,
    /// One Markdown document.
    Markdown,
    /// Exactly one decision-table envelope.
    DecisionTable,
}

impl Format {
    /// Returns whether the format is a machine format that keeps stderr free of diagnostics.
    const fn is_machine(self) -> bool {
        !matches!(self, Self::Human)
    }
}

impl From<OutputFormat> for Format {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Human => Self::Human,
            OutputFormat::Json => Self::Json,
            OutputFormat::Sarif => Self::Sarif,
        }
    }
}

impl From<BasicFormat> for Format {
    fn from(format: BasicFormat) -> Self {
        match format {
            BasicFormat::Human => Self::Human,
            BasicFormat::Json => Self::Json,
        }
    }
}

impl From<RenderFormat> for Format {
    fn from(format: RenderFormat) -> Self {
        match format {
            RenderFormat::Markdown => Self::Markdown,
            RenderFormat::Json => Self::Json,
            RenderFormat::DecisionTable => Self::DecisionTable,
        }
    }
}

/// One command's complete output before the exit-condition matrix resolves it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Produced {
    /// Complete stdout artifact, empty when the failure precedes artifact production.
    pub artifact: Vec<u8>,
    /// Human-mode stderr text carrying diagnostics and progress.
    pub human: Vec<u8>,
    /// Semantic condition resolved by matrix rows 6, 7, and 9.
    pub condition: ArtifactCondition,
    /// Whether the command emitted an unsuppressed Error diagnostic (matrix rows 5 and 8).
    pub diagnostics_error: bool,
    /// Warnings available for `--deny-warnings` promotion (matrix row 5).
    pub warnings: Vec<WarningFinding>,
}

impl Produced {
    /// Builds an execution carrying exactly one artifact.
    fn new(artifact: Vec<u8>, findings: &Findings) -> Self {
        Self {
            artifact,
            human: report::human_lines(&findings.lines),
            condition: ArtifactCondition::Success,
            diagnostics_error: findings.has_error,
            warnings: findings.warnings.clone(),
        }
    }
}

/// Runs one parsed command through production ports and the exit-condition matrix.
#[must_use]
pub fn run(command: &Command) -> CommandOutput {
    run_with(command, &HostWorkflow::new())
}

/// Runs one parsed command against an explicit workflow port.
#[must_use]
pub fn run_with(command: &Command, workflow: &HostWorkflow) -> CommandOutput {
    match execute(workflow, command) {
        Ok(produced) => {
            let status = resolve(&produced, command);
            let stderr = if format_of(command).is_machine() {
                Vec::new()
            } else {
                produced.human.clone()
            };
            CommandOutput {
                stdout: produced.artifact,
                stderr,
                status,
            }
        }
        Err(error) => failure(command, &error),
    }
}

/// Resolves the exit status of one completed execution in specification table order.
///
/// Rows 1-3 are already excluded because a classified failure returned before this point. Row 5
/// ("any command emits an unsuppressed Error or a promotable Warning under `--deny-warnings`")
/// precedes the scenario and analysis rows, and rows 4, 5, and 8 all name exit 1.
fn resolve(produced: &Produced, command: &Command) -> ExitStatus {
    if produced.diagnostics_error || promoted(produced, command) {
        return ExitStatus::DiagnosticsError;
    }
    match produced.condition {
        ArtifactCondition::Success => ExitStatus::Success,
        ArtifactCondition::DiagnosticsError => ExitStatus::DiagnosticsError,
        ArtifactCondition::ScenarioFailure => ExitStatus::ScenarioFailure,
        ArtifactCondition::Inconclusive => ExitStatus::AnalysisIncomplete,
    }
}

/// Returns whether `--deny-warnings` promotes one eligible warning into a command-failing error.
fn promoted(produced: &Produced, command: &Command) -> bool {
    deny_warnings(command)
        && produced.warnings.iter().any(|warning| {
            !warning.suppressed
                && matches!(
                    warning.confidence,
                    WarningConfidence::Proven | WarningConfidence::Witnessed
                )
        })
}

/// Renders one classified failure that precedes its own artifact.
fn failure(command: &Command, error: &HostError) -> CommandOutput {
    match error {
        HostError::Invocation(message) => pre_artifact(ExitStatus::InvalidInvocation, message),
        HostError::Io(message) => pre_artifact(ExitStatus::IoFailure, message),
        HostError::Internal(message) => pre_artifact(ExitStatus::InternalFailure, message),
        HostError::Rejected(rejected) => rejected_command(command, rejected),
    }
}

/// Reports one pre-artifact failure on stderr, which every output format allows.
fn pre_artifact(status: ExitStatus, message: &str) -> CommandOutput {
    CommandOutput {
        stdout: Vec::new(),
        stderr: format!("{message}\n").into_bytes(),
        status,
    }
}

/// Renders a rejected command: a diagnostic report when one is constructible, else plain reasons.
fn rejected_command(command: &Command, failure: &DiagnosticFailure) -> CommandOutput {
    if failure.diagnostics.is_empty() {
        return pre_artifact(ExitStatus::DiagnosticsError, &failure.message());
    }
    let format = format_of(command);
    let findings = Findings::from_registry(
        &failure.diagnostics,
        &rulery::contracts::SourceMap::default(),
    );
    match diagnostic_artifact(&findings, format) {
        Ok(artifact) => CommandOutput {
            stdout: artifact,
            stderr: if format.is_machine() {
                Vec::new()
            } else {
                report::human_lines(&findings.lines)
            },
            status: ExitStatus::DiagnosticsError,
        },
        Err(_) => pre_artifact(ExitStatus::DiagnosticsError, &failure.message()),
    }
}

/// Projects one diagnostic set into the artifact a command emits when diagnostics come first.
fn diagnostic_artifact(findings: &Findings, format: Format) -> Result<Vec<u8>, HostError> {
    match format {
        Format::Json => report::diagnostic_json(&findings.diagnostics),
        Format::Sarif => report::sarif_log(findings),
        Format::Human | Format::Markdown | Format::DecisionTable => {
            Ok(report::human_lines(&findings.lines))
        }
    }
}

/// Returns the output format one command selects.
fn format_of(command: &Command) -> Format {
    match command {
        Command::Analyze { format, .. } | Command::Check { format, .. } => (*format).into(),
        Command::Test { format, .. }
        | Command::Explain { format, .. }
        | Command::Diff { format, .. } => (*format).into(),
        Command::Render { format, .. } => (*format).into(),
        Command::Init { .. } | Command::Fmt { .. } | Command::Lock { .. } => Format::Human,
    }
}

/// Returns whether one command promotes registry warnings into command-failing errors.
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

/// Executes one command and returns its artifact plus the conditions the matrix resolves.
pub(crate) fn execute(workflow: &HostWorkflow, command: &Command) -> Result<Produced, HostError> {
    match command {
        Command::Init { path } => init(workflow, path),
        Command::Fmt { path, check } => format(workflow, path, !check),
        Command::Check { path, format, .. } => {
            check(workflow, path, lock_of(command), (*format).into())
        }
        Command::Analyze {
            path,
            max_states,
            max_witnesses,
            at,
            format,
            ..
        } => analyze(
            workflow,
            path,
            lock_of(command),
            *max_states,
            *max_witnesses,
            at.as_deref(),
            (*format).into(),
        ),
        Command::Test {
            path,
            filter,
            format,
            ..
        } => test(
            workflow,
            path,
            lock_of(command),
            filter.as_deref(),
            (*format).into(),
        ),
        Command::Explain {
            path,
            decision,
            facts,
            at,
            format,
            ..
        } => explain(
            workflow,
            path,
            lock_of(command),
            decision,
            facts,
            at.as_deref(),
            (*format).into(),
        ),
        Command::Diff {
            before,
            after,
            decision,
            max_states,
            max_witnesses,
            at,
            format,
            ..
        } => diff(
            workflow,
            before,
            after,
            lock_of(command),
            decision.as_ref(),
            *max_states,
            *max_witnesses,
            at.as_deref(),
            (*format).into(),
        ),
        Command::Render {
            path,
            format,
            decision,
            ..
        } => render(
            workflow,
            path,
            lock_of(command),
            (*format).into(),
            decision.as_ref(),
        ),
        Command::Lock { path } => lock(workflow, path),
    }
}

/// Returns the assembly lock mode one command selects.
///
/// The decision lives on [`Command::lock_mode`] so the public accessor and the shipped behavior
/// cannot drift apart. `init` and `fmt` do not assemble, so they carry no mode and fall back to
/// update, which no path reaches.
fn lock_of(command: &Command) -> rulery::LockMode {
    command.lock_mode().unwrap_or(rulery::LockMode::Update)
}

/// Rejects a non-empty `init` destination, then writes the scaffolded package.
fn init(workflow: &HostWorkflow, path: &Path) -> Result<Produced, HostError> {
    if workflow.destination_non_empty(path)? {
        return Err(HostError::rejected(format!(
            "`{}` already exists and is not empty",
            path.display()
        )));
    }
    let manifest = workflow.scaffold(path)?;
    let findings = Findings::default();
    Ok(Produced::new(
        format!("{}\n", manifest.display()).into_bytes(),
        &findings,
    ))
}

/// Rewrites authored source into canonical form, or reports what canonical form would change.
fn format(workflow: &HostWorkflow, path: &Path, write: bool) -> Result<Produced, HostError> {
    let result = workflow.canonicalize_sources(path, write)?;
    // A single named file reports its canonical bytes; a package reports one line per rewrite.
    let artifact = if result.stdout.is_empty() {
        summary(&result).into_bytes()
    } else {
        result.stdout.clone()
    };
    let mut produced = Produced::new(artifact, &Findings::default());
    if !write && result.changed {
        produced.condition = ArtifactCondition::DiagnosticsError;
    }
    Ok(produced)
}

/// Renders one human line per affected path, or nothing when no file is affected.
fn summary(result: &crate::FormatResult) -> String {
    let mut text = String::new();
    for path in &result.paths {
        text.push_str(path);
        text.push('\n');
    }
    text
}

/// Validates one package and reports its diagnostics.
fn check(
    workflow: &HostWorkflow,
    path: &Path,
    mode: rulery::LockMode,
    format: Format,
) -> Result<Produced, HostError> {
    let compiled = workflow.compile(path, mode)?;
    let findings = Findings::from_registry(&compiled.diagnostics, &compiled.source_map);
    Ok(Produced::new(
        diagnostic_artifact(&findings, format)?,
        &findings,
    ))
}

/// Runs bounded static analysis over one package and reports it.
#[allow(clippy::too_many_arguments)]
fn analyze(
    workflow: &HostWorkflow,
    path: &Path,
    mode: rulery::LockMode,
    max_states: u64,
    max_witnesses: u32,
    at: Option<&str>,
    format: Format,
) -> Result<Produced, HostError> {
    let compiled = workflow.compile(path, mode)?;
    let findings = Findings::from_registry(&compiled.diagnostics, &compiled.source_map);
    if findings.has_error {
        return rejected_execution(&findings, format);
    }
    let at = workflow.resolve_analysis_instant(at)?;
    let package = compiled_package(&compiled)?;
    let analysis = workflow.analyze(&package, &options(max_states, max_witnesses), at)?;
    let reported = Findings::from_analysis(&analysis, &compiled.source_map);
    let artifact = match format {
        Format::Human => report::human_lines(&report::analysis_lines(&analysis, &reported)),
        Format::Json => report::analysis_json(&analysis)?,
        Format::Sarif => report::sarif_log(&reported)?,
        Format::Markdown | Format::DecisionTable => report::human_lines(&reported.lines),
    };
    let mut produced = Produced::new(artifact, &reported);
    produced.condition = if runtime::is_complete(&analysis) {
        ArtifactCondition::Success
    } else {
        ArtifactCondition::Inconclusive
    };
    Ok(produced)
}

/// Runs the root scenarios and reports one result per scenario.
fn test(
    workflow: &HostWorkflow,
    path: &Path,
    mode: rulery::LockMode,
    filter: Option<&str>,
    format: Format,
) -> Result<Produced, HostError> {
    let run = workflow.scenarios(path, mode, filter)?;
    let mut findings = Findings::from_reasons(&run.messages);
    let mut failed = false;
    for result in &run.results {
        match result.payload().status {
            ScenarioStatus::Passed => {}
            ScenarioStatus::Failed => failed = true,
            // An invalid or errored scenario never reached exact comparison, so it is a
            // command-failing diagnostic rather than the scenario-failure condition.
            ScenarioStatus::Invalid | ScenarioStatus::Error => findings.has_error = true,
        }
    }
    findings
        .lines
        .splice(0..0, report::scenario_lines(&run.results));
    let artifact = match format {
        Format::Json => report::scenario_results(&run.results)?,
        Format::Sarif => report::sarif_log(&findings)?,
        Format::Human | Format::Markdown | Format::DecisionTable => {
            report::human_lines(&findings.lines)
        }
    };
    let mut produced = Produced::new(artifact, &findings);
    if failed {
        produced.condition = ArtifactCondition::ScenarioFailure;
    }
    Ok(produced)
}

/// Evaluates one decision and reports the canonical explanation or trace.
#[allow(clippy::too_many_arguments)]
fn explain(
    workflow: &HostWorkflow,
    path: &Path,
    mode: rulery::LockMode,
    decision: &DecisionId,
    facts: &Path,
    at: Option<&str>,
    format: Format,
) -> Result<Produced, HostError> {
    let compiled = workflow.compile(path, mode)?;
    let findings = Findings::from_registry(&compiled.diagnostics, &compiled.source_map);
    if findings.has_error {
        return rejected_execution(&findings, format);
    }
    let package = compiled_package(&compiled)?;
    let at = workflow.evaluation_instant(at)?;
    let trace = workflow.explain(&package, decision, facts, at)?;
    let artifact = match format {
        Format::Human => report::human_explanation_text(&trace)?,
        Format::Json => report::trace_envelope(&trace)?,
        Format::Sarif => report::sarif_log(&findings)?,
        Format::Markdown | Format::DecisionTable => report::human_lines(&findings.lines),
    };
    Ok(Produced::new(artifact, &findings))
}

/// Compares two packages semantically and reports the diff.
#[allow(clippy::too_many_arguments)]
fn diff(
    workflow: &HostWorkflow,
    before: &Path,
    after: &Path,
    mode: rulery::LockMode,
    decision: Option<&DecisionId>,
    max_states: u64,
    max_witnesses: u32,
    at: Option<&str>,
    format: Format,
) -> Result<Produced, HostError> {
    let left = workflow.compile(before, mode)?;
    let right = workflow.compile(after, mode)?;
    let combined = left
        .diagnostics
        .iter()
        .chain(right.diagnostics.iter())
        .cloned()
        .collect::<Vec<_>>();
    let findings = Findings::from_registry(&combined, &right.source_map);
    if findings.has_error {
        return rejected_execution(&findings, format);
    }
    let at = workflow.resolve_analysis_instant(at)?;
    let left_package = compiled_package(&left)?;
    let right_package = compiled_package(&right)?;
    let comparison = workflow.diff(
        &left_package,
        &right_package,
        decision,
        &options(max_states, max_witnesses),
        at,
    )?;
    let reported = Findings::from_analysis(&comparison, &right.source_map);
    let artifact = match format {
        Format::Human => report::human_lines(&report::diff_lines(&comparison, &reported)),
        Format::Json => report::analysis_json(&comparison)?,
        Format::Sarif => report::sarif_log(&reported)?,
        Format::Markdown | Format::DecisionTable => report::human_lines(&reported.lines),
    };
    let mut produced = Produced::new(artifact, &reported);
    produced.condition = if runtime::is_complete(&comparison) {
        ArtifactCondition::Success
    } else {
        ArtifactCondition::Inconclusive
    };
    Ok(produced)
}

/// Renders one compiled package, Markdown document, or decision table.
fn render(
    workflow: &HostWorkflow,
    path: &Path,
    mode: rulery::LockMode,
    format: Format,
    decision: Option<&DecisionId>,
) -> Result<Produced, HostError> {
    let compiled = workflow.compile(path, mode)?;
    let findings = Findings::from_registry(&compiled.diagnostics, &compiled.source_map);
    if findings.has_error {
        return rejected_execution(&findings, format);
    }
    let package = compiled_package(&compiled)?;
    let artifact = match format {
        Format::Markdown => report::markdown_document(&package, decision)?,
        Format::Json => report::package_envelope(&package, decision)?,
        Format::DecisionTable => decision_table(&package, decision)?,
        Format::Human | Format::Sarif => report::human_lines(&findings.lines),
    };
    Ok(Produced::new(artifact, &findings))
}

/// Lowers one decision to exactly one lossless decision table.
fn decision_table(
    package: &rulery::ir::CompiledPackage,
    decision: Option<&DecisionId>,
) -> Result<Vec<u8>, HostError> {
    let Some(decision) = decision else {
        return Err(HostError::Invocation(
            "`--format decision-table` requires `--decision`".to_owned(),
        ));
    };
    let losses = report::projection_losses(package, decision)?;
    if !losses.is_empty() {
        // Both projection-loss codes require evidence a renderer does not hold, so the reason is
        // reported as text and the command fails as an unsupported projection.
        return Err(HostError::rejected(losses.join("; ")));
    }
    report::decision_table(package, decision)
}

/// Resolves, validates, and atomically writes the complete transitive lock.
fn lock(workflow: &HostWorkflow, path: &Path) -> Result<Produced, HostError> {
    let compiled = workflow.compile(path, rulery::LockMode::Update)?;
    let findings = Findings::from_registry(&compiled.diagnostics, &compiled.source_map);
    if findings.has_error {
        return rejected_execution(&findings, Format::Human);
    }
    let Some(replacement) = compiled.proposed_lock else {
        return Err(HostError::Internal(
            "update mode resolved a package but proposed no lock".to_owned(),
        ));
    };
    let written = workflow.write_lock(path, &replacement)?;
    let findings = Findings::default();
    Ok(Produced::new(
        format!("{}\n", written.display()).into_bytes(),
        &findings,
    ))
}

/// Builds the analysis options one invocation requests.
fn options(max_states: u64, max_witnesses: u32) -> AnalysisOptions {
    AnalysisOptions {
        max_states,
        max_witnesses,
        ..AnalysisOptions::default()
    }
}

/// Returns the compiled package a command requires, or reports a missing one as internal.
fn compiled_package(compiled: &Compiled) -> Result<rulery::ir::CompiledPackage, HostError> {
    compiled.package.clone().ok_or_else(|| {
        HostError::Internal("compilation reported no error but produced no package".to_owned())
    })
}

/// Returns the artifact a command emits when its own diagnostics precede the requested one.
fn rejected_execution(findings: &Findings, format: Format) -> Result<Produced, HostError> {
    let mut produced = Produced::new(diagnostic_artifact(findings, format)?, findings);
    produced.diagnostics_error = true;
    Ok(produced)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Builds one command pair exercising the same conditions under human and JSON output.
    fn commands(deny_warnings: bool) -> (Command, Command) {
        (
            Command::Test {
                path: PathBuf::from("."),
                frozen: false,
                deny_warnings,
                filter: None,
                format: crate::BasicFormat::Human,
            },
            Command::Test {
                path: PathBuf::from("."),
                frozen: false,
                deny_warnings,
                filter: None,
                format: crate::BasicFormat::Json,
            },
        )
    }

    /// Builds one execution carrying the supplied semantic conditions.
    fn produced(condition: ArtifactCondition, promoted: bool) -> Produced {
        Produced {
            artifact: b"artifact".to_vec(),
            human: b"diagnostics\n".to_vec(),
            condition,
            diagnostics_error: false,
            warnings: vec![WarningFinding {
                suppressed: false,
                confidence: if promoted {
                    WarningConfidence::Proven
                } else {
                    WarningConfidence::Heuristic
                },
            }],
        }
    }

    #[test]
    fn exit_matrix_resolves_the_first_matching_row() {
        for (condition, promoted, expected) in [
            (ArtifactCondition::Success, false, ExitStatus::Success),
            (
                ArtifactCondition::ScenarioFailure,
                false,
                ExitStatus::ScenarioFailure,
            ),
            (
                ArtifactCondition::Inconclusive,
                false,
                ExitStatus::AnalysisIncomplete,
            ),
            (
                ArtifactCondition::DiagnosticsError,
                false,
                ExitStatus::DiagnosticsError,
            ),
        ] {
            let (human, json) = commands(false);
            assert_eq!(resolve(&produced(condition, promoted), &human), expected);
            assert_eq!(resolve(&produced(condition, promoted), &json), expected);
        }
    }

    #[test]
    fn deny_warnings_promotes_only_unsuppressed_proven_or_witnessed_warnings() {
        let (human, _) = commands(true);
        assert_eq!(
            resolve(&produced(ArtifactCondition::Success, true), &human),
            ExitStatus::DiagnosticsError
        );
        assert_eq!(
            resolve(&produced(ArtifactCondition::Success, false), &human),
            ExitStatus::Success
        );

        let (ignored, _) = commands(false);
        assert_eq!(
            resolve(&produced(ArtifactCondition::Success, true), &ignored),
            ExitStatus::Success
        );
    }

    #[test]
    fn unsuppressed_error_outranks_scenario_and_analysis_conditions() {
        for condition in [
            ArtifactCondition::ScenarioFailure,
            ArtifactCondition::Inconclusive,
        ] {
            let (human, json) = commands(false);
            let mut execution = produced(condition, false);
            execution.diagnostics_error = true;
            assert_eq!(resolve(&execution, &human), ExitStatus::DiagnosticsError);
            assert_eq!(resolve(&execution, &json), ExitStatus::DiagnosticsError);
        }
    }

    #[test]
    fn machine_formats_drop_human_diagnostics_and_invalid_commands_stay_unavailable() {
        let (human, json) = commands(false);
        let execution = produced(ArtifactCondition::Success, false);
        assert_eq!(format_of(&human), Format::Human);
        assert_eq!(format_of(&json), Format::Json);
        assert!(!Format::Human.is_machine());
        assert!(Format::Json.is_machine());
        assert!(!execution.artifact.is_empty());
    }
}
