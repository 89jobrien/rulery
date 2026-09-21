//! Canonical finite-domain decision partitioning.

use std::collections::BTreeMap;

use rulery_contracts::{DecisionId, FactPath, FactValidationError, Value};
use serde::{Deserialize, Serialize};

/// Analysis completeness state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisCompleteness {
    /// Declared finite domain was completely enumerated.
    Complete,
    /// Enumeration or domain construction was incomplete.
    Inconclusive {
        /// States examined before incompleteness.
        examined: u64,
        /// Configured state limit.
        limit: u64,
    },
}

/// One finite fact-domain value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum FactPartitionValue {
    /// Fact is absent.
    Absent,
    /// Fact is explicit null.
    Null,
    /// Fact is a valid typed value.
    Valid(Value),
    /// Fact is malformed.
    Malformed(FactValidationError),
}

/// Explicit finite values supplied for an otherwise unsupported domain.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteDomain {
    /// Canonically sorted values.
    pub values: Vec<FactPartitionValue>,
}

impl FiniteDomain {
    /// Creates a finite domain; values are canonicalized during partition construction.
    #[must_use]
    pub fn new(values: Vec<FactPartitionValue>) -> Self {
        Self { values }
    }
}

/// Analysis command options.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisOptions {
    /// Maximum examined states.
    pub max_states: u64,
    /// Maximum generated witnesses.
    pub max_witnesses: u32,
    /// Explicit finite domain overrides.
    pub explicit_domains: BTreeMap<FactPath, FiniteDomain>,
    /// Include reachability analysis.
    pub include_reachability: bool,
    /// Include interaction analysis.
    pub include_interactions: bool,
    /// Include coverage analysis.
    pub include_coverage: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            max_states: 100_000,
            max_witnesses: 1_000,
            explicit_domains: BTreeMap::new(),
            include_reachability: true,
            include_interactions: true,
            include_coverage: true,
        }
    }
}

/// Declared domain shape and authored scalar boundaries for one fact path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "boundaries", rename_all = "snake_case")]
pub enum PartitionDomainKind {
    /// Boolean finite domain.
    Boolean,
    /// Declared enum values.
    Enum(Vec<Value>),
    /// Authored integer boundaries.
    Integer(Vec<i64>),
    /// Authored text boundaries; non-literal intervals are unsupported.
    Text(Vec<String>),
    /// List value domain (presence cells only without override).
    List,
    /// Record value domain (presence cells only without override).
    Record,
}

/// Partition declaration for one referenced path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionSpec {
    /// Canonical fact path.
    pub path: FactPath,
    /// Typed value-domain construction rule.
    pub kind: PartitionDomainKind,
    /// Whether absence is vocabulary-satisfiable.
    pub allow_absent: bool,
    /// Whether explicit null is vocabulary-satisfiable.
    pub allow_null: bool,
    /// Whether malformed evidence is relevant to this partition.
    pub allow_malformed: bool,
    /// Vocabulary-unsatisfiable values removed before Cartesian enumeration.
    pub forbidden: Vec<FactPartitionValue>,
}

/// One lexicographically ordered assignment cell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionCell {
    /// Fact assignments in ascending path order.
    pub assignments: BTreeMap<FactPath, FactPartitionValue>,
}

/// Finite partition for one decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionPartition {
    /// Decision identity.
    pub decision: DecisionId,
    /// Referenced paths in ascending order.
    pub paths: Vec<FactPath>,
    /// Canonically sorted satisfiable Cartesian cells.
    pub cells: Vec<PartitionCell>,
    /// Completeness for the declared domain.
    pub completeness: AnalysisCompleteness,
}

