# Active Context

Last updated 2026-09-30, after the gap-closeout programme landed on `main`.

## Current focus

No active work. The ten-gap programme and the four specification
`Required conformance gates` bullets that were unmet at its start are both
closed. The next session should pick up an open question below rather than
continue anything in flight.

## Current state

- `main` is `7f29a71`, identical to `github/main`, working tree clean. `feat/gap-closeout` still
  exists locally and is now identical to `main`; safe to delete.
- Godmode graph: 125 done, 0 blocked, 0 pending, 0 running (`.ctx/godmode/tasks.yaml`).
- `cargo xtask verify` passes all **eight** gates. `cargo nextest run --workspace --all-features`
  is 184 tests, 184 passing. `cargo test --workspace --all-features --doc` is 4 doctests.
  `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- The v0.1 requirement gate (`xtask::check_requirements`) exits 0, and
  `xtask conformance` now additionally fails if the specification's required-gate list loses a
  bullet. Deleting a requirement is a gate failure rather than a silent edit.

## Completed recently

The 2026-09-30 session, 33 commits, +7,927/−1,506 across 75 files:

- gap1–gap10 closed. Latent defects found along the way, none of them on the plan:
  - `rulery-analysis` text-partition domains were unconditionally incomplete, so any text-typed
    vocabulary field blocked completeness and `analyze` was effectively never complete.
  - A record-typed path could be assigned `Null`, hitting `CaseFactsError::PathConflict` and
    voiding an entire decision.
  - `StructuralChange::ImportIntegrity` accompanied every edited package, masking all 600 diff
    cells and making `ReasonOnly`/`PrecedenceOnly` unreachable.
  - `rulery-compiler` collapsed record-typed fields to `CheckedType::Text`, so a record compared
    against text and typechecked. Fixed by adding `CheckedType::Record` and deleting `value_type`,
    which also repaired two unrelated latent rejections.
  - `xtask` ran clippy without `--all-targets`, so five integration suites had never been linted.
- Property tests for the four-valued truth algebra and canonical hashing; four doctests on the
  validating constructors; a new `Doctest` verify gate.
- `insta` snapshots for the `rulery-emit` renderers, including the canonical tool-library explanation.
- `criterion` benchmark for partition scaling, which exposed two performance defects worth 89× on the
  pathological case (a `BTreeMap` clone chain, and `sort_by_key` recomputing its key per comparison).
- The two `examples/` fixtures the specification requires, compared byte for byte by process tests,
  plus a repo-local `.prettierignore` so the pre-commit hook stops rewriting them.
- Renamed-dependency macro hygiene fixture, wired into `xtask conformance` as a real gate.
- `LICENSE-MIT` and `LICENSE-APACHE` added, with a test that derives its expectation from
  `[workspace.package]`.

## Known deviations

Three of the four from 2026-09-21 are now closed:

- ~~`ProductionApplication` hard-codes `JiffTimeZoneDatabase`~~ — fixed; the database is injected
  and the instant is threaded explicitly through `analyze` and `diff`.
- ~~No stdout artifact when a failure precedes artifact production~~ — fixed; `pre_artifact` emits a
  zero-entry envelope in machine formats and always explains itself on stderr.
- ~~`Diagnostic` confidence and labels are private~~ — fixed; three accessors added and the CLI's
  three `serde_json` round trips deleted.

Still open:

- A `datetime` literal in authored source is read with the signed-nanosecond grammar. The
  specification says only that date-time literals must be quoted, so the authored form is ambiguous
  and was left alone. Unchanged by this session.
- One unpairable cell in the analyzer still silently voids a whole decision (`extract.rs`). Left
  deliberately: with the poisonous null-record cell removed there is no honest failing test for it.
  Fixing it properly needs a fixture that produces an unpairable cell through some other route.
- The two `emit` JSON paths order keys differently — `render` sorts (it round-trips through
  `serde_json::Value`, whose maps are `BTreeMap`), `render_scenario_results` uses struct declaration
  order. Both are deterministic so the contract holds, but the two machine formats do not share one
  convention. Documented by snapshot rather than changed, since unifying them moves wire bytes.

## Blockers

None. `init` and `fmt` are no longer blocked: both are specified, implemented, and now have
checked-in fixtures.

Release remains unconfirmed: the Cargo Rail file scope and the initial `0.1.0` / `v0.1.0` tag.

## Environment

- Shell is Nushell. This is the single largest source of wasted turns — see the mistake ledger. Write
  the correct form first; every POSIX-ism is a parse error that costs a turn.
- The pre-commit hook at `~/.config/git/hooks/pre-commit` runs `prettier --write` over staged YAML and
  markdown, then re-adds the result. It exits 0 and prints a green check, so byte-exact artifacts
  corrupt silently. Read it before committing any generated file.
- Another agent has been active in this checkout at times, producing cargo file-lock contention.
- The memory bank lives at `.ctx/godmode/memory-bank/`, **not** the `.ctx/memory-bank/` that the
  memory-banking skill documents as canonical. `godmode memory-banking status`, `inject`, and
  `remind` all read the former, while `init` writes to a _third_ path, `.ctx/memory-banking/`.
  Installed version is 0.7.0. Migrating to the documented path would silently stop the SessionStart
  and Stop hooks from injecting anything, so the bank stays where the tool actually reads it. The
  fix belongs in godmode, not in this repo.

## Decisions

- `analyze` and `diff` fail with a typed error rather than returning a report that was never derived.
- The `ir` re-export of the vocabulary surface stays limited to schema types; the engine takes a
  direct modelled edge for validation behavior rather than widening that re-export.
- CLI policy layers are reused rather than replaced.
- The truth algebra's two absorption orders are **not** unified. They disagree on exactly one pair
  (`Unknown` vs `Invalid`), and that disagreement is why absorption and both distributivity laws
  fail. The specification's tables define the algebra, so this is recorded as law rather than fixed.
- `CheckedType::Record` carries a type identity, not a structural field map. A record may omit its
  optional fields, so shape comparison would reject valid literals.
- A literal's checked type is the type it was decoded against, not one re-derived from its value.
  `value_type` was deleted rather than repaired.
- `rstest` declined: `xtask`'s `collect_test_names` reads literal `fn <name>(` source lines because
  the specification's traceability tables name tests by source identifier, so macro-generated test
  names cannot be referenced. The repo also mandates table-driven loops over parametrised tests.

## Open questions

- The two `emit` JSON paths should perhaps share one key-ordering convention. Would be a wire-byte
  change, so it needs a deliberate decision.
- Confirm the initial release target as workspace version/tag `0.1.0` / `v0.1.0`, and whether to
  include or split the Cargo Rail release-planning files.
- `compact_str` for `FactSegment` / `StableId` internals was offered and not taken. It would cut
  allocation cost across the clone-heavy surfaces with no API-visible change, since only `as_str()`
  and `segments()` are exposed.
- `cargo-semver-checks` has no value until there is a published version to diff against. Revisit
  after the first crates.io release.
