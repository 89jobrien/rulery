//! Production command execution over the real workspace adapters.
//!
//! Every I/O boundary the CLI owns is reached through the same composition the library facade
//! uses: [`FilesystemPackageStore`] for local reads and the atomic lock write, [`YamlSourceParser`]
//! inside [`PackageAssembler`], [`PolicyCompiler`] for the compiled package, and
//! [`ProductionApplication`] for evaluation, analysis, semantic diff, and scenario execution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rulery::analysis::{AnalysisCompleteness, AnalysisOptions, AnalysisReport};
use rulery::compiler::{PolicyCompiler, SourceCompilationInput};
use rulery::contracts::{
    ContentHash, DecisionId, PackagePath, RulebookLock, RulebookLockEnvelope, SourceMap, UtcInstant,
};
use rulery::diagnostics::Diagnostic;
use rulery::engine::{Clock, DecisionTrace, SystemClock};
use rulery::ir::CompiledPackage;
use rulery::scenarios::{CompiledScenario, ScenarioResult};
use rulery::store::{FilesystemPackageStore, PackageStore, StoreError};
use rulery::syntax::YamlSourceParser;
use rulery::{
    ApplicationError, ApplicationService, AssemblyError, LockMode, PackageAssembler,
    PackageAssemblyService, ProductionApplication,
};

use crate::findings::build_registry;
use crate::{
    ArtifactExecution, ArtifactPorts, Command, CommandPorts, CompileResult, DiagnosticFailure,
    FormatResult, HostError,
};

/// Lock file name written by `lock`, matching the store adapter's own layout.
const LOCK_FILE_NAME: &str = "rulery.lock";

/// Producer identity recorded on CLI-authored diagnostic properties.
const CLI_PRODUCER: &str = "rulery-cli";

/// One compiled package plus the diagnostics and lock its assembly produced.
#[derive(Clone, Debug)]
pub struct Compiled {
    /// Compiled package when no error diagnostic rejected the sources.
    pub package: Option<CompiledPackage>,
    /// Constructible registry diagnostics in canonical report order.
    pub diagnostics: Vec<Diagnostic>,
    /// Replacement lock proposed by update mode.
    pub proposed_lock: Option<RulebookLock>,
    /// Source catalog retained for SARIF location resolution.
    pub source_map: SourceMap,
}

/// One completed scenario run.
#[derive(Clone, Debug)]
pub struct ScenarioRun {
    /// One result per executed scenario.
    pub results: Vec<ScenarioResult>,
    /// Messages the scenario compiler reported alongside the compiled package.
    pub messages: Vec<String>,
}

/// Production CLI workflow composed from the real workspace adapters.
#[derive(Clone, Debug, Default)]
pub struct HostWorkflow {
    application: ProductionApplication,
    store: FilesystemPackageStore,
}

impl HostWorkflow {
    /// Creates the production workflow with default resource and time-zone limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Assembles and compiles one package under an explicit lock policy.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when a required read or local path resolution fails and
    /// [`HostError::Rejected`] when assembly, parsing, compilation, or lock policy rejects the
    /// package.
    pub fn compile(&self, path: &Path, mode: LockMode) -> Result<Compiled, HostError> {
        let root = package_path(path)?;
        let assembly = PackageAssembler::new(self.store.clone(), YamlSourceParser)
            .assemble(&root, mode)
            .map_err(|error| assembly_error(&error))?;
        let source_map = assembly.input.source_map.clone();
        let lock_hash = self.lock_hash(&root)?;
        let root_hash = ContentHash::from_bytes(*assembly.input.integrity.root.as_bytes());
        let output = PolicyCompiler.compile_source(&SourceCompilationInput {
            source_bundle_hash: root_hash,
            root: assembly.input.root,
            imports: assembly.input.imports,
            source_map: assembly.input.source_map,
            lock_hash,
        });
        Ok(Compiled {
            diagnostics: build_registry(&output.diagnostics, root_hash)?,
            package: output.package,
            proposed_lock: assembly.proposed_lock,
            source_map,
        })
    }

    /// Returns the canonical lock hash used as compiler integrity input.
    fn lock_hash(&self, root: &PackagePath) -> Result<Option<ContentHash>, HostError> {
        let Some(lock) = self
            .store
            .load_lock(root)
            .map_err(|error| HostError::Io(error.to_string()))?
        else {
            return Ok(None);
        };
        let encoded = serde_json::to_vec(&RulebookLockEnvelope::from(lock))
            .map_err(|error| HostError::Internal(error.to_string()))?;
        Ok(Some(ContentHash::digest(&encoded)))
    }