/// Builds a canonical finite partition from typed path declarations.
#[must_use]
pub fn build_partition(
    decision: DecisionId,
    mut specs: Vec<PartitionSpec>,
    options: &AnalysisOptions,
) -> DecisionPartition {
    specs.sort_by(|left, right| left.path.cmp(&right.path));
    let paths = specs
        .iter()
        .map(|spec| spec.path.clone())
        .collect::<Vec<_>>();
    let mut complete = true;
    let domains = specs
        .iter()
        .map(|spec| {
            let (mut values, domain_complete) =
                if let Some(explicit) = options.explicit_domains.get(&spec.path) {
                    (explicit.values.clone(), true)
                } else {
                    inferred_domain(spec)
                };
            complete &= domain_complete;
            add_presence_values(spec, &mut values);
            values.retain(|value| !spec.forbidden.contains(value));
            canonicalize_values(&mut values);
            values
        })
        .collect::<Vec<_>>();

    let mut cells = vec![PartitionCell {
        assignments: BTreeMap::new(),
    }];
    for (path, domain) in paths.iter().zip(domains) {
        let mut next = Vec::new();
        for cell in &cells {
            for value in &domain {
                let mut assignments = cell.assignments.clone();
                assignments.insert(path.clone(), value.clone());
                next.push(PartitionCell { assignments });
            }
        }
        cells = next;
    }
    cells.sort_by_key(|cell| canonical_bytes(&cell.assignments));

    DecisionPartition {
        decision,
        paths,
        cells,
        completeness: if complete {
            AnalysisCompleteness::Complete
        } else {
            AnalysisCompleteness::Inconclusive {
                examined: 0,
                limit: options.max_states,
            }
        },
    }
}

fn inferred_domain(spec: &PartitionSpec) -> (Vec<FactPartitionValue>, bool) {
    match &spec.kind {
        PartitionDomainKind::Boolean => (
            vec![
                FactPartitionValue::Valid(Value::Boolean(false)),
                FactPartitionValue::Valid(Value::Boolean(true)),
            ],
            true,
        ),
        PartitionDomainKind::Enum(variants) => (
            variants
                .iter()
                .cloned()
                .map(FactPartitionValue::Valid)
                .collect(),
            true,
        ),
        PartitionDomainKind::Integer(boundaries) => (integer_domain(boundaries), true),
        PartitionDomainKind::Text(boundaries) => (
            boundaries
                .iter()
                .cloned()
                .map(Value::Text)
                .map(FactPartitionValue::Valid)
                .collect(),
            false,
        ),
        PartitionDomainKind::List | PartitionDomainKind::Record => (Vec::new(), false),
    }
}

fn integer_domain(boundaries: &[i64]) -> Vec<FactPartitionValue> {
    let mut boundaries = boundaries.to_vec();
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut representatives = Vec::new();
    if let Some(first) = boundaries.first().and_then(|value| value.checked_sub(1)) {
        representatives.push(first);
    }
    for (index, boundary) in boundaries.iter().copied().enumerate() {
        representatives.push(boundary);
        if let Some(next) = boundaries.get(index + 1).copied()
            && boundary.checked_add(1).is_some_and(|value| value < next)
        {
            representatives.push(boundary + 1);
        }
    }
    if let Some(last) = boundaries.last().and_then(|value| value.checked_add(1)) {
        representatives.push(last);
    }
    representatives
        .into_iter()
        .map(Value::Integer)
        .map(FactPartitionValue::Valid)
        .collect()
}

fn add_presence_values(spec: &PartitionSpec, values: &mut Vec<FactPartitionValue>) {
    if spec.allow_absent {
        values.push(FactPartitionValue::Absent);
    }
    if spec.allow_null {
        values.push(FactPartitionValue::Null);
    }
    if spec.allow_malformed {
        values.push(FactPartitionValue::Malformed(
            FactValidationError::OutOfRange,
        ));
    }
}

fn canonicalize_values(values: &mut Vec<FactPartitionValue>) {
    values.sort_by_key(canonical_bytes);
    values.dedup();
}

fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn partitions_follow_canonical_finite_domain_rules() {
        let integer = FactPath::from_str("member.age").expect("path");
        let boolean = FactPath::from_str("member.active").expect("path");
        let state = FactPath::from_str("member.state").expect("path");
        let text = FactPath::from_str("member.name").expect("path");
        let list = FactPath::from_str("member.tags").expect("path");
        let options = AnalysisOptions::default();
        let partition = build_partition(
            DecisionId::new("decision.main").expect("decision"),
            vec![
                PartitionSpec {
                    path: integer.clone(),
                    kind: PartitionDomainKind::Integer(vec![18, 21]),
                    allow_absent: false,
                    allow_null: false,
                    allow_malformed: false,
                    forbidden: vec![FactPartitionValue::Valid(Value::Integer(18))],
                },
                PartitionSpec {
                    path: boolean.clone(),
                    kind: PartitionDomainKind::Boolean,
                    allow_absent: true,
                    allow_null: true,
                    allow_malformed: true,
                    forbidden: Vec::new(),
                },
                PartitionSpec {
                    path: state.clone(),
                    kind: PartitionDomainKind::Enum(vec![
                        Value::Text("gold".to_owned()),
                        Value::Text("silver".to_owned()),
                    ]),
                    allow_absent: false,
                    allow_null: false,
                    allow_malformed: false,
                    forbidden: Vec::new(),
                },
            ],
            &options,
        );
        assert_eq!(
            partition.paths,
            vec![boolean.clone(), integer.clone(), state.clone()]
        );
        assert_eq!(partition.completeness, AnalysisCompleteness::Complete);
        assert!(partition.cells.iter().all(|cell| {
            cell.assignments.get(&integer) != Some(&FactPartitionValue::Valid(Value::Integer(18)))
        }));
        let integer_values = partition
            .cells
            .iter()
            .filter_map(|cell| cell.assignments.get(&integer))
            .collect::<Vec<_>>();
        assert!(integer_values.contains(&&FactPartitionValue::Valid(Value::Integer(17))));
        assert!(integer_values.contains(&&FactPartitionValue::Valid(Value::Integer(19))));
        assert!(integer_values.contains(&&FactPartitionValue::Valid(Value::Integer(21))));
        assert!(integer_values.contains(&&FactPartitionValue::Valid(Value::Integer(22))));

        let unsupported = build_partition(
            DecisionId::new("decision.text").expect("decision"),
            vec![
                PartitionSpec {
                    path: text.clone(),
                    kind: PartitionDomainKind::Text(vec!["a".to_owned(), "z".to_owned()]),
                    allow_absent: true,
                    allow_null: false,
                    allow_malformed: false,
                    forbidden: Vec::new(),
                },
                PartitionSpec {
                    path: list.clone(),
                    kind: PartitionDomainKind::List,
                    allow_absent: true,
                    allow_null: true,
                    allow_malformed: false,
                    forbidden: Vec::new(),
                },
            ],
            &options,
        );
        assert!(matches!(
            unsupported.completeness,
            AnalysisCompleteness::Inconclusive { .. }
        ));

        let mut explicit_options = AnalysisOptions::default();
        explicit_options.explicit_domains.insert(
            text.clone(),
            FiniteDomain::new(vec![
                FactPartitionValue::Valid(Value::Text("z".to_owned())),
                FactPartitionValue::Valid(Value::Text("a".to_owned())),
            ]),
        );
        explicit_options.explicit_domains.insert(
            list.clone(),
            FiniteDomain::new(vec![FactPartitionValue::Valid(Value::List(vec![
                Value::Text("x".to_owned()),
            ]))]),
        );
        let explicit = build_partition(
            DecisionId::new("decision.explicit").expect("decision"),
            vec![
                PartitionSpec {
                    path: text,
                    kind: PartitionDomainKind::Text(Vec::new()),
                    allow_absent: false,
                    allow_null: false,
                    allow_malformed: false,
                    forbidden: Vec::new(),
                },
                PartitionSpec {
                    path: list,
                    kind: PartitionDomainKind::List,
                    allow_absent: false,
                    allow_null: false,
                    allow_malformed: false,
                    forbidden: Vec::new(),
                },
            ],
            &explicit_options,
        );
        assert_eq!(explicit.completeness, AnalysisCompleteness::Complete);
        let canonical = explicit
            .cells
            .iter()
            .map(|cell| serde_json::to_vec(&cell.assignments).expect("json"))
            .collect::<Vec<_>>();
        let mut sorted = canonical.clone();
        sorted.sort();
        assert_eq!(canonical, sorted);
        assert_eq!(options.max_states, 100_000);
        assert_eq!(options.max_witnesses, 1_000);
    }
}
