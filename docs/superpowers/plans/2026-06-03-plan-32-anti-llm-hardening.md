# Luau Obfuscator — Plan 32: Anti-LLM hardening sweep

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

## Context

After Plans 26-31, an unaided Claude Opus agent broke the obfuscated `bank.luau`
in **5 minutes 9 seconds** (full source recovery, byte-identical output). The
agent's report identified three structural weaknesses:

1. The RC4 key sits as an inline 32-byte literal three lines above its use.
2. `_byte` / `_decrypt` are stateless — each byte decodes independently.
3. Stage-1 emits semantic identifier names (`handle_group_a/b/c`) that
   survive mangling and signpost the dispatcher.

Plus implicit context: the artifact is a self-contained decryption
procedure that can be statically simulated. As long as that's true, LLM-
assisted reverse engineering wins in minutes.

**This plan attacks all three named weaknesses AND moves the artifact
from "statically decryptable" toward "must execute to decrypt".**

## Strategy

We accept that a multi-day attacker who hosts the VM will still win.
We design against the **5-minute-LLM-break** threat by making the
artifact's decryption procedure:

- Non-linear (each layer depends on state from previous layers)
- Self-referential (key derivation reads the ciphertext that it'll decrypt)
- Anti-trace (corrupts itself when traced or instrumented)
- Unrecognizable (no semantic identifiers, no isolated hash functions
  the adversary can pattern-match)

The target is to **multiply the LLM time-to-break by ≥10x** (so 50+ min
of focused work instead of 5). Anything that doesn't move that number
isn't worth doing in this plan.

---

## Components

### Component A — Self-hashing stage-0 key derivation

**Problem.** Currently stage-0 wrapper has `local _k = "<base 32 bytes>"`,
trivially extracted by regex. Even with Plan 29 env-binding enabled, the
mix is XOR with a single `_bind` expression — adversary can extract base,
guess bind value, recover folded key.

**Fix.** Replace literal `_k` with a derivation:

```luau
local _k = _h(_kbase, _stage1_fingerprint, _bind)
```

Where:
- `_kbase` is still a 32-byte literal (irreducible).
- `_stage1_fingerprint` is computed AT RUNTIME by hashing chunks of `_s`
  (the stage-1 ciphertext) using a non-cryptographic but non-RC4-shaped
  mixing function (FNV-1a-like over selected byte windows of `_s`).
- `_bind` is the existing optional env-binding string (default: a
  stable host fingerprint, see Component I).
- `_h` is a Luau-implemented 32-byte mixer combining all three.

Build-time mirror: Rust computes the same `_stage1_fingerprint` over the
already-built `_s` ciphertext, then computes `folded_key` for use with
RC4 encryption.

**Effect.** Adversary can no longer copy `_k` directly — they must
identify `_h` (which is NOT RC4-shaped) AND re-run it over the right
bytes of `_s`. Tampering with stage-1 ciphertext (a debugger setting
breakpoints in the wrapper) silently breaks the key.

### Component B — Stateful byte cipher

**Problem.** `_byte(code, pc, pid)` XORs with `(pc * k1 + k0 + pid * X) mod 256`.
Every byte decodes independently — adversary writes a 5-line Python decoder.

**Fix.** Introduce a running u32 state per function decode. Decoding
byte N requires state from bytes 0..N-1. Specifically:

```luau
-- per-vm_call: 
local _bs = _bsinit(proto_id, code)  -- per-function seed
-- per byte:
local function _byte(code, pc, pid)
    local raw = _sbyte(code, pc)
    local mixed = _xor(_xor(raw, _bs % 256), (pc * _k1 + _k0 + pid * 2654435761) % 256)
    _bs = ((_bs * 1103515245) + mixed + 12345) % 4294967296  -- LCG advance
    return mixed
end
```

The `_bsinit` seeds `_bs` from the function's `(pid, num_params, num_regs)`
header bytes (which themselves go through `_byte` — chicken-and-egg
solved by initializing `_bs = pid + 1` for the prologue read, then
recomputing seed after prologue).

**Effect.** Static byte-by-byte decoding fails. Adversary must
simulate the LCG state machine in lockstep with `pc` advancement.
They'll figure it out eventually but it forces them to write a stateful
disassembler, not a stateless one.

**Bytecode-emission counterpart in Rust** — `encode.rs::keystream_byte`
becomes stateful too. The byte emitted at offset `pc` is XOR'd with the
keystream byte derived from the running state at that point. Encoder
walks pc forward, maintains the same LCG, computes the same byte output
in lockstep. **Critical:** the encoder and decoder must produce identical
state sequences — extensively unit-tested.

### Component C — Tangled bytecode + constant cipher

**Problem.** Constant pool decryption `_decrypt(enc, proto_id)` uses
only `_KAS[pid+1]`, `_KBS[pid+1]`, and a `proto_id*7919` salt. All
recoverable statically.

**Fix.** Mix the byte cipher's running state `_bs` (after prologue
decode) into the constant cipher's key derivation:

