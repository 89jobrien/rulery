//! Static rulebook analysis for Rulery.

#![forbid(unsafe_code)]

mod budget;
mod coverage;
mod diff;
mod interaction;
mod partition;
mod reachability;
mod wire;
mod witness;

pub use budget::{AnalysisBudget, BudgetResult};
pub use coverage::{
    CellEvaluation, CoverageReport, RuleCoverage, UncoveredCase, UncoveredCategory,
    compute_coverage,
};
pub use diff::{
    DiffCell, OutcomeChange, OutcomeChangeKind, PolicyDiff, PolicyDiffer, StructuralChange,
};
pub use interaction::{
    AnalysisFinding, InteractionAnalysis, OverlapClassification, RuleAnalysisInput, RuleOverlap,
    analyze_interactions,
};
pub use partition::{
    AnalysisCompleteness, AnalysisOptions, DecisionPartition, FactPartitionValue, FiniteDomain,
    PartitionCell, PartitionDomainKind, PartitionSpec, build_partition,
};
pub use reachability::{ProofCertificate, ReachabilityStatus, RuleReachability};
pub use wire::{
    AnalysisReport, AnalysisReportEnvelope, AnalysisReportV1, DecisionCoverage, PolicyAnalyzer,
};
pub use witness::{WitnessCase, WitnessClaim, minimize_witness};
