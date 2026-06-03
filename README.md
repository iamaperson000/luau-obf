# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 10 — opcode bytes are inlined directly into the dispatcher
(the 35-line opcode-constants block is gone) and bytecode bytes themselves are
XOR-encoded per build against a position-mixed keystream. Static analysis of
the obfuscated output no longer sees a labeled VM, no longer sees an opcode
table, and `string.byte` over a `CODE[i]` entry returns pseudo-random bytes
rather than the underlying opcode stream. Full corpus passes under every seed.

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
