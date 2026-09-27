//! Canonical rendering and fact-path extraction for compiled conditions.
//!
//! Two compiled conditions are semantically identical when their span-free renderings are equal.
//! Source spans are provenance, not semantics, so they are excluded here. The same rendering backs
//! condition hashing, redundancy comparison, and the lossless `Derived` decision-table cell, which
//! is why it lives beside the expression model instead of inside one consumer.

use std::collections::BTreeSet;
use std::fmt::Write;

use rulery_contracts::{FactPath, Value};

use crate::{Expr, ExprOperand, Operator, ReservedOperand};

/// Renders one condition in a canonical, span-free form.
///
/// The rendering is injective over checked conditions: every value carries its kind, every operand
/// carries its role, and every source span is omitted, so two conditions render identically exactly
/// when they are structurally identical.
#[must_use]
pub fn canonical_condition(expression: &Expr) -> String {
    let mut rendered = String::new();
    render_expression(expression, &mut rendered);
    rendered
}

/// Returns every fact path a condition reads, in ascending path order.
///
/// Collection is polarity-blind and shape-blind: a path referenced under `Not` or `Any` is returned
/// exactly like a path referenced directly, because the truth of a compiled rule depends on every
/// fact its expression reads regardless of how the reads are combined.
#[must_use]
pub fn referenced_fact_paths(expression: &Expr) -> BTreeSet<FactPath> {
    let mut paths = BTreeSet::new();
    collect_paths(expression, &mut paths);
    paths
}

fn collect_paths(expression: &Expr, paths: &mut BTreeSet<FactPath>) {
    match expression {
        Expr::Constant { .. } => {}
        Expr::Predicate(predicate) => {
            collect_operand(&predicate.left, paths);
            if let Some(right) = &predicate.right {
                collect_operand(right, paths);
            }
        }
        Expr::All { expressions, .. } | Expr::Any { expressions, .. } => {
            for child in expressions {
                collect_paths(child, paths);
            }
        }
        Expr::Not { expression, .. } => collect_paths(expression, paths),
    }
}

fn collect_operand(operand: &ExprOperand, paths: &mut BTreeSet<FactPath>) {
    if let ExprOperand::Fact(path) = operand {
        paths.insert(path.clone());
    }
}

fn render_expression(expression: &Expr, out: &mut String) {
    match expression {
        Expr::Constant { value, .. } => {
            let _ = write!(out, "constant({value})");
        }
        Expr::Predicate(predicate) => {
            let _ = write!(out, "{}(", operator_name(predicate.operator));
            render_operand(&predicate.left, out);
            if let Some(right) = &predicate.right {
                out.push(',');
                render_operand(right, out);
            }
            out.push(')');
        }
        Expr::All { expressions, .. } => render_group("all", expressions, out),
        Expr::Any { expressions, .. } => render_group("any", expressions, out),
        Expr::Not { expression, .. } => {
            out.push_str("not(");
            render_expression(expression, out);
            out.push(')');
        }
    }
}

fn render_group(name: &str, expressions: &[Expr], out: &mut String) {
    let _ = write!(out, "{name}(");
    for (index, child) in expressions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        render_expression(child, out);
    }
    out.push(')');
}

fn render_operand(operand: &ExprOperand, out: &mut String) {
    match operand {
        ExprOperand::Fact(path) => {
            let _ = write!(out, "fact({path})");
        }
        ExprOperand::Literal(value) => {
            let _ = write!(out, "literal({})", canonical_value(value));
        }
        ExprOperand::Reserved(reserved) => {
            let _ = write!(out, "reserved({})", reserved_name(*reserved));
        }
    }
}

fn reserved_name(reserved: ReservedOperand) -> &'static str {
    match reserved {
        ReservedOperand::Today => "today",
        ReservedOperand::Now => "now",
    }
}

