# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 26 — live-operand junk arithmetic. Both the junk-arith
(Add/Mul) and junk-sub (Sub/Sub) dead-store passes now have a ~50% chance
per injection of using a provably-numeric VLocal from earlier in the same
block as the first BinOp's left operand, instead of always generating two
fresh LoadConst values. A conservative forward scan (`number_vlocals_before`)
identifies candidate VLocals; the selection is seeded and deterministic.
This defeats an adversary's constant-folding pivot: when the lhs is a live
register, static folding fails without a full dataflow model. The RNG
schedule is mode-invariant (six draws per block visit regardless of mode),
so seed-to-output behaviour remains deterministic.

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
