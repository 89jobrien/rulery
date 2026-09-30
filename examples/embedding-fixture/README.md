# Embedding fixture

A separate workspace that depends on the facade as `rlry` and authors, compiles, and evaluates its
own rule package through the public re-exports only.

`examples/macro-hygiene` proves the macros expand hygienically under a rename, but only compiles.
This fixture is _run_ by `cargo xtask embedding`, so it covers the general evaluation path that the
normative tool-library fixture deliberately cannot: that fixture's calendar answers one canonical
instant and its package is checked in, proving the fixture rather than the contract.
