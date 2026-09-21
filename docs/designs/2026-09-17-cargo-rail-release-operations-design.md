# Design: Cargo-Rail Release Operations Console

## Goal

Add a repository-level mise operations console and cargo-rail policy that releases every
publishable Rulery crate at one shared version without adding release logic to `xtask`.

## Approved Approach

Use the approved **Full Rail Operations Console** approach: mise exposes the complete release
lifecycle while cargo-rail owns versioning, changelog generation, release commits, tags, pushes,
dependency-ordered crates.io publication, forge releases, and interrupted-release recovery.

The normal path is release-PR preparation followed by finalization after merge. An explicit
immediate-release task remains available. Patch is the default bump, with minor and major as
validated overrides.

## Context Map

### Files to Modify

| File                | Purpose                         | Change Needed                                               |
| ------------------- | ------------------------------- | ----------------------------------------------------------- |
| `mise.toml`         | Human-facing operations console | Add release validation, mutation, and recovery tasks        |
| `.config/rail.toml` | Cargo-rail policy boundary      | Define lockstep release, changelog, tag, and publish policy |
| `CHANGELOG.md`      | Workspace release history       | Add the initial workspace changelog                         |

This design document is an architecture artifact, not part of the implementation surface.

### Dependencies That Remain Unchanged

| File                                                 | Relationship                                               |
| ---------------------------------------------------- | ---------------------------------------------------------- |
| `Cargo.toml`                                         | Defines the shared version and versioned path dependencies |
| `Cargo.lock`                                         | Updated by cargo-rail during a real release                |
| `xtask/Cargo.toml`                                   | Already declares `publish = false`                         |
| `xtask/src/model.rs`                                 | Defines the current 15-package workspace graph             |
| `xtask/src/verify.rs`                                | Establishes the existing Cargo quality-gate commands       |
| `.cargo/config.toml`                                 | Retains only the existing `cargo xtask` alias              |
| `tests/workspace_scaffold.rs`                        | Verifies the approved crate topology                       |
| `docs/designs/2026-09-17-xtask-automation-design.md` | Keeps release automation outside `xtask`                   |

### Publishable Version Group

The `rulery` cargo-rail version group contains these 14 packages:

1. `rulery-contracts`
2. `rulery-diagnostics`
3. `rulery-syntax`
4. `rulery-vocabulary`
5. `rulery-ir`
6. `rulery-compiler`
7. `rulery-engine`
8. `rulery-analysis`
9. `rulery-scenarios`
10. `rulery-emit`
11. `rulery-store`
12. `rulery-macros`
13. `rulery`
14. `rulery-cli`

`xtask` remains a workspace member but is excluded from publication and changelog attribution.
Cargo-rail reads dependency edges from Cargo metadata and derives publication order rather than
maintaining a second hand-written order in mise; `xtask/src/model.rs` independently mirrors those
approved boundaries for repository validation.

### Test Coverage

| Validation                                          | Covers                                                             |
| --------------------------------------------------- | ------------------------------------------------------------------ |
| `mise tasks validate`                               | Task schema and usage declarations                                 |
| `mise tasks ls --local`                             | Expected operations-console task discovery                         |
| `cargo rail config validate --strict`               | Configuration keys and cross-field constraints                     |
| `cargo rail release check --all --extended`         | Publishability, package dry-runs, MSRV, and advisory semver checks |
| `cargo rail release run --all --bump patch --check` | Non-mutating release planning                                      |
| Existing Cargo gates                                | Workspace formatting, linting, and tests                           |

No existing Rust test exercises mise or cargo-rail configuration. Validation is command-level;
no production Rust API changes require unit or integration tests.

### Reference Patterns

| Reference                                    | Pattern Used                                                              |
| -------------------------------------------- | ------------------------------------------------------------------------- |
| `cargo rail release init --check`            | Current workspace discovery and config schema                             |
| `/Users/joe/dev/taskit/.config/rail.toml`    | Lockstep workspace release policy                                         |
| `/Users/joe/dev/taskit/mise.toml`            | Namespaced release task convention                                        |
| `/Users/joe/dev/looprs/.config/rail.toml`    | Explicit non-publishable `xtask` policy                                   |
| Cargo-rail `v0.17.3` configuration reference | Version groups, workspace changelog, push, signing, and forge constraints |
| Mise task-argument reference                 | `usage` declarations instead of deprecated template arguments             |

