# Plan: Cargo-Rail Release Operations Console

## Goal

Add a repository-level mise operations console and cargo-rail policy that releases all 14
publishable Rulery packages at one shared version while keeping release mechanics out of `xtask`.

## Context Map

### Files to Modify

| File                | Purpose                                                                 |
| ------------------- | ----------------------------------------------------------------------- |
| `CHANGELOG.md`      | Workspace-relative release history maintained by cargo-rail             |
| `.config/rail.toml` | Lockstep version, changelog, tag, signing, push, and publication policy |
| `mise.toml`         | Validation, planning, release, finalization, and recovery task surface  |

### Dependencies That Remain Unchanged

| File                          | Relationship                                                               |
| ----------------------------- | -------------------------------------------------------------------------- |
| `Cargo.toml`                  | Owns the shared workspace version and versioned internal path dependencies |
| `Cargo.lock`                  | Is updated only during a real cargo-rail release                           |
| `xtask/Cargo.toml`            | Already marks `xtask` non-publishable                                      |
| `xtask/src/model.rs`          | Mirrors the 15-package workspace graph for repository validation           |
| `xtask/src/verify.rs`         | Establishes the required Cargo gate commands                               |
| `.cargo/config.toml`          | Keeps the existing `cargo xtask` alias; release commands are mise tasks    |
| `tests/workspace_scaffold.rs` | Verifies crate topology but does not cover release configuration           |

### Existing Coverage And Gaps

- No existing test covers `mise.toml`, `.config/rail.toml`, or `CHANGELOG.md`.
- Configuration work is tested through failing/passing CLI contract checks rather than Rust tests.
- A final clean-tree cargo-rail readiness check must run after all implementation commits because
  `[release].require_clean = true` intentionally rejects the in-progress configuration edits.
- Extended readiness is a post-bootstrap audit: Cargo dry-runs cannot package a dependent Rulery
  crate until its versioned internal dependencies exist on crates.io.

### Risks

- No Rust API, serialization format, feature flag, or crate dependency changes.
- The new mise surface can create commits, push branches and tags, publish immutable crates.io
  versions, and create per-crate GitHub releases.
- Cargo-rail is an operator-installed, pre-1.0 CLI and remains unpinned by explicit design choice.
- The worktree already contains unrelated changes; stage and commit only each task's listed files.

## Architecture

- **Crates affected**: no Rust crate source changes; 14 publishable packages participate through
  Cargo metadata, while `xtask` remains excluded.
- **New traits/types**: none.
- **Ownership**: repository-root release infrastructure owns all new files.
- **Operational API**: `release:gate`, `release:check`, `release:plan`, `release:prepare`,
  `release:finalize`, `release:now`, `release:resume`, and `release:abort`.
- **Data flow**: mise task and validated arguments -> Cargo gates and branch guard -> cargo-rail
  workspace plan -> release PR or immediate release -> signed per-crate tags -> dependency-ordered
  crates.io publication -> per-crate GitHub releases.
- **Boundary**: mise is the operator-facing orchestration boundary, `.config/rail.toml` is policy,
  and cargo-rail is the external adapter for Cargo, Git, GitHub CLI, crates.io, and GitHub releases.
- **Recovery**: interrupted releases continue only from cargo-rail durable state through
  `release:resume`; `release:abort` remains interactive and is valid only before remote effects.

## Tech Stack

- Rust edition 2024 with workspace MSRV 1.85.
- Cargo workspace metadata and the existing versioned path dependencies.
- cargo-rail 0.17.3 command contracts; mise does not install or pin cargo-rail.
- mise 2026.9 task configuration with current `usage` declarations.
- Nushell for the one task that normalizes cargo-rail plan exit codes.
- Git and GitHub CLI as cargo-rail-managed external tools.
- No new Cargo dependencies, feature flags, CI workflows, or credential files.

## Tasks

### Task 1: Initialize Workspace Changelog

**Crate**: `workspace`
**File(s)**: `CHANGELOG.md`
**Run**: `test -f CHANGELOG.md`

1. Write the failing contract check:

   ```sh
   test -f CHANGELOG.md
   ```

