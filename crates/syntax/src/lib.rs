//! Format-neutral authored syntax and strict YAML parsing for Rulery.
//!
//! Source AST nodes preserve caller-visible spans and authored distinctions without resolving
//! vocabulary or symbols. [`YamlSourceParser`] rejects aliases, duplicate/unknown fields, tags,
//! merge keys, and ambiguous condition shapes before producing [`ParsedPackage`].

#![forbid(unsafe_code)]

mod ast;
mod yaml;

pub use ast::{
    ParsedPackage, SourceAction, SourceCondition, SourceDecision, SourceEffect,
    SourceExpectedDecision, SourceImport, SourceMetadata, SourceOperand, SourceOperator,
    SourceOutcome, SourcePackage, SourceParseError, SourceParseErrorKind, SourceParser,
    SourcePrecedence, SourcePredicate, SourceReason, SourceRoot, SourceRule, SourceScenario,
    SourceSemantics, SourceStrategy, SourceType, SourceValue, SourceVocabulary,
};
pub use yaml::YamlSourceParser;
