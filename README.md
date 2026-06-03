# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 23 — Goto trampoline insertion. Each `Goto(target)`
terminator has a 30% chance of being rerouted through an empty
intermediate block whose terminator points to the original target. The
CFG gains one block and one indirect hop per insertion, while runtime
behavior is unchanged. Combined with Plan 20's opaque-true wrapping,
trampolines may themselves be wrapped — turning a single `Goto` into a
two-hop opaque-branch chain.

## Build

```
cargo build --workspace --release
```

## Use

```
cargo run -p luau-obf-cli --release -- input.luau -o output.luau
```

## Test

Differential execution tests require the `luau` CLI on PATH.

```
cargo test --workspace --release
```

## AI Disclosure

AI was used for writing the docs and some funky VM stuff I don't yet understand.


## Architecture

See `docs/superpowers/specs/2026-06-01-luau-obfuscator-design.md`.
