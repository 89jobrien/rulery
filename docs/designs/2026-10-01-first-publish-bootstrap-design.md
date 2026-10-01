# First Publish Bootstrap

## Problem

`cargo rail release check --all --extended` reports `publish-dry-run` failing for 12 of
the 15 workspace members. Reproduced directly:

```
$ cargo package -p rulery --no-verify
    Packaging rulery v0.1.0 (/Users/joe/dev/rulery)
   Updating crates.io index
error: failed to prepare local package for uploading

Caused by:
  no matching package named `rulery-analysis` found
  location searched: crates.io index
  required by package `rulery v0.1.0 (/Users/joe/dev/rulery)`
```

`cargo package` resolves the packaged manifest against crates.io. The root facade crate
depends on `rulery-analysis` through `[workspace.dependencies]`, and no crate in the group is
published, so the dependency cannot be found. `--no-verify` does not help: the failure is in
preparing the package, before any build happens.

This is not a defect. It is the bootstrap problem every lockstep group has exactly once, and
the cost of it is that `--extended` cannot be used as a pre-publish gate for a first release —
it is a check that can only pass _after_ a release has already happened.

## What Is Already Correct

The sequencing is not the work. Verified:

- `release.version_groups` in `.config/rail.toml` is genuinely topological. The intra-workspace
  graph has exactly two leaves — `rulery-contracts` and `rulery-macros`, which have no
  `rulery-*` path dependencies — and both are the two crates that pass `publish-dry-run`. The
  remaining twelve fail for the same reason and in the same order.
- `mise run release:plan` plans the whole thing: `15 crate(s), 14 to publish, 15 tag(s),
0 skipped`, ending `rulery-analysis` → `rulery` → `rulery-cli`. The facade publishes
  second-to-last, after everything it depends on.
- Cargo-rail publishes in dependency order and writes durable state on interruption. The
  release-operations design records both, and `release:resume <state>` consumes that state.
  A stop partway through is therefore recoverable rather than fatal.

So the group is configured correctly and the tool sequences correctly. What is unknown is
timing.

## The One Unknown

`publish_delay = 5` seconds between crates.

Crate N+1 cannot be packaged until crate N appears in the crates.io index. If index
propagation takes longer than five seconds, the release stops when it reaches the first crate
whose dependency has not yet landed. Every subsequent lockstep release is unaffected, because
by then every dependency resolves from the index immediately.

**This has not been measured.** It cannot be measured without publishing, which is why the
first step below is a probe rather than the full release.

## Prerequisite: A Token

`cargo rail release check --all` fails before reaching the packaging stage, and
`~/.cargo/credentials.toml` has no `crates-io` section. Nothing below works until
`cargo publish` has a token for the `89jobrien` account:

```sh
cargo login --registry crates-io
```

Do not add the token to tracked configuration.

## Runbook

### 1. Merge the MSRV change and get to a clean main

`release:require_clean = true` and `release:gate` now runs `cargo xtask verify`, so a red gate
stops the release before it starts.

```sh
gh pr merge 1 --squash          # or however you want main to move
git checkout main && git pull
git status --short              # must be empty
```

### 2. Probe the index with the first leaf

Publish `rulery-contracts` on its own. It has no intra-workspace dependency, so it is exactly
the crate that can publish today, and it is first in the topological order — nothing is
published out of sequence by doing this.

```sh
cargo publish -p rulery-contracts --dry-run    # confirm it packages
time cargo publish -p rulery-contracts
```

Note how long the command takes _after_ it reports the upload. Cargo waits for the index to
reflect a newly published version before returning, so that duration is the propagation figure
this runbook needs. If it is comfortably under five seconds, the configured delay is fine and
you can skip step 3. If it is not, raise the delay.

### 3. Raise the delay if the probe says to

```toml
[release]
publish_delay = 5   # -> whatever the probe measured, with headroom
```

Fourteen crates at a 90-second delay is a 21-minute first publish. That is a one-time cost and
buys a single-shot release instead of a resume loop.

### 4. Publish `rulery-macros`

The other leaf. Same reason, same command.

### 5. Run the lockstep release

```sh
mise run release:now patch      # or release:prepare for the PR-first path
```

Cargo-rail publishes the remaining twelve in dependency order. `rulery-analysis` lands before
`rulery`, and `rulery` before `rulery-cli`.

### 6. If it stops partway

Read the state path cargo-rail prints and resume it:

```sh
mise run release:resume <printed-state-path>
```

Wait for the index to catch up on whatever it published before resuming. Re-running the
original release task is **not** the recovery path — it starts a second release.

## What Cannot Be Undone

A published crate version cannot be deleted. A version published in error can only be
`yank`ed, which leaves it resolvable by existing lockfiles and permanently occupies the version
number. Every step above is therefore irreversible, which is the reason this runbook probes
with a leaf first instead of letting the whole release discover the timing.

## Verifying Afterwards

```sh
cargo rail release check --all --extended     # publish-dry-run should now pass on all 14
```

`semver-checks` will also start running rather than reporting `skipped (Building ... current)`,
because there is finally a prior release to diff against. That is expected, and its findings
are the first real signal about whether the published API matches what was developed against.

Once this passes, `minibox` can take a real version dependency on `rulery` instead of a path
dependency, which is the last thing blocking it.

## Open Questions

- **`bench` is not an accepted commit type.** `unconventional_commits` is only `"warn"`, so this
  does not fail anything, but `91cc2a6` is reported as `unknown commit type 'bench'`. Probably
  not intentional.
- **Should `publish-dry-run` be advisory before a first release?** As configured, `release:check`
  cannot pass until a release exists. It is only run manually today, so nothing is blocked, but
  it will surprise whoever wires `release:check` into a preflight gate later.
