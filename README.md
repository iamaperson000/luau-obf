# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 17 — Mul operand-scatter shipped. Combined with Plans 15
(Sub→Neg+Add) and 16 (Add operand-padding), every additive and
multiplicative MIR operator is rewritten probabilistically into a
seed-derived synthetic expansion. The numeric-VLocal dataflow analyzer
guards every rewrite, so programs that overload arithmetic metamethods
remain correct. Next plans extend the family (boolean mangling, constant
decomposition) before moving to control-flow obfuscation.

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
