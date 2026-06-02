# luau-obf

Rust-implemented Luau obfuscator. VM-based, loadstring-style protection
intended for Roblox scripts.

**Status:** Plan 1 — skeleton & identity pipeline. The obfuscator round-trips
a meaningful Luau subset through a custom VM with no obfuscation passes yet.

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