2. Verify FAIL:

   ```sh
   test -f CHANGELOG.md
   ```

   Expected: exit status 1 because the workspace changelog does not exist.

3. Implement `CHANGELOG.md` exactly:

   ```markdown
   # Changelog

   All notable changes to the Rulery workspace are documented in this file.

   ## [Unreleased]
   ```

4. Verify GREEN:

   ```sh
   test -f CHANGELOG.md
   cargo fmt --all
   git add CHANGELOG.md
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: the file check and every Cargo gate exit 0. Re-stage after formatting as shown.

5. Commit:

   ```sh
   git branch --show-current
   ```

   Verify the output is not `main`; stop immediately if it is. Stage only `CHANGELOG.md`, then
   commit without bypassing hooks:

   ```sh
   git add CHANGELOG.md
   git commit -m "docs(release): initialize workspace changelog"
   ```

### Task 2: Configure Cargo-Rail Release Policy

**Crate**: `workspace`
**File(s)**: `.config/rail.toml`
**Run**: `cargo rail config validate --strict`

1. Write the failing contract check:

   ```sh
   cargo rail config validate --strict
   ```

2. Verify FAIL:

   ```sh
   cargo rail config validate --strict
   ```

   Expected: nonzero exit with no project cargo-rail configuration found.

3. Implement `.config/rail.toml` exactly:

   ```toml
   targets = []

   [release]
   tag_prefix = "v"
   tag_format = "{crate}-{prefix}{version}"
   require_clean = true
   publish_delay = 5
   create_github_release = true
   forge = "github"
   push = true
   sign_tags = true
   require_changelog_entries = false
   require_release_notes = false
   conventional_commits = true
   unconventional_commits = "warn"
   semver_check = "warn"
   change_files = false
   change_dir = ".changes"
   release_notes_dir = ".release-notes"

   [release.version_groups]
   rulery = [
     "rulery-contracts",
     "rulery-diagnostics",
     "rulery-syntax",
     "rulery-vocabulary",
     "rulery-ir",
     "rulery-compiler",
     "rulery-engine",
     "rulery-analysis",
     "rulery-scenarios",
     "rulery-emit",
     "rulery-store",
     "rulery-macros",
     "rulery",
     "rulery-cli",
   ]

   [release.changelog]
   path = "CHANGELOG.md"
   relative_to = "workspace"
   categories = ["added", "changed", "deprecated", "removed", "fixed", "security"]
   emoji = false
   sort = true
   include_authors = false
   include_commit_links = true

   [crates.xtask.release]
   publish = false

   [crates.xtask.changelog]
   skip = true
   ```

4. Verify GREEN:

   ```sh
   cargo rail config validate --strict
   cargo fmt --all
   git add .config/rail.toml
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: strict config validation and every Cargo gate exit 0. Do not run a mutating release.

5. Commit:

   ```sh
   git branch --show-current
   ```

   Verify the output is not `main`; stop immediately if it is. Stage only the rail config, then
   commit without bypassing hooks:

   ```sh
   git add .config/rail.toml
   git commit -m "build(release): configure cargo-rail policy"
   ```

### Task 3: Add Release Validation And Planning Tasks

**Crate**: `workspace`
**File(s)**: `mise.toml`
**Run**: `mise run release:plan --help`

1. Write the failing task-contract check:

   ```sh
   mise run release:plan --help
   ```

2. Verify FAIL:

   ```sh
   mise run release:plan --help
   ```

   Expected: nonzero exit because `mise.toml` and `release:plan` do not exist.

