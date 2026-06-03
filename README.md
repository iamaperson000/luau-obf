# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 7 — first per-build obfuscation transforms. Same source under
two different `--seed` values now produces materially different obfuscated
output: opcode bytes are renumbered per build, and each function's constant
pool is shuffled per build. Semantics preserved; the full differential corpus
(Plans 1–6) passes under every seed.

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