const fn operator_name(operator: Operator) -> &'static str {
    match operator {
        Operator::Exists => "exists",
        Operator::Missing => "missing",
        Operator::Equals => "equals",
        Operator::NotEquals => "not_equals",
        Operator::LessThan => "less_than",
        Operator::LessOrEqual => "less_or_equal",
        Operator::GreaterThan => "greater_than",
        Operator::GreaterOrEqual => "greater_or_equal",
        Operator::IsOneOf => "is_one_of",
        Operator::Contains => "contains",
        Operator::StartsWith => "starts_with",
        Operator::EndsWith => "ends_with",
        Operator::Matches => "matches",
        Operator::Before => "before",
        Operator::After => "after",
        Operator::Between => "between",
        Operator::OnOrBefore => "on_or_before",
        Operator::OnOrAfter => "on_or_after",
        Operator::IsTrue => "is_true",
        Operator::IsFalse => "is_false",
    }
}

fn canonical_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Boolean(inner) => format!("bool:{inner}"),
        Value::Integer(inner) => format!("int:{inner}"),
        Value::Decimal(inner) => format!("decimal:{inner}"),
        Value::Text(inner) => format!("text:{inner:?}"),
        Value::Date(inner) => format!("date:{inner}"),
        Value::DateTime(inner) => format!("datetime:{inner}"),
        Value::Duration(inner) => format!("duration:{inner}"),
        Value::Enum(inner) => {
            format!(
                "enum:{}.{}",
                inner.type_id().as_str(),
                inner.variant().as_str()
            )
        }
        Value::List(items) => {
            let mut rendered = String::from("list:[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                rendered.push_str(&canonical_value(item));
            }
            rendered.push(']');
            rendered
        }
        Value::Record(fields) => {
            let mut rendered = String::from("record:{");
            for (index, (name, item)) in fields.iter().enumerate() {
                if index > 0 {
                    rendered.push(',');
                }
                let _ = write!(rendered, "{name}={}", canonical_value(item));
            }
            rendered.push('}');
            rendered
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use rulery_contracts::{
        EnumValue, SourceFile, SourceId, SourceKey, SourceMap, SourcePath, Span, StableId, TypeId,
    };

    use super::*;

    #[test]
    fn condition_identity_ignores_spans_and_collects_every_path() {
        let path = |value: &str| FactPath::from_str(value).expect("path");
        let (left_span, right_span) = span_pair();
        let active = |span: Span| {
            Expr::Predicate(crate::Predicate {
                operator: Operator::Equals,
                left: ExprOperand::Fact(path("member.account-status")),
                right: Some(ExprOperand::Literal(Value::Enum(EnumValue::new(
                    TypeId::new("account-status").expect("type"),
                    StableId::new("active").expect("variant"),
                )))),
                span,
            })
        };

        assert_eq!(
            canonical_condition(&active(left_span)),
            canonical_condition(&active(right_span))
        );
        assert_eq!(
            canonical_condition(&active(left_span)),
            "equals(fact(member.account-status),literal(enum:account-status.active))"
        );

        let nested = Expr::Any {
            expressions: vec![
                Expr::Not {
                    expression: Box::new(Expr::Predicate(crate::Predicate {
                        operator: Operator::Exists,
                        left: ExprOperand::Fact(path("member.training")),
                        right: None,
                        span: left_span,
                    })),
                    span: left_span,
                },
                active(left_span),
                Expr::Predicate(crate::Predicate {
                    operator: Operator::Missing,
                    left: ExprOperand::Fact(path("tool.reserved-for-member-id")),
                    right: None,
                    span: right_span,
                }),
            ],
            span: left_span,
        };
        assert_eq!(
            referenced_fact_paths(&nested),
            BTreeSet::from([
                path("member.account-status"),
                path("member.training"),
                path("tool.reserved-for-member-id"),
            ])
        );
        assert!(canonical_condition(&nested).starts_with("any(not(exists(fact(member.training)))"));

        let constant = Expr::Constant {
            value: false,
            span: left_span,
        };
        assert!(referenced_fact_paths(&constant).is_empty());
        assert_eq!(canonical_condition(&constant), "constant(false)");
    }

    fn span_pair() -> (Span, Span) {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("source.main").expect("source"),
                SourcePath::new("rules/access.yaml").expect("path"),
                Arc::<str>::from("member access rules"),
            ),
        )
        .expect("source map");
        (
            map.span(SourceKey::new(1), 0, 6).expect("left span"),
            map.span(SourceKey::new(1), 7, 13).expect("right span"),
        )
    }
}
