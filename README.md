# luau-obf

Rust-implemented Luau obfuscator. VM-based, loadstring-style protection
intended for Roblox scripts.

**Status:** Plan 2 — tables, methods, and `break`. The obfuscator round-trips
an expanded Luau subset (tables, indexed access, method calls, dotted/method
function declarations, anonymous function expressions, `break`) through a
custom VM with no obfuscation passes yet.

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

## Architecture

See `docs/superpowers/specs/2026-06-01-luau-obfuscator-design.md`.
