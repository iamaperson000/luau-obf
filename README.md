# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 13 — per-proto opcode byte permutation. Each function now
has its own permutation of the 35 opcode bytes, stored encrypted in its
bytecode prologue. The same logical operation (e.g. ADD) is emitted as
different bytecode bytes in different protos within the same chunk, so the
deobfuscator must extract and apply each proto's permutation before
disassembly. Dispatcher arm order is unchanged (deferred to a later plan).
Full differential corpus passes under every seed.

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
