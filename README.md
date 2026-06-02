# luau-obf

Rust-implemented Luau obfuscator. VM-based, loadstring-style protection
intended for Roblox scripts.

**Status:** Plan 3 — closures with upvalues. The obfuscator round-trips an
expanded Luau subset (Plan 2 features + `local function`, anonymous
functions capturing parent locals, mutual sharing across closures over the
same upvalue, deeply nested captures) through a custom VM with no
obfuscation passes yet.

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
