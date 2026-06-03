# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 18 — first CFG-level obfuscation pass. Each conditional
`Branch` terminator in the MIR is flipped with 30% probability: a `Not(cond)`
is appended to the predecessor block and the then/else successors are
swapped. Combined with Plans 15-17's arithmetic mangling, the obfuscated
bytecode now diverges from source structure at both the expression and
control-flow levels — a static analyzer must simulate every `Not` opcode
just to recover the source-level `if`/`else` polarity. Next plans add
opaque predicates and junk blocks, then full control-flow flattening.

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
