# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 24 — synthetic Move insertion. Each MIR basic block has
a 25% chance of receiving a dead Move from the block's first writer to
a fresh VLocal. Adds Move opcodes throughout the bytecode without
changing semantics. Combined with Plan 21's junk arithmetic, every
block has stochastic dead-store payload that a deobfuscator must
liveness-analyze.

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
