# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 25 — junk Sub-chain insertion (final pass in the
Plans-16-25 mangling family). Each MIR basic block has a 20% chance of
receiving a 4-instruction synthetic Sub chain at a seed-random position.
Symmetric to Plan 21's Add/Mul chain. Combined with Plans 15-24, the
obfuscated bytecode contains stochastic dead-store payload (Move, Add,
Mul, Sub), opaque-true branches with junk blocks, Goto trampolines,
operand-padded arithmetic, commuted comparisons, polarity-flipped
branches, and decomposed constants — all per-build-seeded.

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
