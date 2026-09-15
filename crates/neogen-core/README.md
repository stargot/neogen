# neogen-core

Deterministic, dependency-free simulation core of Neogen
(world model, fixed-timestep tick, seed-based generation, state hashing,
snapshots). The crate must not depend on Godot, GDExtension bindings or any
third-party crate — see the repository `docs/ARCHITECTURE.md` gates.

## Golden determinism fixtures

`tests/golden/world_hashes.txt` records state hashes
(`state_hash`, FNV-1a over an explicit `WorldState` field walk) for fixed
seeds at checkpoint ticks; `tests/golden_determinism.rs` compares every run
against them, so any accidental change to generation, stepping or the state
shape fails loudly.

Regenerate **only** after a deliberate state-format or simulation-semantics
change (and explain the change in the commit):

```sh
NEOGEN_UPDATE_GOLDEN=1 cargo test -p neogen-core --test golden_determinism
```

If fixtures fail without such a change, you introduced non-determinism or a
regression — fix the code, do not refresh the fixtures.

## Gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