3. Implement `mise.toml` with the non-mutating surface and reusable Cargo gate:

   ```toml
   [tasks."release:gate"]
   description = "Run release Cargo quality gates"
   run = [
     "cargo fmt --all",
     "cargo clippy --workspace -- -D warnings",
     "cargo nextest run --workspace",
   ]

   [tasks."release:check"]
   description = "Run post-bootstrap extended checks for all publishable crates"
   run = [
     "cargo rail config validate --strict",
     "cargo rail release check --all --extended",
   ]

   [tasks."release:plan"]
   description = "Preview the lockstep workspace release"
   usage = '''
   arg "[bump]" help="Shared version bump" default="patch" {
     choices "patch" "minor" "major"
   }
   '''
   run = '''
   #!/usr/bin/env nu
   let result = do {
     cargo rail release run --all --bump $env.usage_bump --check
   } | complete

   if not ($result.stdout | is-empty) {
     print $result.stdout
   }
   if not ($result.stderr | is-empty) {
     print --stderr $result.stderr
   }

   if $result.exit_code > 1 {
     exit $result.exit_code
   }
   '''
   ```

   Exit 0 means no release-worthy changes. Exit 1 means cargo-rail found a valid pending release
   plan. The task normalizes both outcomes to success and preserves all exit codes above 1.

4. Verify GREEN:

   ```sh
   nu -c 'let result = {stdout: "", stderr: "", exit_code: 1}; if $result.exit_code > 1 { exit $result.exit_code }'
   mise tasks validate
   mise run release:plan --help
   cargo fmt --all
   git add mise.toml
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: Nushell parses the exit-code logic and treats cargo-rail's pending-plan status as
   success, mise validates the task schema and generated help, and every Cargo gate exits 0.

5. Commit:

   ```sh
   git branch --show-current
   ```

   Verify the output is not `main`; stop immediately if it is. Re-stage after formatting, then
   commit only the mise file:

   ```sh
   git add mise.toml
   git commit -m "build(release): add validation and planning tasks"
   ```

### Task 4: Add Release Execution Tasks

**Crate**: `workspace`
**File(s)**: `mise.toml`
**Run**: `mise run release:prepare --help`

1. Write the failing task-contract check:

   ```sh
   mise run release:prepare --help
   mise run release:finalize --help
   mise run release:now --help
   ```

2. Verify FAIL:

   ```sh
   mise run release:prepare --help
   ```

   Expected: nonzero exit because the mutating release tasks do not exist yet.

3. Implement the execution surface by appending these tasks to `mise.toml` exactly:

   ```toml
   [tasks."release:require-main"]
   description = "Require the default branch for cargo-rail mutations"
   hide = true
   run = '''
   #!/usr/bin/env nu
   let branch = (git branch --show-current | str trim)
   print $branch
   if $branch != "main" {
     error make { msg: $"release task requires main; current branch: ($branch)" }
   }
   '''

   [tasks."release:preflight"]
   description = "Run release gates and readiness checks"
   hide = true
   run = [
     "mise run release:gate",
     "cargo rail config validate --strict",
     "cargo rail release check --all",
   ]

   [tasks."release:prepare"]
   description = "Prepare and open the lockstep release PR"
   depends = ["release:require-main", "release:preflight"]
   usage = '''
   arg "[bump]" help="Shared version bump" default="patch" {
     choices "patch" "minor" "major"
   }
   '''
   run = 'cargo rail release run --all --bump "${usage_bump?}" --pr'

   [tasks."release:finalize"]
   description = "Finalize the merged release PR"
   depends = ["release:require-main", "release:preflight"]
   run = "cargo rail release finalize --all"

   [tasks."release:now"]
   description = "Immediately release the lockstep workspace"
   depends = ["release:require-main", "release:preflight"]
   usage = '''
   arg "[bump]" help="Shared version bump" default="patch" {
     choices "patch" "minor" "major"
   }
   '''
   run = 'cargo rail release run --all --bump "${usage_bump?}"'
   ```

   The `main` requirement is the explicitly approved cargo-rail release exception. It does not
   relax the implementation-commit rule: ordinary commits still stop on `main`. Do not add `--yes`
   or `--no-verify`; cargo-rail's native confirmation and hooks remain active.

4. Verify GREEN without executing a release:

   ```sh
   mise tasks validate
   mise run release:prepare --help
   mise run release:finalize --help
   mise run release:now --help
   cargo fmt --all
   git add mise.toml
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: all help and validation commands exit 0 without invoking dependencies or mutating
   release state; every Cargo gate exits 0.

5. Commit:

   ```sh
   git branch --show-current
   ```

   Verify the output is not `main`; stop immediately if it is. Re-stage the formatted task file,
   then commit without bypassing hooks:

   ```sh
   git add mise.toml
   git commit -m "build(release): add release execution tasks"
   ```