    /// Compiles the root scenarios and executes the selected subset.
    ///
    /// Scenario bridging from authored sources into the typed scenario inputs lives behind the
    /// application boundary, so this step is the one place the CLI calls the whole compile
    /// workflow. When that workflow rejects the package, the compiler is re-run once to recover the
    /// registry diagnostics the application boundary flattened into plain messages.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Rejected`] when the package or its scenarios do not compile and
    /// [`HostError::Internal`] when a scenario cannot be evaluated.
    pub fn scenarios(
        &self,
        path: &Path,
        mode: LockMode,
        filter: Option<&str>,
    ) -> Result<ScenarioRun, HostError> {
        let root = package_path(path)?;
        let output = self
            .application
            .compile_package(&root, mode)
            .map_err(|error| self.workflow_error(path, mode, error))?;
        let Some(package) = output.package else {
            return Err(HostError::rejected(
                "scenario workflow completed without a compiled package",
            ));
        };
        let selected = match filter {
            None => output.scenarios,
            Some(needle) => output
                .scenarios
                .iter()
                .filter(|scenario| scenario_matches(scenario, needle))
                .cloned()
                .collect::<Vec<CompiledScenario>>(),
        };
        let results = self
            .application
            .run_scenarios(&package, &selected)
            .map_err(|error| HostError::Internal(error.to_string()))?;
        Ok(ScenarioRun {
            results,
            messages: output.diagnostics,
        })
    }

    /// Evaluates one decision of one compiled package at an explicit instant.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Invocation`] when the decision is unknown or the facts are rejected,
    /// and [`HostError::Internal`] when evaluation fails for any other reason.
    pub fn explain(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &Path,
        at: UtcInstant,
    ) -> Result<DecisionTrace, HostError> {
        let typed = crate::facts::read_case_facts(facts, package.vocabulary())?;
        self.application
            .evaluate_at(package, decision, &typed, at)
            .map_err(|error| evaluation_error(&error))
    }

    /// Runs bounded static analysis over one compiled package.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Internal`] when the analyzer cannot be constructed for the package.
    pub fn analyze(
        &self,
        package: &CompiledPackage,
        options: &AnalysisOptions,
    ) -> Result<AnalysisReport, HostError> {
        self.application
            .analyze(package, options)
            .map_err(|error| HostError::Internal(error.to_string()))
    }

    /// Compares two compiled packages semantically.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Internal`] when the analyzer cannot be constructed for the packages.
    pub fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> Result<AnalysisReport, HostError> {
        self.application
            .diff(before, after, decision, options)
            .map_err(|error| HostError::Internal(error.to_string()))
    }

    /// Atomically writes a validated replacement lock and returns the written path.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when the temporary write, sync, or rename fails.
    pub fn write_lock(&self, path: &Path, lock: &RulebookLock) -> Result<PathBuf, HostError> {
        let root = package_path(path)?;
        self.application
            .write_lock(&root, lock)
            .map_err(|error| HostError::Io(error.to_string()))?;
        Ok(path.join(LOCK_FILE_NAME))
    }

