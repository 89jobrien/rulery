//! Checked expression model for compiled rules.

use serde::{Deserialize, Serialize};

use rulery_contracts::{FactPath, Span, Value};

/// Checked condition expression.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    /// Constant truth value.
    Constant {
        /// Literal value.
        value: bool,
        /// Source span.
        span: Span,
    },
    /// Predicate expression.
    Predicate(Predicate),
    /// Logical conjunction.
    All {
        /// Child expressions.
        expressions: Vec<Expr>,
        /// Source span.
        span: Span,
    },
    /// Logical disjunction.
    Any {
        /// Child expressions.
        expressions: Vec<Expr>,
        /// Source span.
        span: Span,
    },
    /// Logical negation.
    Not {
        /// Child expression.
        expression: Box<Expr>,
        /// Source span.
        span: Span,
    },
}

impl Expr {
    /// Returns this expression source span.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Constant { span, .. }
            | Self::All { span, .. }
            | Self::Any { span, .. }
            | Self::Not { span, .. } => *span,
            Self::Predicate(predicate) => predicate.span,
        }
    }
}

/// Checked predicate expression.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predicate {
    /// Comparison or presence operator.
    pub operator: Operator,
    /// Left operand.
    pub left: ExprOperand,
    /// Optional right operand.
    pub right: Option<ExprOperand>,
    /// Source span.
    pub span: Span,
}

/// Expression operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum Operator {
    Exists,
    Missing,
    Equals,
    NotEquals,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
    IsOneOf,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
    Before,
    After,
    Between,
    OnOrBefore,
    OnOrAfter,
    IsTrue,
    IsFalse,
}

/// Checked expression operand.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ExprOperand {
    /// Fact path lookup.
    Fact(FactPath),
    /// Literal typed value.
    Literal(Value),
    /// Reserved runtime token.
    Reserved(ReservedOperand),
}

/// Reserved runtime operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservedOperand {
    /// Policy-local date.
    Today,
    /// Current instant.
    Now,
}
