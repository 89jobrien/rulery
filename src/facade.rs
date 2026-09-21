//! Application workflow composition facade.

use rulery_analysis::{AnalysisOptions, AnalysisReport};
use rulery_contracts::{CaseFacts, DecisionId, PackagePath, RulebookLock};
use rulery_engine::DecisionTrace;
use rulery_ir::CompiledPackage;
use rulery_scenarios::{CompiledScenario, ScenarioResult};
use thiserror::Error;

use crate::LockMode;

/// Compilation workflow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileWorkflowOutput {
    /// Compiled package when compilation succeeds.
    pub package: Option<CompiledPackage>,
    /// Compiled root scenarios.
    pub scenarios: Vec<CompiledScenario>,
    /// Deterministically ordered diagnostic messages.
    pub diagnostics: Vec<String>,
    /// Replacement lock proposed by update mode.
    pub proposed_lock: Option<RulebookLock>,
}

/// Evaluation port failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FacadeEvaluationError {
    /// Runtime conflict with boundary evidence.
    RuntimeConflict {
        /// Conflicting rule or outcome evidence.
        evidence: Vec<String>,
    },
    /// Other evaluation failure.
    Other(String),
}

/// Facade boundary error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum FacadeError {
    /// Workflow stage failed.
    #[error("facade workflow failed: {0}")]
    Workflow(String),
    /// Runtime conflict converted to RUL205 evidence.
    #[error("RUL205 runtime conflict")]
    RuntimeConflict {
        /// Stable diagnostic code.
        code: &'static str,
        /// Conflict evidence.
        evidence: Vec<String>,
    },
}

/// Ports consumed by the composition root.
#[allow(clippy::missing_errors_doc)]
pub trait FacadePorts: Send + Sync {
    /// Store load stage.
    fn load(&self, root: &PackagePath) -> Result<(), FacadeError>;
    /// Source parse stage.
    fn parse(&self) -> Result<(), FacadeError>;
    /// Recursive package assembly stage.
    fn assemble(&self, lock_mode: LockMode) -> Result<Option<RulebookLock>, FacadeError>;
    /// Global source-key remap stage.
    fn remap(&self) -> Result<(), FacadeError>;
    /// Policy compilation stage.
    fn compile(&self) -> Result<CompiledPackage, FacadeError>;
    /// Root scenario compilation stage.
    fn compile_scenarios(
        &self,
        package: &CompiledPackage,
    ) -> Result<Vec<CompiledScenario>, FacadeError>;
    /// Decision evaluation port.
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
    ) -> Result<DecisionTrace, FacadeEvaluationError>;
    /// Static analysis port.
    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport;
    /// Semantic diff port.
    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
    /// Scenario runner port.
    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Vec<ScenarioResult>;
}

/// Public facade workflow contract.
#[allow(clippy::missing_errors_doc)]
pub trait RuleryFacade: Send + Sync {
    /// Compiles one package workflow.
    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, FacadeError>;
    /// Evaluates one decision without reopening source files.
    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
    ) -> Result<DecisionTrace, FacadeError>;
    /// Analyzes a compiled package.
    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport;
    /// Diffs two compiled packages.
    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport;
    /// Runs compiled scenarios.
    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Vec<ScenarioResult>;
}

/// Default generic facade composition root.
#[derive(Clone, Debug)]
pub struct WorkflowFacade<P> {
    ports: P,
}

impl<P> WorkflowFacade<P> {
    /// Creates a facade from its ports.
    #[must_use]
    pub const fn new(ports: P) -> Self {
        Self { ports }
    }
}

impl<P: FacadePorts> RuleryFacade for WorkflowFacade<P> {
    fn compile_package(
        &self,
        root: &PackagePath,
        lock_mode: LockMode,
    ) -> Result<CompileWorkflowOutput, FacadeError> {
        self.ports.load(root)?;
        self.ports.parse()?;
        let proposed_lock = self.ports.assemble(lock_mode)?;
        self.ports.remap()?;
        let package = self.ports.compile()?;
        let scenarios = self.ports.compile_scenarios(&package)?;
        Ok(CompileWorkflowOutput {
            package: Some(package),
            scenarios,
            diagnostics: Vec::new(),
            proposed_lock,
        })
    }

    fn evaluate(
        &self,
        package: &CompiledPackage,
        decision: &DecisionId,
        facts: &CaseFacts,
    ) -> Result<DecisionTrace, FacadeError> {
        self.ports
            .evaluate(package, decision, facts)
            .map_err(|error| match error {
                FacadeEvaluationError::RuntimeConflict { evidence } => {
                    FacadeError::RuntimeConflict {
                        code: "RUL205",
                        evidence,
                    }
                }
                FacadeEvaluationError::Other(message) => FacadeError::Workflow(message),
            })
    }