```luau
local function _decrypt(enc, proto_id, bs_after_prologue)
    local ka = _KAS[proto_id + 1]
    local kb = _KBS[proto_id + 1]
    local out = {}
    local mix = bs_after_prologue  -- the running byte-cipher state
    for i = 1, #enc do
        local b = _sbyte(enc, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1 + (mix % 256)) % 256
        out[i] = _schar(_xor(_xor(_xor(b, ka[idx]), pos), kb[idx]))
        mix = (mix * 1103515245 + b + 12345) % 4294967296
    end
    return _tconcat(out)
end
```

Now `_const(c, idx, proto_id, bs)` takes the running state from the
caller (the dispatcher) and threads it into `_decrypt`. The
constants for function P can only be decoded after the byte-cipher
state has advanced through P's prologue.

**Build-time:** Rust mirrors the same state evolution, encrypting
constants with the matching keystream.

### Component D — Per-function CRC integrity check

**Problem.** Adversary can tamper with bytecode bytes and the artifact
keeps running.

**Fix.** Reserve `consts[1]` (1-indexed) of each function as a
CRC32 of the function's encrypted bytecode (the actual bytes that
appear in `_oj`/`CODE[i]`). At function entry, after prologue decode,
the VM:

1. Decrypts `consts[1]` (this requires the byte cipher state to be
   correct — already coupled via Component C).
2. Computes CRC32 of `code` (the encrypted bytes — no need to decrypt
   them all, just the ciphertext).
3. Compares. **On mismatch: corrupt `_bs` silently, do not error.**
   Continue execution. The bytecode will then decrypt to garbage and
   probably crash with a confusing error far from the actual integrity
   site.

**Build-time:** `encode_function` computes the CRC after producing
ciphertext, stores it as the first constant.

### Component E — Anti-trace tripwires

**Problem.** Adversary can `debug.sethook` the wrapper or instrument
`pcall` to dump state. No detection.

**Fix.** At stage-1 load time, cache trusted references:

```luau
local _trusted_pcall = pcall
local _trusted_sbyte = string.byte
local _trusted_gethook = debug and debug.gethook
```

At `vm_call` entry, check these match current references. On mismatch,
corrupt `_bs` silently (state goes wrong, dispatch eventually breaks).

Spread the checks: two checks in `vm_call`'s prologue, one in each
handler group's first invocation, one inside `_const`. So an
adversary who patches one check still trips others.

Also: at `vm_call` entry, check `debug.gethook()` returns nil.
If a hook is installed (e.g., for tracing), corrupt state.

### Component F — Aggressive identifier mangling

