# Project Brief

## What Rulery is

A local-first Rust **compiler and evaluator for structured operational rules**. It turns YAML rule
packages into deterministic intermediate representations, evaluates decisions with explicit
four-valued truth semantics, records reproducible traces, runs scenarios, performs static analysis,
and renders machine- and human-readable artifacts.

Source: `README.md:3-7`.

The normative target is `docs/specification.md` (v0.1 core specification, 3489 lines). A feature
described there is **not** assumed complete unless it is also implemented in the current source.
Source: `README.md:24-25`.

## Who it is for

People who need operational decisions — eligibility, approvals, triage, access — to be expressed as
a reviewable rulebook rather than scattered code, and who need to be able to audit why a decision
came out the way it did.

Rulery is deliberately **not** an LLM policy engine, workflow executor, remote package registry, or
substitute for accountable human judgment. Imports are local; v0.1 requires offline operation.
Source: `README.md:9-12`.

## Status

Experimental, pre-release `0.1.0` workspace under active development. All nine `rulery`
subcommands execute: the binary parses arguments, runs the command through the root facade's
application service, writes exactly one artifact, and exits with the status the specification's
exit-condition matrix names. Source: `README.md:17-22`.

As of `7f29a71` / `dae7f5e` (2026-09-30): the ten-gap closeout programme is complete, the godmode
task graph is 125/125 done, and `cargo xtask verify` passes all eight gates at 184 tests plus 4
doctests. See `progress.md` and `active-context.md`.

## Done criteria

A change is done when `cargo xtask verify` passes. That command is the authoritative gate — there is
**no CI**. The repository has no `.github/` directory.

The eight gates run fail-fast in this exact order, defined at `xtask/src/verify.rs:44-52`:

| #   | Gate            | What it proves                                               |
| --- | --------------- | ------------------------------------------------------------ |
| 1   | bootstrap check | manifests reconcile against the declarative model, no writes |
| 2   | conformance     | the specification contract holds                             |
| 3   | architecture    | workspace membership and dependency boundaries               |
| 4   | fmt             | formatting                                                   |
| 5   | clippy          | lints clean, warnings are errors                             |
| 6   | nextest         | the test suite                                               |
| 7   | doctest         | doc examples compile and pass                                |
| 8   | rustdoc         | documentation builds                                         |

Source: `AGENTS.md` ("Commands") and `xtask/src/verify.rs`.

Beyond the gate, four specification-level criteria are enforced separately and are worth knowing
about, because each was previously satisfied only in prose:

- **The required-gate list is read.** `xtask conformance` fails if `docs/specification.md` loses a
  bullet from its `Required conformance gates` section, so deleting a requirement is now itself a
  gate failure.
- **Renamed-dependency macro hygiene** is compiled under two feature configurations.
- **Renderer output is snapshotted** as reviewed `insta` files.
- **Both `examples/` fixtures** are compared byte for byte by process tests.

## Scope boundaries

These are declined on purpose, not gaps awaiting time:

- `cargo-semver-checks` — no value until a published version exists to diff against.
- Remote package registry or network imports — v0.1 forbids them.
- Fuzzing (`cargo-fuzz`) and Kani model checking — the specification's gate bullet asks only for
  "unit, property, nextest, and doctest suites", which `proptest` plus the existing harness satisfies.

See `active-context.md` for the open questions and `mistakes.md` for process traps.
