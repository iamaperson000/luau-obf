# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 16 — Add identity-padding shipped. Each `Add` instruction
is rewritten with 30% probability as `(a + k) + (b − k)` with a seed-derived
integer `k`. Combined with Plan 15's `Sub → Neg + Add` rewrite, neither
additive operator maps cleanly to a single bytecode instruction; a static
analyser must reconstruct which arithmetic ops are real and which are
identity padding. Subsequent plans extend the expression-mangling family
(Mul rewrites, boolean mangling, constant decomposition) before moving to
control-flow obfuscation (opaque predicates, junk blocks, flattening).

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
