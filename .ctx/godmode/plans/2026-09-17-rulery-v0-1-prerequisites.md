# Plan: Rulery v0.1 Recovered Prerequisites

## Goal

Restore the five contract tasks skipped by the initial ingest ID collision.

## Architecture

- Crate affected: `rulery-contracts`.
- Data flow: validated identifiers and scalar/source/value contracts feed hashing and locks.

## Tech Stack

- Rust 2024, serde, rust_decimal, jiff, blake3, and cargo nextest.

## Tasks

### Task 61: Validate stable and typed IDs

**Crate**: `rulery-contracts`
**File(s)**: `crates/contracts/Cargo.toml`, `crates/contracts/src/lib.rs`, `crates/contracts/src/ids.rs`
**Run**: `cargo nextest run -p rulery-contracts stable_ids_enforce_wire_grammar`

1. Write `stable_ids_enforce_wire_grammar` asserting legal boundaries and punctuation parse, invalid length/case/boundaries/ASCII fail, every typed ID deserializes through the check, and `community::rule` parses as a qualified rule ID.
2. Run the targeted nextest command and verify FAIL because the ID contracts do not exist.
3. Implement private `StableId`, all typed ID wrappers, `ReasonCode`, `FactSegment`, and `QualifiedRuleId` with checked constructors, accessors, display, ordering, hashing, serde, and typed errors.
4. Verify targeted GREEN; run `cargo fmt --all`; stage the three listed files; run `cargo clippy --workspace -- -D warnings`; run `cargo nextest run --workspace`.
5. Run `git branch --show-current`; stop if it prints `main`; otherwise commit with `git commit -m "feat(contracts): add validated stable identifiers"`.

### Task 62: Validate fact paths and boundary strings

**Crate**: `rulery-contracts`
**File(s)**: `crates/contracts/src/ids.rs`, `crates/contracts/src/source.rs`
**Run**: `cargo nextest run -p rulery-contracts fact_paths_and_locations_reject_invalid_segments`

1. Write `fact_paths_and_locations_reject_invalid_segments` asserting valid segmented fact paths and rejecting empty/doubled paths, unsafe source paths, malformed locations, invalid package paths, versions, requirements, language versions, and empty required strings.
2. Run the targeted nextest command and verify FAIL because path contracts do not exist.
3. Implement `FactPath`, `UnresolvedFactPath`, `SourcePath`, `NormalizedSourceLocation`, `PackagePath`, `Version`, `VersionRequirement`, and `LanguageVersion` with private validated fields.
4. Verify targeted GREEN; run `cargo fmt --all`; stage the two listed files; run `cargo clippy --workspace -- -D warnings`; run `cargo nextest run --workspace`.
5. Run `git branch --show-current`; stop if it prints `main`; otherwise commit with `git commit -m "feat(contracts): validate paths and package boundaries"`.

### Task 63: Implement exact decimal and temporal scalars

**Crate**: `rulery-contracts`
**File(s)**: `crates/contracts/Cargo.toml`, `crates/contracts/src/lib.rs`, `crates/contracts/src/scalar.rs`, `crates/contracts/src/time.rs`
**Run**: `cargo nextest run -p rulery-contracts exact_scalars_use_canonical_wire_forms`

1. Write `exact_scalars_use_canonical_wire_forms` asserting decimal normalization and rejection rules, Gregorian leap-year validation, signed nanosecond wire forms, nine-digit UTC display, leap-second rejection, and non-empty timezone identities.
2. Run the targeted nextest command and verify FAIL because exact scalar contracts do not exist.
3. Implement `DecimalValue`, `PolicyDate`, `UtcInstant`, `DurationValue`, `PolicyTimeZone`, `TimeZoneDatabaseIdentity`, `DateExpiryPolicy`, `TimeSemantics`, `TimeZoneError`, `Clock`, and `TimeZoneDatabase` with exact serde/display grammars.
4. Verify targeted GREEN; run `cargo fmt --all`; stage the four listed files; run `cargo clippy --workspace -- -D warnings`; run `cargo nextest run --workspace`.
5. Run `git branch --show-current`; stop if it prints `main`; otherwise commit with `git commit -m "feat(contracts): add exact policy scalar types"`.

### Task 64: Enforce source map and span invariants

**Crate**: `rulery-contracts`
**File(s)**: `crates/contracts/src/source.rs`
**Run**: `cargo nextest run -p rulery-contracts spans_are_half_open_utf8_intervals`

1. Write `spans_are_half_open_utf8_intervals` asserting accepted and rejected UTF-8 boundaries, lazy line/column derivation, unique keys, deterministic path ordering, and non-Copy source identities.
2. Run the targeted nextest command and verify FAIL because source map behavior does not exist.
3. Implement `SourceKey`, `Span`, `SourceFile`, `SourceMap`, `SourceDocument`, `SourceBundle`, `SourceIntegrity`, and `LoadedSourceBundle` with validated constructors and read-only accessors.
4. Verify targeted GREEN; run `cargo fmt --all`; stage the listed file; run `cargo clippy --workspace -- -D warnings`; run `cargo nextest run --workspace`.
5. Run `git branch --show-current`; stop if it prints `main`; otherwise commit with `git commit -m "feat(contracts): enforce source span invariants"`.

### Task 65: Model values facts actions and outcomes

**Crate**: `rulery-contracts`
**File(s)**: `crates/contracts/src/lib.rs`, `crates/contracts/src/value.rs`, `crates/contracts/src/outcome.rs`
**Run**: `cargo nextest run -p rulery-contracts outcomes_require_reasons_and_kind_specific_data`

1. Write `outcomes_require_reasons_and_kind_specific_data` asserting strict value round trips, distinct absent/null/valid/malformed facts, non-empty reasons and requested facts, and outcome-kind-specific destination/fact invariants.
2. Run the targeted nextest command and verify FAIL because fact and outcome contracts do not exist.
3. Implement `Value`, `ValueKind`, `EnumValue`, `CaseFacts`, `CaseFactsV1`, `FactState`, `FactValidationError`, action types, reason types, `RequiredFacts`, `OutcomeKind`, `OutcomeTemplate`, and `Outcome` with validated private collections.
4. Verify targeted GREEN; run `cargo fmt --all`; stage the three listed files; run `cargo clippy --workspace -- -D warnings`; run `cargo nextest run --workspace`.
5. Run `git branch --show-current`; stop if it prints `main`; otherwise commit with `git commit -m "feat(contracts): model facts actions and outcomes"`.