**Problem.** `handle_group_a/b/c`, `read_u16`, `read_i16`, `vm_call`,
`_const`, `_decrypt`, `_byte` all appear with their semantic names in
the obfuscated output (the mangle list didn't cover them).

**Fix.** Audit `crates/luau-emit/src/mangle.rs::MANGLE_TARGETS` against
the actual template. Add every identifier defined in `vm.luau.j2` that
isn't a Luau global. Also extend STAGE0_MANGLE_TARGETS with everything
in `render_stage0`.

Increase mangling alphabet from 2-letter (`_xy`, 676 suffixes) to
3-letter (`_xyz`, 17576 suffixes). This reduces the probability of
collisions when the target list grows, and increases the surface area
adversaries must process to recognize each rename.

Add a unit test that obfuscates a tiny program and asserts that NONE
of: `handle_group_a`, `handle_group_b`, `handle_group_c`, `read_u16`,
`read_i16`, `vm_call`, `_const`, `_decrypt`, `_byte`, `_byte_state`,
`_trusted_pcall`, `_trusted_sbyte` appear in the final output.

### Component G — Junk arithmetic result consumption

**Problem.** The junk passes' `tmp2` is always dead. Dead-store
elimination strips the entire chain in one pass.

**Fix.** In ~25% of injections, modify the chain so `tmp2` is consumed
by a live VLocal in a net-zero way:

```
... existing chain producing tmp2 ...
live_x_new = BinOp::Sub(live_x, tmp2)  -- = live_x - tmp2
live_x_final = BinOp::Add(live_x_new, tmp2)  -- = live_x - tmp2 + tmp2 = live_x
```

Now `tmp2` is read; the original `live_x` slot is overwritten (twice)
with the same final value. Dead-store elimination must prove that the
two operations cancel — a SMT-class problem.

(For seeded determinism: always draw the consume-mode coin; only apply
when a live Number-typed VLocal is available at the post-chain position.)

### Component I — Env-binding default-on (soft mode)

**Problem.** Plan 29 added env-binding but it defaults OFF. With the
default OFF, the artifact decrypts off-host with no resistance.

**Fix.** Change `Options::env_binding` default from `None` to
`Some(EnvBinding { ... })` with a "soft" expression that evaluates
identically on every Luau host:

```rust
runtime_expr: r#"(type(rawget) == "function" and "ok" or "fail")"#,
expected_value: "ok".to_string(),
```

The expression evaluates to `"ok"` on any Luau (and standard Lua),
so functionally the binding doesn't lock the artifact to a specific
host. But the BUILD nonetheless folds `"ok"` into the key, and any
runtime where `rawget` is shadowed or missing gets `"fail"` → garbage
decryption. This raises the bar for analysts who modify the runtime
to instrument behavior.

CLI: add `--no-env-bind` flag to fully disable. Tests use this.

---

## File-level diff plan

| File | Changes |
|---|---|
| `crates/luau-emit/src/stage0.rs` | New `_h` mixer + Rust mirror; `STAGE0_*_LUAU` consts grow |
| `crates/luau-emit/src/encode.rs` | Stateful keystream w/ LCG; CRC32 of encrypted bytecode |
| `crates/luau-emit/src/render.rs` | Thread state through; first constant slot reserved for CRC |
| `crates/luau-runtime/assets/vm.luau.j2` | Massive: stateful `_byte`, tangled `_decrypt`, integrity check, tripwires |
| `crates/luau-emit/src/mangle.rs` | Extend MANGLE_TARGETS + STAGE0_MANGLE_TARGETS; suffix → 3 letters |
| `crates/luau-obf/src/lib.rs` | `Options::env_binding` default-on (soft); acceptance tests |
| `crates/luau-obf-cli/src/main.rs` | `--no-env-bind` flag |
| `crates/luau-passes/src/{junk_arith,junk_sub}.rs` | 25% consume-mode |
| `README.md` | Status |

---

## Tasks

Each task is its own implementer subagent invocation. Dependencies are
serial.

### Task 1 — Component B: Stateful byte cipher (foundation)

This is the largest change and gates Components C and D.

- Add LCG-stateful keystream byte computation to both Rust (`encode.rs`)
  and Luau (`vm.luau.j2`). Encoder maintains running `bs: u32` while
  emitting per-pc keystream bytes. Decoder maintains identical `bs`
  threaded through `vm_call`'s scope.
- Bytecode header (prologue) bytes use a SIMPLER deterministic
  initialization (just `bs = pid + 1`) so the bootstrap chicken-and-egg
  resolves.
- After the prologue is decoded, `bs` has advanced N + 5 times (where N
  = ALL_OPS.len()). That post-prologue state is THE INPUT to Component C.
- Add a Rust↔Luau cross-check test: build a known bytecode buffer in
  Rust, write a tempfile Luau program with the new decoder, verify
  identical decode.
- Run full corpus.

**Commits:**
- `feat(emit/runtime): stateful LCG keystream replacing positional XOR`
- `test: Plan 32-B cross-impl + corpus`

### Task 2 — Component C: Tangled constant cipher

Builds on Task 1.

- `_decrypt` now takes a `bs_seed` (the byte-cipher state after the
  current function's prologue). Mix `bs_seed` into the constant cipher's
  per-byte position term.
- Rust `encrypt_bytes` mirrors the same mixing.
- The `bs_after_prologue` value is computed in Rust during
  `encode_function` and stored alongside the function's per-proto keys
  (it's a deterministic function of pid + k0 + k1 + n_ops, so it can be
  recomputed but caching is cleaner).
- Add a test that asserts: encrypting a known constant under a known
  seed produces a deterministic output, AND decoding via Luau yields
  the same plaintext.

**Commits:**
- `feat(emit/runtime): tangle constant cipher with byte-cipher state`
- `test: Plan 32-C constant-pool decryption requires post-prologue state`

### Task 3 — Component D: CRC32 integrity check

- Add CRC32 (textbook reflected, polynomial 0xEDB88320) to both Rust
  (`encode.rs`) and Luau (`vm.luau.j2`).
- After encoding the function's bytecode, compute CRC32 of the
  encrypted bytes. Store as the FIRST constant (slot 0) of the proto's
  constant pool. Shift all existing constants by +1.
- Update LIR→bytecode `Const(c)` operand emission to add +1 to all
  ConstIdx references. (CRITICAL: this is a global renumbering. The LIR
  layer is unchanged; the +1 happens in `encode.rs::push_operand`-style
  emission for `Operand::Const`.)
- In `_const(consts, idx, proto_id, bs)` no change needed if the
  caller already passes the new idx values.
- At `vm_call` entry: decode constant 1 (the CRC32), compute CRC32 of
  `code`, compare. If mismatch, `_bs = _xor(_bs, 0xDEADBEEF)` (or
  similar irreversible mutation). DO NOT error.

**Commits:**
- `feat(emit/runtime): CRC32 integrity check on encrypted bytecode`
- `test: Plan 32-D tamper detection corrupts state`

### Task 4 — Component A: Self-hashing stage-0 key

- Implement a Luau `_h(base, s_fingerprint, bind)` mixer in
  `STAGE0_*_LUAU` const that:
  - Reads `_kbase` (32 bytes).
  - Hashes 32 selected byte-windows of `_s` (the stage-1 ciphertext)
    into a 32-byte fingerprint via FNV-1a style.
  - Optionally folds `_bind` (env-binding string).
  - Combines via XOR-of-permutations into the 32-byte working key.
- Rust mirror in `stage0.rs` produces the same key for use with RC4
  encryption.
- Wrapper template changes: `local _k = _h(_kbase, _hash(_s), _bind)`.
- Inline-key path (no binding) still works because `_bind = ""` →
  fingerprint+kbase contributes deterministically.

**Commits:**
- `feat(emit/stage0): self-hashing key derivation from stage-1 ciphertext`
- `test: Plan 32-A wrapper executes; tampering with _s breaks key`

### Task 5 — Component E: Anti-trace tripwires

- Stage-1 caches references to `pcall`, `string.byte`, `debug.gethook`
  at module load (top of `vm.luau.j2`, before `vm_call`).
- `vm_call` prologue: check `pcall == _trusted_pcall and string.byte ==
  _trusted_sbyte`. Mismatch → `_bs = _xor(_bs, 0xCAFEBABE)`.
- ALSO check `_trusted_gethook() == nil` if `_trusted_gethook` is
  non-nil (debug library is optional in stripped Luau builds).
- Spread one more check into `_const` and into one handler group.
- These checks must NOT use `if-then-error`. They mutate `_bs` and let
  state corruption do the rest.

**Commits:**
- `feat(runtime): anti-trace tripwires corrupt state on instrumentation`
- `test: Plan 32-E hook installation corrupts decode`

### Task 6 — Component F + I: Mangling sweep + env-bind default-on

Combined because both touch user-facing identifiers and `Options`.

- F: Walk through `vm.luau.j2` after Tasks 1-5 land. List EVERY
  `local <name>` declaration. Add each to `MANGLE_TARGETS` if not
  already present. Specifically: `handle_group_a, handle_group_b,
  handle_group_c, read_u16, read_i16, vm_call, _const, _decrypt,
  _byte, _bs, _trusted_pcall, _trusted_sbyte, _trusted_gethook,
  _bsinit`, and any new identifiers introduced in this plan.
- Extend STAGE0_MANGLE_TARGETS similarly for `_h, _hash, _s_fp, _kbase,
  _bind, _mix`, etc.
- Bump suffix alphabet from 2 → 3 letters. Update `build_name_map`
  and `build_stage0_name_map` to generate `_xyz`-style suffixes.
  Update determinism tests.
- I: Change `Options::env_binding` default. Add `--no-env-bind` CLI
  flag.
- Add a `no_semantic_names_leak` integration test that obfuscates a
  trivial program and asserts none of the protected identifiers appear
  in the output as bare tokens.

**Commits:**
- `feat(emit/mangle): extend MANGLE_TARGETS + 3-letter suffixes`
- `feat(obf): env-binding default-on (soft host fingerprint)`
- `test: Plan 32-F+I no semantic names leak; --no-env-bind works`

### Task 7 — Component G: Junk arith result consumption

- Modify `junk_arith.rs` and `junk_sub.rs` so ~25% of injections
  follow the chain with a net-zero `tmp2` consumption against a live
  VLocal (when available).
- Always draw the consume-mode coin to maintain mode-invariance.
- Add a unit test confirming `tmp2` is referenced as an operand in at
  least some seeds.

**Commits:**
- `feat(passes): junk-arith result occasionally consumed in net-zero usage`

### Task 8 — README + status

One paragraph documenting Plan 32 status.

**Commits:**
- `docs: mark Plan 32 status`

---

## Verification

After each task:
- `cargo test --workspace --release` — the corpus difftest is THE gate.
  Any byte-cipher state mismatch, CRC mismatch, key derivation error, or
  constant cipher entanglement bug WILL surface here.
- `cargo build --workspace --release 2>&1 | grep -iE "warning|error"` — clean.

After all tasks:
- Dispatch a fresh Opus adversary against the new pipeline. Time-to-break
  should be ≥10x of the 5-min baseline.

## Threat-model targets

- **Tier 1 (script kiddie):** push from ~25 → ~55 (custom-VM
  recognition still possible, but the stateful crypto + integrity check
  forces them off the static-decode path).
- **Tier 2 (1-hour LLM-assisted RE):** push from ~8 (5 min) → ~25
  (45-60 min) — they still get there but it's a real session.
- **Tier 3 (multi-day team):** unchanged at ~2 — the host-the-VM
  attack still wins. This plan doesn't claim to fix that.

---

## Out of scope (deferred)

- **Component H (per-build dispatcher topology randomization).** Worth
  doing but deferred — too much surface area for one plan.
- **Server-side eval / off-host execution.** Architectural; rewrite.
- **Native compilation.** Architectural; rewrite.
- **Real cryptographic primitives (e.g., ChaCha20 in Luau).** Possible
  but the runtime cost is severe and the gain marginal once stateful
  mixing is in place.
