//! Exact v0.1 diagnostic registry.

use std::sync::LazyLock;

use crate::{
    DiagnosticCode, DiagnosticFamily, DiagnosticProducer, EvidenceRequirement, Severity,
    Suppressibility,
};

/// Metadata defining one known diagnostic code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticDefinition {
    code: DiagnosticCode,
    title: &'static str,
    family: DiagnosticFamily,
    severity: Severity,
    producer: DiagnosticProducer,
    suppressibility: Suppressibility,
    evidence: EvidenceRequirement,
}

impl DiagnosticDefinition {
    /// Returns the diagnostic code.
    #[must_use]
    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }
    /// Returns the default severity.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }
    /// Returns the required evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRequirement {
        &self.evidence
    }
    /// Returns the canonical registry title.
    #[must_use]
    pub(crate) const fn title_value(&self) -> &'static str {
        self.title
    }
    /// Returns the canonical producing subsystem.
    #[must_use]
    pub(crate) const fn producer_value(&self) -> DiagnosticProducer {
        self.producer
    }
    /// Returns the canonical suppression policy.
    #[must_use]
    pub(crate) const fn suppressibility_value(&self) -> Suppressibility {
        self.suppressibility
    }
    pub(crate) const fn family_value(&self) -> DiagnosticFamily {
        self.family
    }
}

#[derive(Clone, Copy)]
struct Raw(
    &'static str,
    &'static str,
    DiagnosticFamily,
    Severity,
    DiagnosticProducer,
    Suppressibility,
    EvidenceTag,
);

#[derive(Clone, Copy)]
enum EvidenceTag {
    None,
    Provenance,
    Witness,
    Trace,
    BehaviorChange,
    AnalysisLimit,
    Proof,
    Conflict,
    Properties,
}

use DiagnosticFamily as F;
use DiagnosticProducer as P;
use EvidenceTag as E;
use Severity as S;
use Suppressibility as U;

