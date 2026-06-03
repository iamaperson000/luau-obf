# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 20 — opaque-true predicates. Each unconditional `Goto`
in the MIR is candidate for being wrapped in a `Branch { cond: δ*δ > 0,
then: target, else: junk_block }`, where δ is a seed-derived integer in
[1, 65535]. The else branch points to a per-function junk block holding
~3 dead `LoadConst` instructions and a `Return(None)`. At runtime the
opaque is always true; statically the analyzer must constant-propagate
the Mul + Gt + δ value to prove it. The junk block compiles to bytecode
but is never executed. Combined with Plans 17-18 (Mul scatter + branch
polarity), the opaque Mul and Branch are themselves further mangled.

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
