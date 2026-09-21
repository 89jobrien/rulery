//! Deterministic decision evaluation engine for Rulery.

#![forbid(unsafe_code)]

mod evaluator;
mod precedence;
mod predicate;
mod relevance;
mod time;
mod trace;
mod truth;

pub use evaluator::{
    EvaluationError, EvidenceClass, InvalidFactStrategy, MissingFactStrategy, PredicateObservation,
    StrategyApplication, StrategyCandidate, StrategyOutcome, StrategyReason, apply_strategies,
};
pub use precedence::{
    Candidate, ConflictTrace, ExplicitRanks, PrecedenceError, PrecedenceModel, Selection,
    SemanticPrecedenceKey, SupersededCandidate, select_candidates,
};
pub use predicate::{
    BinaryPredicate, FactEvidence, OperandLookup, OperandState, PresencePredicate, evaluate_binary,
    evaluate_presence, lookup_operand,
};
pub use relevance::{
    ClassifiedObservation, DecisionRelevance, UnresolvedCandidate, classify_unresolved,
    relevant_observations, should_use_default,
};
pub use time::{
    Clock, ClockValue, DateExpiryPolicy, FixedClock, JiffTimeZoneDatabase, SystemClock,
    TimeZoneDatabase, TimeZoneError, evaluate_before, evaluate_expired, evaluate_on_or_after,
    evaluate_unexpired,
};
pub use trace::{
    CandidateTrace, ClockOperand, DecisionConflictTrace, DecisionTrace, DecisionTraceDraft,
    DecisionTraceEnvelope, DecisionTraceV1, EvaluatedOperand, ExpressionTrace, InvalidFactTrace,
    PredicateTrace, RuleTrace, ShortCircuitReason, StrategyApplicationTrace, StrategyKind,
    SupersededRuleTrace, SupersessionReason, TraceDetail, TraceError, evaluation_id,
};
pub use truth::Truth;
