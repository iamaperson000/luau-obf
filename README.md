# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 19 — constant decomposition. Each finite numeric `LoadConst`
in small-magnitude range is rewritten with 30% probability as
`LoadConst(n + δ); LoadConst(δ); Sub`, with δ a seed-derived integer in
[1, 65535]. The original literal is gone from the encrypted constant pool;
two seed-derived blobs replace it. Composes with Plans 15-17: the inserted
Sub becomes a candidate for `Sub → Neg + Add` rewriting, then for Add
operand-padding. One source-level constant can compile to a multi-instruction
synthetic chain per build.

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
