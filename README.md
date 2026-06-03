# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 9 — VM template hardening. Internal identifiers (`vm_call`,
`OP_LoadConst`, `read_u16`, register/upvalue locals, etc.) are renamed to
seed-derived opaque names; all comments are stripped from the rendered output;
string-constant encryption now mixes a per-proto salt so identical plaintexts
produce different ciphertexts across protos. A reader of the obfuscated chunk
no longer sees a labeled VM — they see ~400 lines of two-letter locals
operating on opaque tables and bytes. Full corpus passes under every seed.

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
