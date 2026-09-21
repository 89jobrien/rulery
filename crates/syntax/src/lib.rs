//! Authored source syntax and parsing for Rulery.

#![forbid(unsafe_code)]

mod ast;
mod yaml;

pub use ast::{
    ParsedPackage, SourceAction, SourceCondition, SourceDecision, SourceEffect,
    SourceExpectedDecision, SourceImport, SourceMetadata, SourceOperand, SourceOperator,
    SourcePackage, SourceParseError, SourceParseErrorKind, SourceParser, SourcePredicate,
    SourceRule, SourceScenario, SourceSemantics, SourceVocabulary,
};
pub use yaml::YamlSourceParser;