const RAW: &[Raw] = &[
    Raw(
        "RUL001",
        "Invalid source syntax",
        F::Syntax,
        S::Error,
        P::Parser,
        U::Never,
        E::None,
    ),
    Raw(
        "RUL002",
        "Duplicate declaration",
        F::Syntax,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL003",
        "Unknown symbol",
        F::Syntax,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL004",
        "Invalid fact path",
        F::Syntax,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL005",
        "Type mismatch",
        F::Syntax,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL006",
        "Source resource limit exceeded",
        F::Syntax,
        S::Error,
        P::Parser,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL100",
        "Undefined operational term",
        F::Clarity,
        S::Warning,
        P::Compiler,
        U::WithJustificationAndExpiry,
        E::Provenance,
    ),
    Raw(
        "RUL101",
        "Temporal term undefined",
        F::Clarity,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL102",
        "Missing outcome reason",
        F::Clarity,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL150",
        "Unknown-fact policy unhandled",
        F::Semantics,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL151",
        "Default outcome missing",
        F::Semantics,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL152",
        "Precedence is ambiguous",
        F::Semantics,
        S::Error,
        P::Compiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL153",
        "Timezone unavailable",
        F::Semantics,
        S::Error,
        P::Evaluator,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL200",
        "Conflicting rules",
        F::RuleInteraction,
        S::Error,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL201",
        "Shadowed approval",
        F::RuleInteraction,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL202",
        "Unreachable rule",
        F::RuleInteraction,
        S::Warning,
        P::Analyzer,
        U::WithJustification,
        E::Proof,
    ),
    Raw(
        "RUL203",
        "Redundant rule",
        F::RuleInteraction,
        S::Warning,
        P::Analyzer,
        U::WithJustification,
        E::Witness,
    ),
    Raw(
        "RUL204",
        "Undeclared override",
        F::RuleInteraction,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL205",
        "Runtime conflict",
        F::RuleInteraction,
        S::Error,
        P::Evaluator,
        U::Never,
        E::Conflict,
    ),
    Raw(
        "RUL250",
        "Uncovered combination",
        F::Coverage,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL251",
        "Unhandled enum variant",
        F::Coverage,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL252",
        "Unhandled missing fact",
        F::Coverage,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL253",
        "Temporal boundary uncovered",
        F::Coverage,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::Witness,
    ),
    Raw(
        "RUL254",
        "Analysis inconclusive",
        F::Coverage,
        S::Warning,
        P::Analyzer,
        U::Never,
        E::AnalysisLimit,
    ),
    Raw(
        "RUL300",
        "Scenario failed",
        F::Scenario,
        S::Error,
        P::ScenarioRunner,
        U::Never,
        E::Trace,
    ),
    Raw(
        "RUL301",
        "Scenario uses undeclared fact",
        F::Scenario,
        S::Error,
        P::ScenarioCompiler,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL302",
        "Scenario is stale",
        F::Scenario,
        S::Warning,
        P::ScenarioCompiler,
        U::WithJustificationAndExpiry,
        E::Provenance,
    ),
    Raw(
        "RUL350",
        "Outcome changed",
        F::Change,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::BehaviorChange,
    ),
    Raw(
        "RUL351",
        "More restrictive change",
        F::Change,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::BehaviorChange,
    ),
    Raw(
        "RUL352",
        "More permissive change",
        F::Change,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::BehaviorChange,
    ),
    Raw(
        "RUL353",
        "Default changed",
        F::Change,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::BehaviorChange,
    ),
    Raw(
        "RUL354",
        "Precedence changed",
        F::Change,
        S::Warning,
        P::Analyzer,
        U::WithJustificationAndExpiry,
        E::BehaviorChange,
    ),
    Raw(
        "RUL400",
        "Lossy target export",
        F::Export,
        S::Error,
        P::Renderer,
        U::Never,
        E::BehaviorChange,
    ),
    Raw(
        "RUL401",
        "Unsupported target outcome",
        F::Export,
        S::Error,
        P::Renderer,
        U::Never,
        E::Provenance,
    ),
    Raw(
        "RUL500",
        "Lockfile out of date",
        F::Evidence,
        S::Error,
        P::Store,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL501",
        "Import hash changed",
        F::Evidence,
        S::Error,
        P::Store,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL502",
        "Required lockfile missing",
        F::Evidence,
        S::Error,
        P::Store,
        U::Never,
        E::None,
    ),
    Raw(
        "RUL503",
        "Import cycle",
        F::Evidence,
        S::Error,
        P::Facade,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL504",
        "Import resource limit exceeded",
        F::Evidence,
        S::Error,
        P::Facade,
        U::Never,
        E::Properties,
    ),
    Raw(
        "RUL900",
        "Internal invariant violated",
        F::Internal,
        S::Error,
        P::Facade,
        U::Never,
        E::Provenance,
    ),
];

static DEFINITIONS: LazyLock<Vec<DiagnosticDefinition>> = LazyLock::new(|| {
    RAW.iter()
        .map(|raw| DiagnosticDefinition {
            code: DiagnosticCode::known(raw.0),
            title: raw.1,
            family: raw.2,
            severity: raw.3,
            producer: raw.4,
            suppressibility: raw.5,
            evidence: match raw.6 {
                E::None => EvidenceRequirement::None,
                E::Provenance => EvidenceRequirement::Provenance,
                E::Witness => EvidenceRequirement::Witness,
                E::Trace => EvidenceRequirement::Trace,
                E::BehaviorChange => EvidenceRequirement::BehaviorChange,
                E::AnalysisLimit => EvidenceRequirement::AnalysisLimit,
                E::Proof => EvidenceRequirement::Proof,
                E::Conflict => EvidenceRequirement::Conflict,
                E::Properties => EvidenceRequirement::Properties,
            },
        })
        .collect()
});

/// Returns the complete v0.1 registry.
#[must_use]
pub fn diagnostic_definitions() -> &'static [DiagnosticDefinition] {
    &DEFINITIONS
}

/// Finds the definition of a known code.
#[must_use]
pub fn diagnostic_definition(code: &DiagnosticCode) -> Option<&'static DiagnosticDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.code == *code)
}
