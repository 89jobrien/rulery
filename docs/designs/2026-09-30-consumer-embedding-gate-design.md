# Consumer embedding contract gate

## Problem

Every gate in this repository proves Rulery against its own specification. `xtask conformance`
checks that `docs/specification.md` declares nine headings, seven schema tags, forty diagnostic
codes, four fixed BLAKE3 vectors, and that every requirement in the traceability table is `satisfied`
and names a test that exists. `tests/conformance/` folds four suites into one binary. The CLI suite
spawns the real binary. `examples/macro-hygiene` compiles an out-of-tree crate under a renamed
dependency.

All of that is in-tree or compile-only, and that is the gap. A crates.io consumer depends on one
specific thing: that the **public facade is sufficient on its own**. An in-tree test can reach any
module it likes, so no in-tree test can fail when `assemble` is moved behind an internal path, when
`CaseFacts` changes shape, or when a different type starts carrying the compiled package. Those are
exactly the changes that would stop the crate being embeddable, and every existing gate would stay
green through all of them.

The one apparent exception proves the opposite. `rulery::tool_library_explain` does exercise the
full assemble → compile → evaluate chain, but its own documentation states the calendar "holds
policy-local date data for the canonical instant only, and it is not a general evaluator path". Its
package is checked in, so it proves the fixture rather than the contract.

This was found the hard way. An out-of-tree spike was written to answer whether minibox could adopt
the engine for container admission. It worked, first try, using only public re-exports — and nothing
in the repository would have told anyone if a later refactor had broken it.

## Decision

Add `examples/embedding-fixture/`, a separate workspace that depends on the facade as `rlry` and
authors, compiles, and evaluates **its own** rule package, and gate on _running_ it.

The fixture is a `main.rs` that reports each contract check by name and exits non-zero on failure,
so a broken contract identifies itself rather than merely failing. `xtask embedding` builds and runs
it under default features and with `macros` enabled, and is the third gate in the fail-fast order.

### Why the fixture is a separate workspace with a renamed dependency

`examples/macro-hygiene` already established the pattern, and the comment in its manifest explains
why: an empty `[workspace]` table keeps it out of the parent, and a renamed dependency is the whole
point, because a hard-coded `::rulery` path would fail there and nowhere else. Reusing the pattern
means the two fixtures fail the same way for the same reason, and it is what makes the check
meaningful: the fixture _cannot_ reach an internal module, so if it compiles the facade is
sufficient.

### Why the gate runs rather than checks

`cargo check` proves types resolve. It cannot observe a decision that evaluates to the wrong answer,
which is the failure mode that matters for a decision engine. The fixture therefore asserts
behavior, and the gate treats a non-zero exit as failure while quoting the fixture's own output so
the failing check is nameable.

The feature-set pair is not copy-paste from the hygiene gate. `macros` swaps how facts are
constructed — the `RuleFacts` derive instead of a hand-written adapter — which is the most likely
place for the two paths to disagree, so the derived path is fed to the same evaluator and asserted
to reach the same outcome. That is a stronger claim than the hygiene gate's, which only needs the
derive to compile.

### Why the gate also checks the fixture's formatting

The fixture is its own workspace, so the root `cargo fmt --all` gate cannot see it. Adding a
declared contract surface that no formatting gate can reach would have introduced a drift hole, so
`ProcessRunner` gained `cargo_fmt_check` and the gate checks it before spending a build.

### What the fixture asserts

`examples/embedding-fixture/policy` is a promotion policy: a release names a target environment, a
source branch, an optional change ticket, an optional rollback plan, and a reviewer count. The
package is deliberately domain-neutral — it is not a second tool library, and it is not any
consumer's domain.