    fn analyze(&self, package: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport {
        self.ports.analyze(package, options)
    }

    fn diff(
        &self,
        before: &CompiledPackage,
        after: &CompiledPackage,
        decision: Option<&DecisionId>,
        options: &AnalysisOptions,
    ) -> AnalysisReport {
        self.ports.diff(before, after, decision, options)
    }

    fn run_scenarios(
        &self,
        package: &CompiledPackage,
        scenarios: &[CompiledScenario],
    ) -> Vec<ScenarioResult> {
        self.ports.run_scenarios(package, scenarios)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rulery_analysis::{AnalysisCompleteness, AnalysisReportV1};
    use rulery_contracts::{ContentHash, LanguageVersion, PackageId, Version};
    use rulery_diagnostics::DiagnosticReport;
    use rulery_ir::{
        CompilationInput, CompiledPackageDraft, PackageIntegritySet, ResolvedVocabulary,
    };

    use super::*;

    #[test]
    fn facade_composes_exact_v01_workflow_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let facade = WorkflowFacade::new(RecordingPorts {
            events: events.clone(),
        });
        let root = PackagePath::new("/packages/main").expect("root");
        let output = facade
            .compile_package(&root, LockMode::Frozen)
            .expect("compile");
        assert!(output.package.is_some());
        assert_eq!(
            events.lock().expect("events").as_slice(),
            [
                "store",
                "parser",
                "assembly",
                "remap",
                "compiler",
                "scenarios"
            ]
        );

        let package = output.package.as_ref().expect("package");
        let before = events.lock().expect("events").len();
        let facts = CaseFacts::new(Default::default());
        let conflict = facade
            .evaluate(
                package,
                &DecisionId::new("decision.main").expect("decision"),
                &facts,
            )
            .expect_err("conflict");
        assert!(
            matches!(conflict, FacadeError::RuntimeConflict { code: "RUL205", ref evidence } if evidence == &["rule.a", "rule.b"])
        );
        let options = AnalysisOptions::default();
        let _ = facade.analyze(package, &options);
        let _ = facade.diff(package, package, None, &options);
        assert!(facade.run_scenarios(package, &[]).is_empty());
        assert_eq!(events.lock().expect("events").len(), before + 4);
        assert!(
            !events.lock().expect("events")[before..]
                .iter()
                .any(|event| *event == "store" || *event == "parser")
        );
    }

    struct RecordingPorts {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordingPorts {
        fn record(&self, event: &'static str) {
            self.events.lock().expect("events").push(event);
        }
    }

    impl FacadePorts for RecordingPorts {
        fn load(&self, _: &PackagePath) -> Result<(), FacadeError> {
            self.record("store");
            Ok(())
        }
        fn parse(&self) -> Result<(), FacadeError> {
            self.record("parser");
            Ok(())
        }
        fn assemble(&self, _: LockMode) -> Result<Option<RulebookLock>, FacadeError> {
            self.record("assembly");
            Ok(None)
        }
        fn remap(&self) -> Result<(), FacadeError> {
            self.record("remap");
            Ok(())
        }
        fn compile(&self) -> Result<CompiledPackage, FacadeError> {
            self.record("compiler");
            Ok(package())
        }
        fn compile_scenarios(
            &self,
            _: &CompiledPackage,
        ) -> Result<Vec<CompiledScenario>, FacadeError> {
            self.record("scenarios");
            Ok(Vec::new())
        }
        fn evaluate(
            &self,
            _: &CompiledPackage,
            _: &DecisionId,
            _: &CaseFacts,
        ) -> Result<DecisionTrace, FacadeEvaluationError> {
            self.record("evaluate");
            Err(FacadeEvaluationError::RuntimeConflict {
                evidence: vec!["rule.a".to_owned(), "rule.b".to_owned()],
            })
        }
        fn analyze(&self, _: &CompiledPackage, options: &AnalysisOptions) -> AnalysisReport {
            self.record("analyze");
            report(options.clone())
        }
        fn diff(
            &self,
            _: &CompiledPackage,
            _: &CompiledPackage,
            _: Option<&DecisionId>,
            options: &AnalysisOptions,
        ) -> AnalysisReport {
            self.record("diff");
            report(options.clone())
        }
        fn run_scenarios(
            &self,
            _: &CompiledPackage,
            _: &[CompiledScenario],
        ) -> Vec<ScenarioResult> {
            self.record("run-scenarios");
            Vec::new()
        }
    }

    fn package() -> CompiledPackage {
        CompiledPackage::new(
            CompiledPackageDraft {
                package_id: PackageId::new("pkg.main").expect("package"),
                package_version: Version::new("1.0.0").expect("version"),
                language_version: LanguageVersion::V1,
                compiler_identity: "compiler".to_owned(),
                decisions: Vec::new(),
                actions: Vec::new(),
                integrity: PackageIntegritySet::new(CompilationInput::new(
                    ContentHash::from_bytes([1; 32]),
                    ContentHash::from_bytes([2; 32]),
                    None,
                )),
            },
            Default::default(),
            ResolvedVocabulary::default(),
        )
        .expect("package")
    }

    fn report(options: AnalysisOptions) -> AnalysisReport {
        let diagnostics = DiagnosticReport::new(Vec::new(), "analysis", LanguageVersion::V1)
            .expect("diagnostics");
        AnalysisReport::new(AnalysisReportV1 {
            package_hash: ContentHash::from_bytes([1; 32]),
            options,
            completeness: AnalysisCompleteness::Complete,
            diagnostics: diagnostics.payload().clone(),
            reachability: Vec::new(),
            overlaps: Vec::new(),
            coverage: Vec::new(),
            witnesses: Vec::new(),
            semantic_diffs: Vec::new(),
        })
    }
}
