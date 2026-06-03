# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 15 — first real MIR-level obfuscation pass. Each `Sub`
instruction is rewritten as `Neg + Add` with 50% probability per instance
(seed-derived). The bytecode no longer 1:1 maps to source-level operators:
a disassembler sees a mix of real `Sub` ops and `Sub`-via-`Neg+Add`
sequences, and can't be sure from the bytecode alone which subtractions in
the original program correspond to which. The luau-passes infrastructure
from Plan 7 is now exercised end-to-end; subsequent plans will add more
rewrites (Add identity-padding, Mul shifts, opaque predicates, control-flow
flattening).

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