| Assertion                                             | Contract it protects                                                          |
| ----------------------------------------------------- | ----------------------------------------------------------------------------- |
| `consumer_package_compiles_through_the_public_facade` | The assemble → compile chain is reachable from the facade alone               |
| `host_records_convert_to_case_facts`                  | A consumer can map its own data onto the declared vocabulary                  |
| `undeclared_host_fields_are_rejected`                 | The closed vocabulary turns vocabulary drift into an error, not a silent drop |
| `every_outcome_kind_is_reachable`                     | `approve`, `deny`, `escalate`, and `request_information` are all producible   |
| `priority_resolves_overlapping_rules`                 | Priority, not source order, selects the winning rule                          |
| `absent_source_escalates_instead_of_denying`          | An absent optional fact is `unknown`, not `false`                             |
| `absent_rollback_plan_requests_information`           | An information request names the facts a host must supply                     |
| `unmatched_cases_fall_back_to_the_default_outcome`    | A record no rule authorizes is visible rather than approved                   |
| `evaluation_is_reproducible_for_one_instant`          | A trace hash is stable enough to audit                                        |
| `evaluation_is_stable_across_lock_modes`              | What a consumer assembles at startup is what CI tested                        |
| `derived_facts_are_accepted_by_the_evaluator`         | The derive and the hand-built adapter reach the same engine                   |

## Enforcement

Three mechanisms, so the obligation cannot quietly lapse:

1. `REQUIRED_GATE_BULLETS` (`xtask/src/conformance.rs`) gains the bullet, so
   `docs/specification.md` must declare it or `xtask conformance` fails. The surrounding comment is
   explicit that an unmet gate is a known gap and a deleted gate is a hidden one; this gate is met,
   and the bullet is what stops it from being dropped later.
2. Requirement `V13` is added to `REQUIREMENT_IDS` and to the traceability table with
   `status: satisfied`, so `check_requirements` rejects the specification if the row is removed,
   unsatisfied, or names a test that does not exist.
3. `VerifyGate::Embedding` sits in the fail-fast order, and
   `verify_runs_exact_fail_fast_gate_order` pins that order.

## Cost

Two `cargo run` invocations of a small fixture, plus one `cargo fmt --check`. Measured at a few
seconds warm; the fixture has no heavy dependencies beyond the facade it is testing.

## Release wiring

A gate that nothing on the release path runs protects only the working tree. `mise.toml`'s
`release:gate` used to restate its own cargo invocations — `cargo fmt --all`,
`cargo clippy --workspace -- -D warnings`, `cargo nextest run --workspace` — which is what let
conformance, architecture, embedding, doctest, and rustdoc sit out of the release path entirely.

Every one of those three was strictly weaker than the gate `verify` already runs: fmt without
`--check`, clippy without `--all-targets`, nextest without `--all-features`. So `release:gate` now
delegates to `cargo xtask verify`, and `release:prepare`, `release:finalize`, and `release:now`
inherit it through the `release:preflight` dependency they already had. One command list now covers
both paths, so a gate added to `xtask/src/verify.rs` cannot be forgotten by the release tasks. This
is the same drift hazard `Command::lock_mode` is documented for, and the same fix.

One behavior change is worth naming: fmt now checks rather than rewrites. Previously a release
preflight would silently format the working tree on its way to opening a release PR, which meant a
release commit could absorb unrelated uncommitted edits. Failing loudly and letting the operator
commit formatting deliberately is the safer contract for a task whose whole job is to cut a
release.

## Not covered

- **Publication itself.** This gate makes the contract safe to depend on. It does not make the
  crate publishable: the sixteen-crate lockstep release, and the `docs/language` /
  `docs/semantics` placeholders, are separate work.
- **`--extended`.** `release:check` runs `cargo rail release check --all --extended`, while
  `release:preflight` runs the plain `--all`. That asymmetry predates this work and was left alone
  deliberately: preflight is the fast pre-PR gate, `release:check` is the post-bootstrap extended
  pass. Worth revisiting, but it is not this gate's business.
- **Semantic completeness.** The gate proves the engine keeps behaving as it does. It cannot prove
  the policy a consumer authors is the policy that consumer intends.
