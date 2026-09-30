# Product Context

## Why this exists

Operational decisions get encoded once in application logic and then drift. The business rule, the
code that enforces it, the audit trail, and the thing the auditor was shown all become separate
artifacts that disagree. Rulery's premise is that these should be **one artifact**, authored once
and compiled into every other form it needs.

Concretely, a Rulery rulebook is a YAML package that compiles to a deterministic IR. That same
package can then be evaluated (decision traces), statically analysed (reachability, coverage,
semantic diffs), rendered (human explanation, JSON, SARIF, decision table, Markdown), and locked
by content hash. Nothing re-implements the rule to produce those views; they are all projections of
the compiled form.

## What "auditable" means here

The specification's framing is that an inconclusive result must never be promoted to a proof of
absence or equivalence. Source: `crates/analysis/src/lib.rs:1-6`.

Three concrete consequences visible in the code:

- **Four-valued truth, not boolean.** `True`, `False`, `Unknown` (evidence absent), `Invalid`
  (evidence present but violating its contract). Collapsing `Unknown` and `Invalid` into one
  "false" case is what makes rulebooks lie. The algebra is deliberately not a lattice: conjunction
  and disjunction use **two different total orders**, which is why absorption and both
  distributivity laws fail. Those failures are recorded as law in `crates/engine/src/truth.rs`, not
  repaired, because the specification's tables define the evaluation semantics.

- **Analysis reports its own completeness.** `AnalysisCompleteness::Inconclusive` carries the
  examined count and the limit. A budget-exhausted analysis says so rather than claiming it proved
  something.

- **Traces are reproducible by hash.** Canonical BLAKE3 with length-prefixed framing and explicit
  domain separation, so a trace's identity is a function of its inputs and nothing else.

## Design principles to preserve

1. **Determinism over cleverness.** `BTreeMap`/`BTreeSet` universally, never `HashMap`/`HashSet`,
   specifically so serialization is byte-deterministic. Source: `AGENTS.md` ("Collections").

2. **A wrong answer is worse than no answer.** When a result was never derived, fail with a typed
   error rather than emitting a plausible artifact. This is why `analyze` and `diff` error out
   instead of returning an un-derived report.

3. **Validation at the boundary, once.** Validating newtypes (`StableId`, `UtcInstant`,
   `PolicyDate`, `DecimalValue`) mean downstream code receives values that are correct by
   construction. Doctests exist on exactly these constructors, because a doctest earns its keep
   where it documents a wire grammar.

4. **Unknown is not a bug to route around.** Adding record support to the compiler meant deleting
   `value_type` rather than repairing it, because a literal's checked type is the type it was
   _decoded against_ — re-deriving it from the value cannot be right for an empty list, a record,
   or an explicit null.

5. **Ports, not mocks.** Every I/O boundary is a trait with a `Default` unit-struct host adapter.
   No mocking crate is in the dependency tree.

6. **Missing facts and invalid facts are separate policy decisions.** The manifest declares them
   explicitly (`missing_facts`, `invalid_facts`), and the vocabulary's `presence` field distinguishes
   absent from null. These are not implementation details to be defaulted silently.

## User-facing surface

One binary, `rulery`, with nine subcommands: `check`, `compile`, `analyze`, `test`, `explain`,
`diff`, `render`, `lock`, `init`, plus `fmt`. Every command writes exactly one artifact to stdout
and resolves its exit status through the specification's exit-condition matrix
(`crates/cli/src/dispatch.rs`).

The human-readable explanation is the primary UI surface. Its canonical form is pinned as an
`insta` snapshot at
`crates/emit/src/snapshots/rulery_emit__human__tests__human_explain_matches_canonical_tool_library_text.snap`,
because it is the thing an operator actually reads.

## Constraints that shape the product

- **Offline only.** No network-capable dependency is permitted; `tests/workspace_scaffold.rs`
  asserts the resolved `Cargo.lock` closure contains none.
- **v0.1, pre-release.** No published crate yet, so no semver compatibility promise is in force.
- **No CI.** `cargo xtask verify` is the gate, which means it must be fast enough to actually run
  before every commit. The full gate is the slowest thing in the loop.
