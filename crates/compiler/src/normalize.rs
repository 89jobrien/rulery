//! Condition normalization and static specificity scoring.

use rulery_ir::{Expr, ExprOperand, Operator, Predicate};

/// Normalized condition and its static metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizationOutput {
    /// Canonically ordered condition expression.
    pub condition: Expr,
    /// Checked static specificity score.
    pub specificity: u32,
    /// Advisory messages produced while lowering empty containers.
    pub advisories: Vec<String>,
}

/// Condition normalization failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizationError {
    /// Specificity arithmetic exceeded `u32`.
    SpecificityOverflow,
    /// Canonical expression serialization failed.
    Canonicalization(String),
}

impl std::fmt::Display for NormalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpecificityOverflow => formatter.write_str("specificity exceeds u32"),
            Self::Canonicalization(message) => {
                write!(formatter, "condition canonicalization failed: {message}")
            }
        }
    }
}

impl std::error::Error for NormalizationError {}

/// Normalizes a checked condition and computes its static specificity.
///
/// # Errors
///
/// Returns [`NormalizationError`] when canonicalization or checked score arithmetic fails.
pub fn normalize_condition(condition: Expr) -> Result<NormalizationOutput, NormalizationError> {
    let mut advisories = Vec::new();
    let condition = normalize_expr(condition, &mut advisories)?;
    let mut predicates = Vec::new();
    collect_unique_predicates(&condition, &mut predicates);

    let atomic_count =
        u32::try_from(predicates.len()).map_err(|_| NormalizationError::SpecificityOverflow)?;
    let mut segment_count = 0_u32;
    let mut temporal_count = 0_u32;
    for predicate in predicates {
        if let ExprOperand::Fact(path) = &predicate.left {
            let segments =
                u32::try_from(path.len()).map_err(|_| NormalizationError::SpecificityOverflow)?;
            segment_count = segment_count
                .checked_add(segments)
                .ok_or(NormalizationError::SpecificityOverflow)?;
        }
        if is_temporal_or_range(predicate.operator) {
            temporal_count = temporal_count
                .checked_add(1)
                .ok_or(NormalizationError::SpecificityOverflow)?;
        }
    }
    let specificity = specificity_from_counts(atomic_count, segment_count, temporal_count)?;

    Ok(NormalizationOutput {
        condition,
        specificity,
        advisories,
    })
}

/// Computes specificity from already-counted formula components.
///
/// # Errors
///
/// Returns [`NormalizationError::SpecificityOverflow`] if checked `u32` arithmetic overflows.
pub fn specificity_from_counts(
    atomic_predicates: u32,
    fact_path_segments: u32,
    temporal_and_range_predicates: u32,
) -> Result<u32, NormalizationError> {
    let temporal_weight = temporal_and_range_predicates
        .checked_mul(2)
        .ok_or(NormalizationError::SpecificityOverflow)?;
    atomic_predicates
        .checked_add(fact_path_segments)
        .and_then(|score| score.checked_add(temporal_weight))
        .ok_or(NormalizationError::SpecificityOverflow)
}

fn normalize_expr(
    expression: Expr,
    advisories: &mut Vec<String>,
) -> Result<Expr, NormalizationError> {
    match expression {
        Expr::All { expressions, span } => {
            if expressions.is_empty() {
                advisories.push("empty all condition lowered to true".to_owned());
                return Ok(Expr::Constant { value: true, span });
            }
            Ok(Expr::All {
                expressions: normalize_children(expressions, advisories)?,
                span,
            })
        }
        Expr::Any { expressions, span } => {
            if expressions.is_empty() {
                advisories.push("empty any condition lowered to false".to_owned());
                return Ok(Expr::Constant { value: false, span });
            }
            Ok(Expr::Any {
                expressions: normalize_children(expressions, advisories)?,
                span,
            })
        }
        Expr::Not { expression, span } => Ok(Expr::Not {
            expression: Box::new(normalize_expr(*expression, advisories)?),
            span,
        }),
        leaf => Ok(leaf),
    }
}

