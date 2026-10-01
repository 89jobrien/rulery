# Active Context

Last updated 2026-09-30, after the session closeout.

## Current focus

No active work in `rulery`. The ten-gap programme and the four specification
`Required conformance gates` bullets are closed and pushed.

The one genuinely unfinished body of work is **`model-ingress-redaction`**, a
four-repo secret-redaction feature whose graph lives at
`/Users/joe/dev/.ctx/_WORKING_DIR/model-ingress-redaction/tasks.yaml`. Three of
six tasks are committed; the remaining three are all OpenCode plugin work.

## Current state

- `rulery` is checked out on **`fix/msrv-1.98`**, one commit ahead of `main`
  (`e4ef330`, not written by me). `main` is at `7f29a71` and identical to
  `github/main`. Decide whether that branch merges first or in parallel.
- Godmode graph: 125 done, 0 blocked, 0 pending, 0 running.
- `cargo xtask verify` passes all **eight** gates; 184 tests, 4 doctests, clippy clean.
- `model-ingress-redaction` task graph: 3 done (`obfsck-policy`,
  `personal-mcp-response`, `devloop-mcp-response`), 1 active
  (`opencode-lifecycle`), 2 pending (`opencode-fail-closed`, `lifecycle-canary`).
  `personal-mcp` is on `~/dev/personal-mcp` at `807ef7e`; `devloop` at `994e96d`.

## Cross-repo state touched this session

| Repo                  | Location                                            | State                                                               |
| --------------------- | --------------------------------------------------- | ------------------------------------------------------------------- |
| `rulery`              | `~/dev/rulery`                                      | on `fix/msrv-1.98`, +1 unpushed (not mine)                          |
| `personal-mcp`        | `~/dev/personal-mcp`                                | `feat/model-ingress-redaction`, clean, needs BAML client generated  |
| `devloop`             | `.ctx/_WORKING_DIR/model-ingress-redaction/devloop` | committed; 70 markdown files still uncommitted                      |
| `notfiles`            | `~/.notfiles`                                       | on `feat/secure-tailscale-key-sync`, heavy unrelated worktree churn |
| `godmode` memory bank | `.ctx/godmode/memory-bank/`                         | 3 paths; the tooling's own path bug is documented below             |

## Environment

- Shell is Nushell. The single largest source of wasted turns; see `mistakes.md`, now at 34 recorded
  occurrences. Treat the first `2>&1` of a session as a signal to re-read that entry.
- The pre-commit hook at `~/.config/git/hooks/pre-commit` runs `prettier --write` over staged YAML and
  markdown, then re-adds the result. It exits 0 and prints a green check, so byte-exact artifacts
  corrupt silently. Read it before committing any generated file; see `.prettierignore` in `rulery`.
- `whatidid` harvests `~/.claude/projects/**/*.jsonl`. Sessions run in opencode are invisible to it —
  verify there is a same-day transcript before invoking it, or summarise from git log instead.
- `personal-mcp` needs `bunx --yes @boundaryml/baml@0.219.0 generate --from baml_src` before it
  compiles; `scripts/baml-log-lab.sh` resolves an unpinned `bunx`, which resolves 0.226.2 and fails
  against the 0.219.0 generator target.
- Another agent works in these checkouts. Twice this session a worktree or branch appeared under me
  mid-review.

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
- Rejected a vocabulary root nested under another at resolution time (`ResolveError::ShadowedRoot`).
  The "prefer the most specific root match" alternative was proved a no-op: two roots can only both
  prefix a path if one prefixes the other, which the new check forbids.
- Global agent config: removed the standing `git add -A` mandate from the notfiles-managed
  `CLAUDE.md`, and made `daily-orchestration`'s fix-agent commit its own recorded paths rather than
  anything a global `git status` shows.

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
- Outside this repo: `personal-mcp`'s build depends on a gitignored generated client plus a `build.rs`
  stub that cannot satisfy `baml.rs`. Neither fixed — the correct fix is checking the generated client
  in, and that is a build-architecture decision for its owner.
- `godmode` 0.7.0 uses three different memory-bank paths (`status`/`inject`/`remind` read
  `.ctx/godmode/memory-bank/`, `init` writes `.ctx/memory-banking/`), while its skill document calls
  `.ctx/memory-bank/` canonical. The bank stays where the tool reads it; the fix belongs in godmode.

## Blockers

None in `rulery`. `init` and `fmt` are no longer blocked: both are specified, implemented, and now
have checked-in fixtures.

Release remains unconfirmed: the Cargo Rail file scope and the initial `0.1.0` / `v0.1.0` tag.

`model-ingress-redaction` has no hard blocker, but three tasks depend on a decision about whether the
OpenCode plugin is the right enforcement point at all — the design excludes it from crates.io and
names plugin removal as a standing bypass.

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
- A root nested under another root is rejected at vocabulary resolution rather than resolved by
  precedence. Once rejected, "prefer the longest matching root" is provably a no-op, so it is
  documented as a dependency rather than implemented.
- Global git discipline: work on a branch, merge explicitly, stage named paths, read the staged
  diff before every commit. Escalate to worktrees only when two agents genuinely run concurrently.
  Applied globally rather than per-repo because the failure it prevents is not repo-specific.

## Open questions

- The two `emit` JSON paths should perhaps share one key-ordering convention. Would be a wire-byte
  change, so it needs a deliberate decision.
- Confirm the initial release target as workspace version/tag `0.1.0` / `v0.1.0`, and whether to
  include or split the Cargo Rail release-planning files.
- Should `model-ingress-redaction` enforce at the OpenCode plugin, given the design already names
  plugin removal as a bypass? Personal MCP and DevLoop MCP redaction is done and independent.
- Should `personal-mcp` check in its generated BAML client? Doing so removes the missing-input branch
  entirely; the alternative is deleting the broken stub and letting the build fail loudly.
- `rulery` is on `fix/msrv-1.98` with an unpushed commit that is not mine. Merge order matters if
  session-closeout changes also land on a branch.
- `compact_str` for `FactSegment` / `StableId` internals was offered and not taken. It would cut
  allocation cost across the clone-heavy surfaces with no API-visible change, since only `as_str()`
  and `segments()` are exposed.
- `cargo-semver-checks` has no value until there is a published version to diff against. Revisit
  after the first crates.io release.
