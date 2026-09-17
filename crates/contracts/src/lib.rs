//! Stable shared contracts for Rulery.

#![forbid(unsafe_code)]

mod ids;
mod outcome;
mod scalar;
mod source;
mod time;
mod value;

pub use ids::{
    ActionId, DecisionId, EscalationId, FactRootId, FactSegment, FactSegmentError, PackageId,
    PredicateId, QualifiedRuleId, QualifiedRuleIdError, ReasonCode, RuleId, ScenarioId, SourceId,
    StableId, StableIdError, TypeId,
};
pub use outcome::{
    Action, ActionInvocation, ActionParameter, Outcome, OutcomeError, OutcomeKind, OutcomeTemplate,
    Reason, Reasons, RequiredFacts,
};
pub use scalar::{DecimalValue, DecimalValueError};
pub use source::{
    BoundaryError, FactPath, LanguageVersion, LoadedSourceBundle, NormalizedSourceLocation,
    PackagePath, SourceBundle, SourceDocument, SourceFile, SourceIntegrity, SourceKey, SourceMap,
    SourcePath, Span, UnresolvedFactPath, Version, VersionRequirement,
};
pub use time::{
    Clock, DateExpiryPolicy, DurationValue, PolicyDate, PolicyTimeZone, TimeSemantics,
    TimeValueError, TimeZoneDatabase, TimeZoneDatabaseIdentity, TimeZoneError, UtcInstant,
};
pub use value::{CaseFacts, EnumValue, FactState, FactValidationError, Value, ValueKind};
