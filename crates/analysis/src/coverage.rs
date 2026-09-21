//! Exact coverage over satisfiable finite partition cells.

use std::collections::{BTreeMap, BTreeSet};

use rulery_contracts::QualifiedRuleId;
use serde::{Deserialize, Serialize};

use crate::{AnalysisCompleteness, DecisionPartition, PartitionCell, WitnessCase};

/// Reason a satisfiable cell is not covered by a decisive non-default rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncoveredCategory {
    /// Only the declared default resolved the cell.
    DefaultOnly,
    /// A declared enum variant was not handled.
    UnhandledEnumVariant,
    /// Missing evidence remained relevant.
    MissingFact,
    /// Invalid evidence remained relevant.
    InvalidFact,
    /// Temporal boundary was uncovered.
    TemporalBoundary,
    /// No rule matched.
    NoMatchingRule,
    /// Domain could not be finitely represented.
    UnsupportedDomain,
}

/// Evaluator projection for one partition cell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellEvaluation {
    /// Evaluated partition cell.
    pub cell: PartitionCell,
    /// Whether vocabulary constraints permit the cell.
    pub satisfiable: bool,
    /// Rules reached by this cell.
    pub reached_rules: BTreeSet<QualifiedRuleId>,
    /// Non-default determining rules.
    pub determining_rules: BTreeSet<QualifiedRuleId>,
    /// Whether relevant unknown/invalid evidence remains.
    pub relevant_uncertainty: bool,
    /// Whether evaluation produced runtime conflict.
    pub conflict: bool,
    /// Uncovered category when not covered.
    pub category: Option<UncoveredCategory>,
    /// Replayable witness.
    pub witness: WitnessCase,
}

/// One uncovered satisfiable cell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncoveredCase {
    /// Partition cell.
    pub cell: PartitionCell,
    /// Exact uncovered category.
    pub category: UncoveredCategory,
    /// Replayable witness.
    pub witness: WitnessCase,
}

/// Per-rule exact coverage counts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleCoverage {
    /// Rule identity.
    pub rule: QualifiedRuleId,
    /// Satisfiable cells where the rule matched.
    pub reached_cells: u64,
    /// Satisfiable cells where the rule determined the outcome.
    pub determining_cells: u64,
}

/// Exact finite coverage report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageReport {
    /// Covered satisfiable cells.
    pub numerator: u64,
    /// All satisfiable partition cells before rule evaluation.
    pub denominator: u64,
    /// Exact floor basis points, absent when incomplete.
    pub percent_basis_points: Option<u16>,
    /// Enumeration completeness.
    pub completeness: AnalysisCompleteness,
    /// Uncovered satisfiable cells.
    pub uncovered: Vec<UncoveredCase>,
    /// Per-rule counts.
    pub rule_coverage: Vec<RuleCoverage>,
    /// Stable uncovered/inconclusive diagnostic codes.
    pub diagnostics: BTreeSet<String>,
}

/// Computes exact coverage from complete cell evaluations.
#[must_use]
pub fn compute_coverage(
    partition: &DecisionPartition,
    evaluations: Vec<CellEvaluation>,
) -> CoverageReport {
    let mut denominator = 0_u64;
    let mut numerator = 0_u64;
    let mut uncovered = Vec::new();
    let mut diagnostics = BTreeSet::new();
    let mut counts = BTreeMap::<QualifiedRuleId, (u64, u64)>::new();

    for evaluation in evaluations {
        if !evaluation.satisfiable {
            continue;
        }
        denominator += 1;
        for rule in &evaluation.reached_rules {
            counts.entry(rule.clone()).or_default().0 += 1;
        }
        for rule in &evaluation.determining_rules {
            counts.entry(rule.clone()).or_default().1 += 1;
        }
        let covered = !evaluation.determining_rules.is_empty()
            && !evaluation.relevant_uncertainty
            && !evaluation.conflict;
        if covered {
            numerator += 1;
        } else {
            let category = evaluation
                .category
                .unwrap_or(UncoveredCategory::NoMatchingRule);
            diagnostics.insert(category_code(category).to_owned());
            uncovered.push(UncoveredCase {
                cell: evaluation.cell,
                category,
                witness: evaluation.witness,
            });
        }
    }
    uncovered.sort_by_key(|case| serde_json::to_vec(&case.cell.assignments).unwrap_or_default());
    let rule_coverage = counts
        .into_iter()
        .map(|(rule, (reached_cells, determining_cells))| RuleCoverage {
            rule,
            reached_cells,
            determining_cells,
        })
        .collect();
    let percent_basis_points = if partition.completeness == AnalysisCompleteness::Complete {
        Some(if denominator == 0 {
            10_000
        } else {
            u16::try_from(u128::from(numerator) * 10_000 / u128::from(denominator))
                .unwrap_or(10_000)
        })
    } else {
        diagnostics.insert("RUL254".to_owned());
        None
    };
    CoverageReport {
        numerator,
        denominator,
        percent_basis_points,
        completeness: partition.completeness.clone(),
        uncovered,
        rule_coverage,
        diagnostics,
    }
}

