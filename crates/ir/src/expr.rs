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
pub enum Operator {
    /// Tests whether the left fact is present.
    Exists,
    /// Tests whether the left fact is absent.
    Missing,
    /// Tests type-strict equality with the right operand.
    Equals,
    /// Negates type-strict equality with the right operand.
    NotEquals,
    /// Tests strict ascending order.
    LessThan,
    /// Tests ascending order including equality.
    LessOrEqual,
    /// Tests strict descending order.
    GreaterThan,
    /// Tests descending order including equality.
    GreaterOrEqual,
    /// Tests membership using the checked right operand.
    IsOneOf,
    /// Tests text substring or typed list-element containment.
    Contains,
    /// Tests a text prefix.
    StartsWith,
    /// Tests a text suffix.
    EndsWith,
    /// Tests text against a checked match expression.
    Matches,
    /// Tests whether a date or instant precedes the right operand.
    Before,
    /// Tests whether a date or instant follows the right operand.
    After,
    /// Tests whether a temporal value falls within checked bounds.
    Between,
    /// Tests temporal ordering before or at the right operand.
    OnOrBefore,
    /// Tests temporal ordering after or at the right operand.
    OnOrAfter,
    /// Tests that the left operand is Boolean `true`.
    IsTrue,
    /// Tests that the left operand is Boolean `false`.
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
