//! Scaling of the decision partition's Cartesian product.
//!
//! `build_partition` enumerates the complete product of every path's inferred domain. That product
//! is where the analyzer's cost lives, and its size is why the normative tool-library analysis
//! reaches 600 cells. Hierarchical generation is the known alternative, but changing the cell
//! count would move every witness hash and the frozen BLAKE3 vectors, so it is not a change worth
//! making blind.
//!
//! These benches exist to answer one question before that work is attempted: how does partition
//! construction actually scale with path count and domain width, and is the cost in the product
//! itself or in the per-cell `BTreeMap` clone? Run with:
//!
//! ```text
//! cargo bench -p rulery-analysis
//! ```
//!
//! The measurement is deliberately over synthetic specs rather than the tool-library package.
//! Synthetic input is what makes the exponent legible; the fixture only confirms one operating
//! point, and the fixture's own count is already pinned by its conformance test.

use std::collections::BTreeMap;
use std::str::FromStr;

use criterion::{BatchSize, Criterion, Throughput};
use rulery_analysis::{AnalysisOptions, PartitionDomainKind, PartitionSpec, build_partition};
use rulery_contracts::{DecisionId, FactPath, Value};

/// Builds `paths` boolean specs, each with `absent`, `null`, and `malformed` added to its domain.
///
/// Four values per path, so cell count is exactly `4^paths` and the scaling exponent is readable
/// off the result.
fn boolean_specs(paths: usize) -> Vec<PartitionSpec> {
    (0..paths)
        .map(|index| PartitionSpec {
            path: FactPath::from_str(&format!("member.field-{index}")).expect("path"),
            kind: PartitionDomainKind::Boolean,
            allow_absent: true,
            allow_null: true,
            allow_malformed: true,
            forbidden: Vec::new(),
        })
        .collect()
}

/// Builds `paths` specs over a boolean domain narrowed to a single value.
///
/// One value per path means the product is size 1 while still touching every spec, which isolates
/// the fixed per-path cost from the multiplicative one.
fn single_value_specs(paths: usize) -> Vec<PartitionSpec> {
    (0..paths)
        .map(|index| PartitionSpec {
            path: FactPath::from_str(&format!("member.field-{index}")).expect("path"),
            kind: PartitionDomainKind::Enum(vec![Value::Boolean(index % 2 == 0)]),
            allow_absent: false,
            allow_null: false,
            allow_malformed: false,
            forbidden: Vec::new(),
        })
        .collect()
}

fn options() -> AnalysisOptions {
    AnalysisOptions {
        max_states: u64::MAX,
        max_witnesses: 1,
        explicit_domains: BTreeMap::new(),
        include_reachability: false,
        include_interactions: false,
        include_coverage: false,
    }
}

fn decision() -> DecisionId {
    DecisionId::new("checkout").expect("decision")
}

fn partition_product_scaling(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("partition_product");
    for paths in [3_usize, 4, 5, 6, 7] {
        let specs = boolean_specs(paths);
        let exponent = u32::try_from(paths).expect("path count fits in u32");
        let expected = 4_usize.pow(exponent);
        let throughput = u64::try_from(expected).expect("cell count fits in u64");
        group.throughput(Throughput::Elements(throughput));
        group.bench_function(format!("{paths}-paths-4-values"), |bencher| {
            // `build_partition` consumes its specs, so each iteration needs a fresh vector.
            // Cloning inside the timed closure would measure the clone — at 1024 paths that is
            // several thousand allocations and it swamps the thing being measured. `iter_batched`
            // times only the function under test.
            bencher.iter_batched(
                || specs.clone(),
                |specs| build_partition(decision(), specs, &options()).cells.len(),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn partition_fixed_cost_scaling(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("partition_fixed_cost");
    for paths in [16_usize, 64, 256, 1_024] {
        let specs = single_value_specs(paths);
        // The group is only meaningful if the product really is one cell: the whole point is to
        // separate per-path cost from the Cartesian explosion. Assert it rather than assume it,
        // because a silently wrong cell count would make the quadratic reading below a fiction.
        let produced = build_partition(decision(), specs.clone(), &options())
            .cells
            .len();
        assert_eq!(
            produced, 1,
            "{paths} single-value paths must produce one cell, not {produced}"
        );
        group.bench_function(format!("{paths}-paths-1-value"), |bencher| {
            bencher.iter_batched(
                || specs.clone(),
                |specs| build_partition(decision(), specs, &options()).cells.len(),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// The clone that `iter_batched` performs as unmeasured setup.
///
/// Without this control the fixed-cost numbers are unattributable: a benchmark measuring a
/// contaminated input looks exactly like a slow function. If this ever approaches the cost of
/// `partition_fixed_cost`, the group above is measuring setup rather than partition construction.
fn setup_clone_scaling(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("setup_clone");
    for paths in [16_usize, 64, 256, 1_024] {
        let specs = single_value_specs(paths);
        group.bench_function(format!("{paths}-paths-1-value"), |bencher| {
            bencher.iter(|| specs.clone().len());
        });
    }
    group.finish();
}

/// Runs every benchmark group.
///
/// `criterion_group!` is deliberately not used. It expands to undocumented functions, which the
/// workspace's `missing_docs` lint rejects, and silencing that would need an `#[allow]` outside the
/// set this repository permits. Writing the entry point out costs a few lines and keeps the strict
/// lint baseline intact with no suppressions.
fn main() {
    let mut criterion = Criterion::default();
    partition_product_scaling(&mut criterion);
    partition_fixed_cost_scaling(&mut criterion);
    setup_clone_scaling(&mut criterion);
    criterion.final_summary();
}
