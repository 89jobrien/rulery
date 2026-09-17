//! Stable shared contracts for Rulery.

#![forbid(unsafe_code)]

mod ids;
mod source;

pub use ids::{
    ActionId, DecisionId, EscalationId, FactRootId, FactSegment, FactSegmentError, PackageId,
    PredicateId, QualifiedRuleId, QualifiedRuleIdError, ReasonCode, RuleId, ScenarioId, SourceId,
    StableId, StableIdError, TypeId,
};
pub use source::{
    BoundaryError, FactPath, LanguageVersion, NormalizedSourceLocation, PackagePath, SourcePath,
    UnresolvedFactPath, Version, VersionRequirement,
};
