# luau-obf
<img width="1196" height="634" alt="Screenshot 2026-06-08 at 12 59 28 PM" src="https://github.com/user-attachments/assets/c7c70813-46f1-4eab-948c-97ace1c8aab1" />
Rust-implemented Luau obfuscator. VM-based.

**Status:** Plan 31 — Dispatcher split via closure groups. The single monolithic
`if/elseif` dispatch ladder (36 opcodes, one function) is now split into three
independent closure-shaped handler groups: Group A (arithmetic + comparison,
13 ops), Group B (load/move/global/table/upval/vararg/superop, 14 ops), and
Group C (control flow/calls/closures, 9 ops). The main `vm_call` loop tries
groups in fixed order A→B→C, propagating a `"miss"` sentinel when an opcode
does not belong to the current group. Group C uses additional status strings
(`"return"`, `"return_multi"`) to signal early exit from the dispatch loop
without relying on Lua's multi-return from inner functions. The group functions
are forward-declared at module scope and close over `vm_call` via the standard
Luau forward-declaration pattern, so Group C's `Closure` handler can
recursively invoke the VM. An adversary must now locate and read three separate
closures rather than one function to map the full opcode set.

Previously: Plan 30 — Handler-fusion superoperators. The VM dispatcher now
includes a `LoadConstLoadConst` (LCLC) superopcode that performs two
consecutive LoadConst operations in a single 9-byte instruction (vs 10 bytes
for two separate LoadConst instructions). Fusion is stochastic: during
encode, each eligible pair of adjacent `LoadConst` instructions is fused with
~50% probability, driven by the build seed via ChaCha20. Jump-target safety is
enforced — the second instruction of a candidate pair is never fused if it is
a jump target, ensuring no control-flow edge lands in the middle of a LCLC.
Because the handler shape (read 4 u16s, do 2 const-table lookups) does not
match any MIR-generated opcode, an adversary must identify LCLC independently
rather than pattern-matching against the known 35-opcode set. The opcode count
grows from 35 to 36; the per-proto inv table header and the VM's inv-loop
bound both update automatically via `ALL_OPS.len()` and the `opcodes | length`
Jinja filter respectively.

Previously: Plan 29 — Env-bound stage-0 key. The stage-0 decryption key
can now be bound to a runtime environment value (e.g.
`tostring(game.PlaceId)`) so the obfuscated blob cannot be decrypted by
simply running the wrapper offline in a plain `luau` interpreter. At
build time the obfuscator XOR-folds the `expected_value` into the
cipher key; the emitted stage-0 wrapper evaluates `runtime_expr` at
load time and reproduces the fold via a small `_mix` helper. A host
that produces the wrong value gets garbled plaintext and `loadstring`
fails silently. The binding is optional and off by default so the
corpus difftest and all existing acceptance tests remain unaffected. The
CLI gains `--env-bind-expr` / `--env-bind-expected` flags (mutual-pair
required). Three new acceptance tests cover match, mismatch, and the
default-off path.

Previously: Plan 28 — RC4 stream cipher (drop-256) for stage-0. The
stage-0 self-decrypting bootstrap previously used positional XOR
(`b ^ key[i%32] ^ (i%256)`), which an adversary broke in ~30 lines of
Python by bucketing ciphertext bytes by `i mod 32` and exploiting the
known backslash-byte density of the stage-1 payload. The stage-0
`encrypt_payload` function now runs RC4 with a 256-byte keystream drop
(KSA then skip-256 then PRGA), and the Luau `_d(s)` wrapper in the
emitted output mirrors this algorithm exactly. Positional structure in
the ciphertext is destroyed: each byte's keystream contribution depends
on the full S-box state, making the earlier bucketing attack infeasible.
A new Rust unit test (`rc4_rust_luau_agree`) encrypts with Rust RC4 and
decrypts with the verbatim Luau implementation to catch any
KSA/PRGA/drop-count mismatch at the boundary.

Plan 27 — per-proto constant-pool XOR keys. Each proto in the
emitted VM now gets its own `(key_a_P, key_b_P)` 32-byte key pair derived
from the build-time RNG, replacing the single shared `(key_a, key_b)` pair
that all protos previously used. The VM template's `_KAS`/`_KBS`
arrays-of-arrays replace the old scalar `_KA`/`_KB`, and `_decrypt` indexes
by `proto_id`. The Round 4 adversary's cross-proto bucketing attack (bucket
ciphertext bytes by `i mod 32` across all protos, exploit backslash density
to recover the shared key) is defeated: each proto's buckets now reflect a
distinct key, collapsing the attack's signal-to-noise for short protos.

Plan 26: live-operand junk arithmetic. Both the junk-arith
(Add/Mul) and junk-sub (Sub/Sub) dead-store passes now have a ~50% chance
per injection of using a provably-numeric VLocal from earlier in the same
block as the first BinOp's left operand, instead of always generating two
fresh LoadConst values. A conservative forward scan (`number_vlocals_before`)
identifies candidate VLocals; the selection is seeded and deterministic.
This defeats an adversary's constant-folding pivot: when the lhs is a live
register, static folding fails without a full dataflow model. The RNG
schedule is mode-invariant (six draws per block visit regardless of mode),
so seed-to-output behaviour remains deterministic.

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