fn normalize_children(
    children: Vec<Expr>,
    advisories: &mut Vec<String>,
) -> Result<Vec<Expr>, NormalizationError> {
    // Serialized JSON bytes provide one deterministic ordering for commutative all/any children.
    // Normalization changes child order but retains each child's semantic content.
    let mut keyed = children
        .into_iter()
        .map(|child| {
            let child = normalize_expr(child, advisories)?;
            let key = serde_json::to_vec(&child)
                .map_err(|error| NormalizationError::Canonicalization(error.to_string()))?;
            Ok((key, child))
        })
        .collect::<Result<Vec<_>, NormalizationError>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(keyed.into_iter().map(|(_, child)| child).collect())
}

fn collect_unique_predicates<'a>(expression: &'a Expr, predicates: &mut Vec<&'a Predicate>) {
    // Specificity counts structurally unique predicates. Equality includes operands and spans, so
    // separately authored occurrences remain distinct even when their text is otherwise equal.
    match expression {
        Expr::Predicate(predicate) => {
            if !predicates.contains(&predicate) {
                predicates.push(predicate);
            }
        }
        Expr::All { expressions, .. } | Expr::Any { expressions, .. } => {
            for child in expressions {
                collect_unique_predicates(child, predicates);
            }
        }
        Expr::Not { expression, .. } => collect_unique_predicates(expression, predicates),
        Expr::Constant { .. } => {}
    }
}

const fn is_temporal_or_range(operator: Operator) -> bool {
    matches!(
        operator,
        Operator::Before
            | Operator::After
            | Operator::Between
            | Operator::OnOrBefore
            | Operator::OnOrAfter
    )
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use rulery_contracts::{FactPath, SourceFile, SourceId, SourceKey, SourceMap, SourcePath};
    use rulery_ir::{ExprOperand, ReservedOperand};

    use super::*;

    #[test]
    fn specificity_uses_exact_checked_formula() {
        let map = source_map();
        let span = map.span(SourceKey::new(1), 0, 1).expect("span");
        let temporal = Expr::Predicate(Predicate {
            operator: Operator::Before,
            left: ExprOperand::Fact(
                FactPath::from_str("member.training.valid-until").expect("fact path"),
            ),
            right: Some(ExprOperand::Reserved(ReservedOperand::Today)),
            span,
        });

        let normalized = normalize_condition(Expr::All {
            expressions: vec![
                Expr::Constant { value: true, span },
                Expr::Not {
                    expression: Box::new(temporal.clone()),
                    span,
                },
                temporal.clone(),
            ],
            span,
        })
        .expect("normalize");
        assert_eq!(normalized.specificity, 1 + 3 + 2);

        let empty_all = normalize_condition(Expr::All {
            expressions: Vec::new(),
            span,
        })
        .expect("empty all");
        assert!(matches!(
            empty_all.condition,
            Expr::Constant { value: true, .. }
        ));
        assert_eq!(empty_all.specificity, 0);
        assert_eq!(empty_all.advisories.len(), 1);

        let empty_any = normalize_condition(Expr::Any {
            expressions: Vec::new(),
            span,
        })
        .expect("empty any");
        assert!(matches!(
            empty_any.condition,
            Expr::Constant { value: false, .. }
        ));
        assert_eq!(empty_any.specificity, 0);

        let left = normalize_condition(Expr::All {
            expressions: vec![Expr::Constant { value: false, span }, temporal.clone()],
            span,
        })
        .expect("left");
        let right = normalize_condition(Expr::All {
            expressions: vec![temporal, Expr::Constant { value: false, span }],
            span,
        })
        .expect("right");
        assert_eq!(left.condition, right.condition);
        assert_eq!(left.specificity, right.specificity);

        assert_eq!(
            specificity_from_counts(u32::MAX, 1, 0),
            Err(NormalizationError::SpecificityOverflow)
        );
        assert_eq!(
            specificity_from_counts(0, 0, u32::MAX),
            Err(NormalizationError::SpecificityOverflow)
        );
    }

    fn source_map() -> SourceMap {
        let mut map = SourceMap::new();
        map.insert(
            SourceKey::new(1),
            SourceFile::new(
                SourceId::new("src.main").expect("source id"),
                SourcePath::new("rules/main.yaml").expect("source path"),
                Arc::<str>::from("x"),
            ),
        )
        .expect("insert source");
        map
    }
}
