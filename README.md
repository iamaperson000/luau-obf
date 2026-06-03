# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 14 — the entire VM (dispatcher, helpers, constant pool, bytecode)
is now wrapped in a stage-0 self-decrypting bootstrap. The raw output file
contains only a small XOR decryption helper, a 32-byte key, an encrypted
payload string, and a `loadstring(decrypt(payload))(...)` invocation. A
reader cannot statically see the dispatcher, opcode handlers, encryption
constants, or bytecode without first executing the stage-0 decryption.
Plans 1–13 features unchanged; full corpus passes under every seed.

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