### Risk

- [ ] Public Rust API change: no.
- [ ] Serialization or persisted runtime format change: no.
- [x] CLI surface change: new mise tasks become the repository release interface.
- [x] External side effects: release tasks can commit, push, tag, publish crates, and create forge releases.
- [x] External tool drift: cargo-rail is intentionally not provisioned or pinned by mise.
- [x] Existing dirty worktree: cargo-rail readiness and release commands will reject it until unrelated changes are resolved.

## Ownership

- **Owner**: repository-root release infrastructure.
- **Affected crates**: no crate imports new code; all 14 publishable packages participate as Cargo metadata.
- **Excluded crate**: `xtask` remains non-publishable.
- **New crate**: none.

## Operational API

No Rust traits, types, functions, or public crate APIs are added or changed. The public operational
surface consists of these mise tasks:

| Task                     | Contract                                                                                                      |
| ------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `release:gate`           | Run `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, then `cargo nextest run --workspace`        |
| `release:check`          | Strictly validate rail config and run extended readiness checks for all publishable crates                    |
| `release:plan [bump]`    | Preview the all-crate release; accept `patch`, `minor`, or `major`, defaulting to `patch`                     |
| `release:prepare [bump]` | From `main`, run gates and readiness checks, then create and push a cargo-rail release PR                     |
| `release:finalize`       | From `main` after merge, rerun gates and readiness checks, then tag, push, publish, and create forge releases |
| `release:now [bump]`     | From `main`, run gates and readiness checks, then perform the immediate full release                          |
| `release:resume <state>` | Resume the exact cargo-rail durable state file without starting a second release                              |
| `release:abort <state>`  | Interactively restore an active release that has not reached remote side effects                              |

Mise `usage` declarations validate bump choices and required state paths. The plan task preserves
cargo-rail's semantic exit codes: both "no release changes" and "valid pending release plan" are
successful previews, while configuration or planning failures remain failures.

## Branch And Confirmation Policy

Every commit-producing mise task runs `git branch --show-current` before invoking cargo-rail and
requires `main`. This is the explicitly approved exception to the normal rule that stops commits on
`main`; it applies only to cargo-rail-owned release commits from `release:prepare` and `release:now`.

The exception does not permit arbitrary commits on `main`. Implementation commits for this feature
and all ordinary commits remain prohibited on `main`.

Mutating tasks retain cargo-rail's native interactive safety gate and never pass `--yes`.
No task uses `--no-verify`, bypasses Git hooks, force-pushes, or deletes tags. A signing failure is
reported as a request to unlock 1Password; Git configuration is never changed.

## Cargo-Rail Policy

The project configuration uses cargo-rail's current release schema with these decisions:

- `require_clean = true`.
- `publish_delay = 5` seconds.
- `push = true` so release commits and tags reach `origin` before publication.
- `create_github_release = true` with the GitHub forge selected explicitly.
- `sign_tags = true`.
- `require_changelog_entries = false` and `require_release_notes = false` during initial adoption.
- Unconventional commits and semver-check findings are warnings rather than blockers.
- Change files are not required.
- The `rulery` version group contains all 14 publishable packages.
- The changelog path is workspace-relative `CHANGELOG.md` with ASCII-only headings.
- `xtask` has `publish = false` and skips changelog generation.

Cargo-rail creates one tag per released crate. The tag format is
`{crate}-{prefix}{version}`, producing names such as `rulery-engine-v0.1.1`. This intentionally
refines the brainstorm's provisional single `v{version}` tag: cargo-rail `v0.17.3` creates tags and
forge releases per crate, so a shared tag name would collide during an all-crate release.

Because forge release creation is enabled, one GitHub release is created for each crate tag. A
single umbrella GitHub release is outside cargo-rail's direct all-crate model and is not added as
custom automation.

## Data Flow

1. Source: the operator selects a mise release task and optionally supplies a validated bump level or state path.
2. Guard: mise checks the branch where applicable and runs the required Cargo gates before release mutation.
3. Validate: cargo-rail loads `.config/rail.toml` and Cargo metadata, filters out `xtask`, and validates every publishable package.
4. Plan: cargo-rail expands the lockstep version group and derives the dependency-safe release plan.
5. Prepare: the PR-first path writes version and changelog changes to a release branch and opens the release PR.
6. Finalize: after merge, cargo-rail creates signed per-crate tags, atomically pushes release refs, publishes crates in dependency order, and publishes forge releases.
7. Recover: an interrupted release writes durable state consumed only by `release:resume` or `release:abort`.

## Hexagonal Boundaries

No new in-repository Rust port or adapter is justified. Mise is the human-facing orchestration
boundary, `.config/rail.toml` is the policy input, and cargo-rail is the external release adapter
for Cargo, Git, `gh`, crates.io, and GitHub release operations.

Credentials remain outside tracked configuration. Cargo-rail reads Cargo's configured credentials
or `CARGO_REGISTRY_TOKEN`; mise does not resolve, store, print, or transform secrets.

## Error Handling And Recovery

- Task or gate failure stops the sequence immediately; mise adds no retry loop.
- Readiness failures make no release mutation.
- Release-plan validation distinguishes expected pending changes from actual command errors.
- Publish or push interruption is resumed from cargo-rail's printed state path; rerunning the original release task is not the recovery path.
- Abort is allowed only while cargo-rail reports that no remote side effect has occurred.
- The three-attempt rule applies during implementation validation; after three failures of the same gate or fix, work stops with the root cause reported.

## Validation Strategy

Implementation validation runs in this order:

1. `mise tasks validate`.
2. `mise tasks ls --local` and verify the expected release task names.
3. `cargo rail config validate --strict`.
4. `cargo rail release check --all --extended` on a clean tree.
5. `cargo rail release run --all --bump patch --check` and confirm that no files, commits, tags, or remote state change.
6. `cargo fmt --all`.
7. Re-stage the implementation files.
8. `cargo clippy --workspace -- -D warnings`.
9. `cargo nextest run --workspace`.

No mutating release task is executed during implementation validation.

## Integration Points

- Cargo metadata remains the single source of package names, dependency edges, and publishability.
- The root workspace version remains the source inherited by every member.
- Existing versioned path dependencies remain suitable for crates.io publication.
- Existing `cargo xtask verify` remains unchanged and independent of release orchestration.
- New workspace members must be deliberately added to the cargo-rail version group or marked non-publishable.

## Out Of Scope

- New xtask commands or changes to any Rust crate.
- CI workflows, trusted publishing, or GitHub Actions changes.
- Mise tool provisioning or cargo-rail version pinning.
- Independent crate versions, affected-crate releases, or selective publishing.
- crates.io credential management or 1Password integration.
- Binary archives, checksums, SBOMs, attestations, or other release assets.
- A single umbrella tag or GitHub release for the workspace.
- Automatic retries, force pushes, hook bypasses, or rollback after remote publication.

## Doublecheck

- [x] Release ownership is repository-level; no crate dependency is introduced.
- [x] No circular dependency can be introduced because no Rust dependency changes.
- [x] The API surface is limited to approved mise tasks and cargo-rail policy.
- [x] External operations remain delegated to cargo-rail rather than reimplemented in `xtask`.
- [x] Task names follow the repository-wide namespaced mise convention.
- [x] `--all` filters non-publishable packages during cargo-rail readiness checks.
- [x] Per-crate tags avoid the collision caused by one tag template across 14 release plans.
- [x] `push = true` satisfies cargo-rail's forge-release precondition.
- [x] Mise uses current `usage` declarations rather than deprecated Tera argument functions.

## Risk Summary

- [ ] Breaking API changes: no.
- [x] New external dependency: yes, cargo-rail is an operator-installed CLI rather than a Cargo dependency.
- [ ] Feature flag required: no.
- [x] Irreversible operation: crates.io publication; guarded by clean-tree, branch, gate, readiness, plan, and interactive confirmation checks.
- [x] Remote mutation: release commits, tags, PR branches, and GitHub releases.
- [x] Tool compatibility: cargo-rail is pre-1.0 and unpinned by explicit user choice; configuration must be revalidated after upgrades.
