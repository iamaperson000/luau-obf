# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 22 — comparison-operator commute. Each comparison BinOp
(<, <=, >, >=, ==, ~=) has a 40% chance of being commuted: operands
swapped and the operator flipped to the symmetric form. Bit-exact for
all Luau values including NaN and metatable __lt/__le/__eq. Subsequent
plans wrap return paths (Plan 23) and enrich junk blocks (Plan 24).

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