    /// Returns whether a destination already exists and is non-empty.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when destination metadata cannot be read.
    pub fn destination_non_empty(&self, path: &Path) -> Result<bool, HostError> {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(HostError::Io(format!("{}: {error}", path.display()))),
        };
        Ok(entries.flatten().next().is_some())
    }

    /// Writes the scaffolded package into an absent or empty destination.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when a directory or file cannot be created.
    pub fn scaffold(&self, path: &Path) -> Result<PathBuf, HostError> {
        std::fs::create_dir_all(path)
            .map_err(|error| HostError::Io(format!("{}: {error}", path.display())))?;
        for name in crate::scaffold::ROOT_DIRS {
            std::fs::create_dir_all(path.join(name)).map_err(|error| {
                HostError::Io(format!("{}: {error}", path.join(name).display()))
            })?;
        }
        for (name, bytes) in crate::scaffold::scaffold_files(path) {
            let target = path.join(name);
            std::fs::write(&target, bytes.as_bytes())
                .map_err(|error| HostError::Io(format!("{}: {error}", target.display())))?;
        }
        Ok(path.join(crate::scaffold::ROOT_FILES[0]))
    }

    /// Returns the authored source files one package root declares, in ascending path order.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when a directory cannot be read.
    pub fn source_files(&self, path: &Path) -> Result<Vec<PathBuf>, HostError> {
        let mut files: Vec<PathBuf> = crate::scaffold::ROOT_FILES
            .iter()
            .map(|name| path.join(name))
            .collect();
        for name in ["rules", "scenarios"] {
            let directory = path.join(name);
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(HostError::Io(format!("{}: {error}", directory.display())));
                }
            };
            for entry in entries.flatten() {
                let candidate = entry.path();
                if candidate
                    .extension()
                    .is_some_and(|extension| extension == "yaml")
                {
                    files.push(candidate);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    /// Rewrites authored source into canonical form, or reports what would change.
    ///
    /// Rewrites authored source into canonical form, or reports what canonical form would change.
    ///
    /// A single named file is reported rather than rewritten: its canonical bytes become the
    /// result's stdout and the file is left untouched. A package is rewritten in place.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Io`] when a source file cannot be read or written, and
    /// [`HostError::Rejected`] when a source file is not a YAML document.
    pub fn canonicalize_sources(
        &self,
        path: &Path,
        write: bool,
    ) -> Result<crate::FormatResult, HostError> {
        let mut changed = Vec::new();
        if path.is_file() {
            let canonical = Self::canonicalize_file(path)?;
            let current = std::fs::read(path)
                .map_err(|error| HostError::Io(format!("{}: {error}", path.display())))?;
            if current != canonical.as_bytes() {
                changed.push(path.display().to_string());
            }
            return Ok(crate::FormatResult {
                changed: !changed.is_empty(),
                paths: changed,
                stdout: if write {
                    canonical.into_bytes()
                } else {
                    Vec::new()
                },
            });
        }
        for file in self.source_files(path)? {
            let canonical = Self::canonicalize_file(&file)?;
            let current = std::fs::read(&file)
                .map_err(|error| HostError::Io(format!("{}: {error}", file.display())))?;
            if current == canonical.as_bytes() {
                continue;
            }
            if write {
                Self::write_source(&file, canonical.as_bytes())?;
            }
            changed.push(file.display().to_string());
        }
        Ok(crate::FormatResult {
            changed: !changed.is_empty(),
            paths: changed,
            stdout: Vec::new(),
        })
    }

    /// Renders one source file in canonical form.
    fn canonicalize_file(file: &Path) -> Result<String, HostError> {
        let source = std::fs::read_to_string(file)
            .map_err(|error| HostError::Io(format!("{}: {error}", file.display())))?;
        crate::canonical::canonicalize(&source)
            .map_err(|error| HostError::rejected(format!("{}: {error}", file.display())))
    }

    /// Writes one source file through a temporary file and a rename.
    fn write_source(file: &Path, bytes: &[u8]) -> Result<(), HostError> {
        let temporary = file.with_extension("yaml.tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|error| HostError::Io(format!("{}: {error}", temporary.display())))?;
        std::fs::rename(&temporary, file)
            .map_err(|error| HostError::Io(format!("{}: {error}", file.display())))
    }

    /// Classifies a whole-package workflow failure, recovering registry diagnostics when possible.
    fn workflow_error(&self, path: &Path, mode: LockMode, error: ApplicationError) -> HostError {
        match error {
            ApplicationError::Compilation { .. } => self.compile(path, mode).map_or_else(
                |fallback| fallback,
                |compiled| {
                    if compiled.diagnostics.is_empty() {
                        HostError::rejected("the package did not compile")
                    } else {
                        HostError::rejected_with(compiled.diagnostics)
                    }
                },
            ),
            other => classify_application(&other),
        }
    }

    /// Returns the instant `explain` evaluates at, reading the system UTC clock when absent.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Invocation`] when a supplied value is not RFC 3339 and
    /// [`HostError::Internal`] when the system clock cannot be represented.
    pub fn evaluation_instant(&self, at: Option<&str>) -> Result<UtcInstant, HostError> {
        match at {
            Some(value) => UtcInstant::parse_rfc3339(value)
                .map_err(|error| HostError::Invocation(error.to_string())),
            None => SystemClock
                .now()
                .map_err(|error| HostError::Internal(error.to_string())),
        }
    }
}

/// Returns whether one compiled scenario matches an identity or tag filter.
fn scenario_matches(scenario: &CompiledScenario, needle: &str) -> bool {
    scenario.id.as_str() == needle || scenario.tags.iter().any(|tag| tag.as_str() == needle)
}

/// Returns whether an analysis or diff report enumerated every requested domain completely.
#[must_use]
pub fn is_complete(report: &AnalysisReport) -> bool {
    matches!(
        report.payload().completeness,
        AnalysisCompleteness::Complete
    )
}

/// Classifies a package-graph assembly failure against the exit-condition matrix.
fn assembly_error(error: &AssemblyError) -> HostError {
    match error {
        // A source resource bound is a package rejection, not a failed read.
        AssemblyError::Store(StoreError::ResourceLimitExceeded { .. })
        | AssemblyError::Parse(_)
        | AssemblyError::Boundary(_)
        | AssemblyError::ImportCycle { .. }
        | AssemblyError::PackageIdMismatch { .. }
        | AssemblyError::VersionMismatch { .. }
        | AssemblyError::PackageConflict { .. }
        | AssemblyError::ResourceLimit { .. }
        | AssemblyError::LockfileMissing
        | AssemblyError::LockfileOutOfDate
        | AssemblyError::ImportHashChanged
        | AssemblyError::LockConstruction
        | AssemblyError::PackageLocationOutsideRoot { .. }
        | AssemblyError::NonUtf8PackageLocation { .. } => graph_error(error),
        AssemblyError::SourceKeyOverflow
        | AssemblyError::MissingRoot(_)
        | AssemblyError::UnmappedSourceKey { .. } => HostError::Internal(error.to_string()),
        AssemblyError::Store(_) => HostError::Io(error.to_string()),
    }
}

/// Classifies an import-graph or lock-policy failure the registry names a code for.
fn graph_error(error: &AssemblyError) -> HostError {
    let reason = error.to_string();
    match error.diagnostic_code() {
        Some(code) => match crate::findings::policy_diagnostic(
            code,
            reason.clone(),
            graph_properties(&reason),
        ) {
            Ok(diagnostic) => {
                HostError::Rejected(DiagnosticFailure::from_diagnostics(vec![diagnostic]))
            }
            Err(_) => HostError::rejected(reason),
        },
        None => HostError::rejected(reason),
    }
}

/// Returns the properties evidence an import-graph or lock-policy diagnostic records.
fn graph_properties(reason: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("producer".to_owned(), CLI_PRODUCER.to_owned()),
        ("reason".to_owned(), reason.to_owned()),
    ])
}