const fn category_code(category: UncoveredCategory) -> &'static str {
    match category {
        UncoveredCategory::DefaultOnly | UncoveredCategory::NoMatchingRule => "RUL250",
        UncoveredCategory::UnhandledEnumVariant => "RUL251",
        UncoveredCategory::MissingFact => "RUL252",
        UncoveredCategory::TemporalBoundary => "RUL253",
        UncoveredCategory::InvalidFact | UncoveredCategory::UnsupportedDomain => "RUL254",
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rulery_contracts::{
        FactPath, PackageId, RuleId, TimeZoneDatabaseIdentity, UtcInstant, Value,
    };

    use crate::{FactPartitionValue, WitnessClaim, minimize_witness};

    use super::*;

    #[test]
    fn coverage_uses_satisfiable_partition_denominator() {
        let covered = cell(1);
        let missing = cell(2);
        let unsatisfiable = cell(3);
        let rule_a = qualified("rule.a");
        let rule_b = qualified("rule.b");
        let partition = DecisionPartition {
            decision: "decision.main".parse().expect("decision"),
            paths: vec![FactPath::from_str("member.value").expect("path")],
            cells: vec![covered.clone(), missing.clone(), unsatisfiable.clone()],
            completeness: AnalysisCompleteness::Complete,
        };
        let report = compute_coverage(
            &partition,
            vec![
                evaluation(covered, true, [rule_a.clone()], [rule_a.clone()], None),
                evaluation(
                    missing,
                    true,
                    [rule_a.clone(), rule_b.clone()],
                    [],
                    Some(UncoveredCategory::MissingFact),
                ),
                evaluation(unsatisfiable, false, [], [], None),
            ],
        );
        assert_eq!(report.denominator, 2);
        assert_eq!(report.numerator, 1);
        assert_eq!(report.percent_basis_points, Some(5_000));
        assert_eq!(report.uncovered.len(), 1);
        assert_eq!(report.uncovered[0].category, UncoveredCategory::MissingFact);
        assert_eq!(report.diagnostics, BTreeSet::from(["RUL252".to_owned()]));
        assert_eq!(
            report.rule_coverage,
            vec![
                RuleCoverage {
                    rule: rule_a,
                    reached_cells: 2,
                    determining_cells: 1
                },
                RuleCoverage {
                    rule: rule_b,
                    reached_cells: 1,
                    determining_cells: 0
                },
            ]
        );

        let zero = compute_coverage(
            &DecisionPartition {
                decision: "decision.zero".parse().expect("decision"),
                paths: Vec::new(),
                cells: Vec::new(),
                completeness: AnalysisCompleteness::Complete,
            },
            Vec::new(),
        );
        assert_eq!(zero.denominator, 0);
        assert_eq!(zero.percent_basis_points, Some(10_000));

        let incomplete = compute_coverage(
            &DecisionPartition {
                decision: "decision.incomplete".parse().expect("decision"),
                paths: Vec::new(),
                cells: Vec::new(),
                completeness: AnalysisCompleteness::Inconclusive {
                    examined: 1,
                    limit: 1,
                },
            },
            Vec::new(),
        );
        assert_eq!(incomplete.percent_basis_points, None);
        assert!(incomplete.diagnostics.contains("RUL254"));

        let categories = [
            (UncoveredCategory::DefaultOnly, "RUL250"),
            (UncoveredCategory::UnhandledEnumVariant, "RUL251"),
            (UncoveredCategory::MissingFact, "RUL252"),
            (UncoveredCategory::TemporalBoundary, "RUL253"),
            (UncoveredCategory::UnsupportedDomain, "RUL254"),
        ];
        for (category, code) in categories {
            assert_eq!(category_code(category), code);
        }
    }

    fn cell(value: i64) -> PartitionCell {
        PartitionCell {
            assignments: BTreeMap::from([(
                FactPath::from_str("member.value").expect("path"),
                FactPartitionValue::Valid(Value::Integer(value)),
            )]),
        }
    }

    fn evaluation<const R: usize, const D: usize>(
        cell: PartitionCell,
        satisfiable: bool,
        reached: [QualifiedRuleId; R],
        determining: [QualifiedRuleId; D],
        category: Option<UncoveredCategory>,
    ) -> CellEvaluation {
        let witness = minimize_witness(
            "coverage",
            BTreeMap::new(),
            UtcInstant::new(1).expect("instant"),
            TimeZoneDatabaseIdentity::new("test", "1").expect("database"),
            BTreeSet::new(),
            WitnessClaim::Uncovered,
            |_, _, _| true,
        );
        CellEvaluation {
            cell,
            satisfiable,
            reached_rules: BTreeSet::from(reached),
            determining_rules: BTreeSet::from(determining),
            relevant_uncertainty: category == Some(UncoveredCategory::MissingFact)
                || category == Some(UncoveredCategory::InvalidFact),
            conflict: false,
            category,
            witness,
        }
    }

    fn qualified(rule: &str) -> QualifiedRuleId {
        QualifiedRuleId::new(
            PackageId::new("pkg.main").expect("package"),
            RuleId::new(rule).expect("rule"),
        )
    }
}
