# luau-obf

Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 12 — every encrypted constant is now a uniform fixed-size
64-byte blob with the type tag and original length hidden inside the
encrypted payload. The wrapper call site is `_cw("<bytes>")` — a single
argument, identical-looking regardless of whether the constant is a string,
number, boolean, or nil. The previous structural fingerprints (counting
strings vs numbers per proto, inferring string length from ciphertext) are
closed.

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
