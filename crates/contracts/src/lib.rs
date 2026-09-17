//! Stable shared contracts for Rulery.

#![forbid(unsafe_code)]

mod ids;

pub use ids::{
    ActionId, DecisionId, EscalationId, FactRootId, FactSegment, FactSegmentError, PackageId,
    PredicateId, QualifiedRuleId, QualifiedRuleIdError, ReasonCode, RuleId, ScenarioId, SourceId,
    StableId, StableIdError, TypeId,
};