### Task 5: Add Interrupted-Release Recovery Tasks

**Crate**: `workspace`
**File(s)**: `mise.toml`
**Run**: `mise run release:resume --help`

1. Write the failing task-contract check:

   ```sh
   mise run release:resume --help
   mise run release:abort --help
   ```

2. Verify FAIL:

   ```sh
   mise run release:resume --help
   ```

   Expected: nonzero exit because recovery tasks do not exist yet.

3. Implement recovery by appending these tasks to `mise.toml` exactly:

   ```toml
   [tasks."release:resume"]
   description = "Resume an interrupted cargo-rail release"
   usage = 'arg "<state>" help="Cargo-rail durable state file"'
   run = 'cargo rail release resume "${usage_state?}"'

   [tasks."release:abort"]
   description = "Abort a cargo-rail release before remote effects"
   usage = 'arg "<state>" help="Cargo-rail durable state file"'
   run = 'cargo rail release abort "${usage_state?}"'
   ```

   Recovery tasks do not rerun gates or start a second release. They delegate the supplied state
   file directly to cargo-rail, and abort retains cargo-rail's interactive safety gate.

4. Verify GREEN without a state file or release mutation:

   ```sh
   mise tasks validate
   mise tasks ls --local
   mise run release:resume --help
   mise run release:abort --help
   cargo fmt --all
   git add mise.toml
   cargo clippy --workspace -- -D warnings
   cargo nextest run --workspace
   ```

   Expected: task validation, task discovery, help generation, and every Cargo gate exit 0. Confirm
   the list contains `release:gate`, `release:check`, `release:plan`, `release:prepare`,
   `release:finalize`, `release:now`, `release:resume`, and `release:abort`.

5. Commit:

   ```sh
   git branch --show-current
   ```

   Verify the output is not `main`; stop immediately if it is. Re-stage after formatting, then
   commit only the mise file:

   ```sh
   git add mise.toml
   git commit -m "build(release): add release recovery tasks"
   ```

## Final Clean-Tree Verification

After all five task commits, verify the worktree is fully clean. If unrelated pre-existing user
changes remain, do not stash, revert, stage, or commit them; report clean-tree release validation as
blocked until their owner resolves them. Once the worktree is clean, run:

```sh
cargo rail config validate --strict
cargo rail release check --all
cargo rail release run --all --bump patch --check
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo nextest run --workspace
```

The plan command may exit 1 to signal a valid pending release; that is expected and must not be
treated as a failed design. `mise run release:check` intentionally remains a post-bootstrap audit:
before the first publication it will report missing internal crates.io dependencies during Cargo
dry-runs. Immediately confirm the plan dry-run created no commit, tag, or remote change with
`git status --short --branch` and `git log --oneline -3`. Do not execute `release:prepare`,
`release:finalize`, `release:now`, `release:resume`, or `release:abort` during implementation.

If the same test or fix fails three times, stop and report the root cause. If commit or tag signing
fails, ask the user to unlock 1Password and do not modify Git configuration.

## Introspection Checklist

- [x] `CHANGELOG.md` initialization maps to Task 1.
- [x] Lockstep package policy, publication order inputs, signed per-crate tags, GitHub releases,
      workspace changelog, and `xtask` exclusion map to Task 2.
- [x] Cargo gates, post-bootstrap extended checks, and patch-default planning map to Task 3.
- [x] PR-first preparation, finalization, immediate release, native prompts, and the approved `main`
      exception map to Task 4.
- [x] Durable-state resume and interactive abort map to Task 5.
- [x] No task changes Rust APIs, CI workflows, tool provisioning, credentials, or binary assets.
- [x] Names are consistent with the approved design and across all task definitions.
- [x] Every task contains a failing contract check, explicit FAIL verification, exact implementation,
      GREEN verification, required Cargo gates, branch verification, re-staging, and a conventional commit.
- [x] Each task is scoped to one 2-5 minute configuration change, excluding gate runtime.
- [x] No placeholders, inferred paths, vague implementation directives, or hook bypasses remain.
