# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 11 — constant pool and proto-header uniformity. The top-level
META table is gone (proto headers fold into each function's encrypted bytecode
prologue). Every constant of every type is uniformly wrapped as `_cw(tag,
bytes)` — strings, numbers, booleans, and nil all encrypt the same way, so
reading the constant pool no longer tells you "this proto has N strings and
M numbers." Full differential corpus still passes under every seed.

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
