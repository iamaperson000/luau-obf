# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 21 — junk arithmetic dead-store injection. Each MIR
basic block has a 30% chance of receiving a 4-instruction synthetic
arithmetic chain (LoadConst, LoadConst, Add, Mul) at a seed-determined
position. The four fresh VLocals are never read — a static analyzer must
do liveness analysis to identify them as dead. Combined with Plans 16-17,
the inserted Add and Mul are themselves scattered into operand-padded
forms, so a single junk chain compiles to 10+ bytecode instructions.

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
