# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 4 — multi-return, varargs, generic-for. The obfuscator round-trips
an expanded Luau subset (Plan 3 features + multi-value `return a, b`, multi-bind
`local a, b = f()`, varargs `...` parameters and spread, `for k, v in pairs(t)`
and other generic-for iterators) through a custom VM with no obfuscation passes
yet.

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
