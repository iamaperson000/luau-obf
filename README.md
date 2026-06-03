# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 8 — string constants in the constant pool are now stored
encrypted (XOR with per-build position-mixed key) and decrypted lazily inside
the VM. Static inspection of the obfuscated output no longer reveals global
names, method names, or user string literals. Plans 1–7 features unchanged;
full corpus passes under every seed.

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