/// Classifies one application-boundary failure.
fn classify_application(error: &ApplicationError) -> HostError {
    match error {
        ApplicationError::InvalidInvocation(message) => HostError::Invocation(message.clone()),
        ApplicationError::Assembly(error) => assembly_error(error),
        ApplicationError::Compilation { diagnostics } => HostError::Rejected(DiagnosticFailure {
            diagnostics: Vec::new(),
            reasons: diagnostics.clone(),
        }),
        ApplicationError::Scenario { message, .. } => HostError::rejected(message.clone()),
        ApplicationError::InvalidFacts => {
            HostError::rejected("case facts are invalid for the compiled package")
        }
        ApplicationError::RuntimeConflict { evidence } => HostError::rejected(evidence.join("; ")),
        ApplicationError::Evaluation(message) => evaluation_rejection(message),
        ApplicationError::Io(message) => HostError::Io(message.clone()),
        ApplicationError::Internal(message) => HostError::Internal(message.clone()),
    }
}

/// Classifies a decision evaluation failure.
fn evaluation_error(error: &ApplicationError) -> HostError {
    match error {
        ApplicationError::InvalidFacts => {
            HostError::rejected("case facts are invalid for the compiled package")
        }
        ApplicationError::Evaluation(message) => evaluation_rejection(message),
        other => classify_application(other),
    }
}

/// Reports an unknown decision identity as an invalid argument value.
fn evaluation_rejection(message: &str) -> HostError {
    if message.contains("unknown decision") {
        HostError::Invocation(message.to_owned())
    } else {
        HostError::rejected(message.to_owned())
    }
}

/// Converts a local path into a validated package root.
///
/// # Errors
///
/// Returns [`HostError::Invocation`] when the path is not a normalized package path.
pub fn package_path(path: &Path) -> Result<PackagePath, HostError> {
    PackagePath::new(path.to_path_buf()).map_err(|error| HostError::Invocation(error.to_string()))
}

impl ArtifactPorts for HostWorkflow {
    fn execute(&self, command: &Command) -> Result<ArtifactExecution, String> {
        crate::dispatch::execute(self, command).map_or_else(
            |error| Err(error.to_string()),
            |produced| {
                Ok(ArtifactExecution {
                    artifact: produced.artifact,
                    human_stderr: produced.human,
                    condition: produced.condition,
                    warnings: produced.warnings,
                })
            },
        )
    }

    fn write_stdout(&self, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;

        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(bytes)
            .and_then(|()| stdout.flush())
            .map_err(|error| error.to_string())
    }
}

impl CommandPorts for HostWorkflow {
    fn destination_non_empty(&self, path: &Path) -> Result<bool, String> {
        Self::destination_non_empty(self, path).map_err(|error| error.to_string())
    }

    fn init(&self, path: &Path) -> Result<(), String> {
        Self::scaffold(self, path)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn format(&self, path: &Path, write: bool) -> Result<FormatResult, String> {
        Self::canonicalize_sources(self, path, write).map_err(|error| error.to_string())
    }

    fn compile(&self, path: &Path, mode: LockMode) -> Result<CompileResult, String> {
        let compiled = Self::compile(self, path, mode).map_err(|error| error.to_string())?;
        Ok(CompileResult {
            valid: compiled.package.is_some(),
            diagnostics: Vec::new(),
            proposed_lock: compiled.proposed_lock,
        })
    }

    fn write_lock(&self, path: &Path, lock: &RulebookLock) -> Result<(), String> {
        Self::write_lock(self, path, lock)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
