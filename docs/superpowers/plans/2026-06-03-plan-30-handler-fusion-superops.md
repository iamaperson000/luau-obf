# Luau Obfuscator — Plan 30: Handler-fusion superoperators

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Round 1/5 finding: the opcode permutation in plan 13 is "just
a Caesar cipher" because once the adversary identifies one handler by
shape (e.g., `regs[a] = regs[b] + regs[c]`), the *handler shapes are
fixed*. Adding fused superoperators changes that: a handler that does
"two LoadConsts in a row" is a *new shape*. The adversary has to ID
each one separately.

**Scope (minimum viable).** Add **one** superop —
`LoadConstLoadConst` (LCLC) — that performs two consecutive LoadConst
operations in a single dispatch. Demonstrate the pattern. If time
permits, also add `MoveMove`. Don't try to fuse arbitrary opcodes; that's
a separate research project.

LCLC operands: `(a1, b1, a2, b2)` — 4 × u16 = 8 bytes after the opcode.

LCLC semantics:

```
regs[a1] = const_table[b1+1]
regs[a2] = const_table[b2+1]
```

I.e., exactly the same as two LoadConsts.

**Where fusion happens:** at *encode time* in
`crates/luau-emit/src/encode.rs`, scan adjacent LIR instructions; when
two `LoadConst` opcodes appear back-to-back in the same encode loop,
emit one LCLC instead. No MIR or LIR-pass changes required.

**Threshold:** Fuse only with a per-build probability (e.g., 50% — a
seed-driven coin per opportunity). At 100% we'd fuse every consecutive
LC pair and the adversary still has a static pattern; randomizing makes
it stochastic. RNG byte consumption: one byte per opportunity.

---

## Implementation

### `crates/luau-lir/src/types.rs`

Extend `OpKind` enum:

```rust
LoadConstLoadConst,
// (optional second pass) MoveMove,
```

### `crates/luau-emit/src/opmap.rs`

Add `LoadConstLoadConst` to `ALL_OPS`. The opcode-shuffle code is
length-agnostic; opcode count grows from 35 → 36.

### `crates/luau-runtime/assets/vm.luau.j2`

Two updates:

1. The `inv[]` table size depends on opcode count. Currently
   `for i = 1, 35 do inv[i] = _byte(code, 5 + i, proto_id) end` (line 104).
   Change `35` to `{{ opcodes | length }}` or a similar template-driven
   constant. **Action item:** check that the bytecode header from
   `encode_function` matches in length — the header writes 35 bytes for
   `inv`. That needs to grow to 36 too. Look in `encode.rs` for the
   `write_inv` / opcode permutation header emission.

2. Add a new branch in the dispatcher:

   ```luau
   elseif op == {{ ops.LoadConstLoadConst }} then
       local a1 = read_u16(code, pc, proto_id); pc = pc + 2
       local b1 = read_u16(code, pc, proto_id); pc = pc + 2
       local a2 = read_u16(code, pc, proto_id); pc = pc + 2
       local b2 = read_u16(code, pc, proto_id); pc = pc + 2
       regs[a1] = _const(consts, b1 + 1, proto_id)
       regs[a2] = _const(consts, b2 + 1, proto_id)
   ```

3. Update `opname` in `render.rs` to map the new variant. Update any
   other place that pattern-matches `OpKind` exhaustively
   (`#[deny(unreachable_patterns)]` or `match` exhaustiveness will tell
   you on `cargo check`).

### `crates/luau-emit/src/encode.rs`

In `encode_function`, when iterating LIR instructions, peek at the next
instruction:

```rust
let mut i = 0;
while i < f.instrs.len() {
    let cur = &f.instrs[i];
    let nxt = f.instrs.get(i + 1);
    let fuse_lc_lc = matches!(cur.op, OpKind::LoadConst)
        && nxt.is_some_and(|n| matches!(n.op, OpKind::LoadConst))
        && rng.gen::<u8>() < 128;
    if fuse_lc_lc {
        // emit LCLC with operands (cur.a, cur.b, nxt.a, nxt.b)
        emit_op(OpKind::LoadConstLoadConst, ...);
        i += 2;
    } else {
        emit_op(cur.op, ...);
        i += 1;
    }
}
```

Caveat: branching/jump targets. If the second LC is a jump target from
elsewhere, fusing would break it (jump would land mid-superop). Need to
either:

- (a) Skip fusion when the second instruction is a jump target.
- (b) Confirm that the encoder's jump-target relocation framework is
      instruction-index-based, not byte-offset based, and adjust target
      mapping accordingly.

**Check first** — read `encode.rs` to understand the existing jump
target handling. If unsure, take option (a): build a `HashSet<usize>` of
instruction indices that are jump targets, refuse to fuse when
`nxt_idx ∈ targets`.

### `encode.rs` jump-target set

After examining `encode_function`, build a `Vec<bool>` of "is target"
flags. For each `Jmp`/`JmpIfTrue`/`JmpIfFalse` in the function, mark
the resolved target index. Fusion only proceeds when
`!is_target[next_idx]`.

---

## File structure

```
crates/luau-lir/src/types.rs                    # MODIFY: enum OpKind
crates/luau-emit/src/opmap.rs                   # MODIFY: ALL_OPS
crates/luau-emit/src/encode.rs                  # MODIFY: header inv size + fusion logic
crates/luau-emit/src/render.rs                  # MODIFY: opname + jinja key
crates/luau-runtime/assets/vm.luau.j2           # MODIFY: handler + inv loop bound
README.md                                       # MODIFY: status
docs/superpowers/plans/2026-06-03-plan-30-handler-fusion-superops.md  # CREATE
```

---

## Task 1: Add LCLC superopcode end-to-end

- [ ] Add `OpKind::LoadConstLoadConst` and update `ALL_OPS`. `cargo check`
      and resolve all match-exhaustiveness errors. (Most are "fail-loud":
      the variant doesn't come from MIR, only from encode-time fusion,
      so an `_ => panic!("internal: superop reached lowering")` arm is
      fine in any context that lowers MIR → LIR.)
- [ ] Adjust `inv[]` header size in both encoder and template. Use
      `ALL_OPS.len()` as the source of truth.
- [ ] Add the dispatcher handler.
- [ ] Add the encode-time fusion logic with jump-target gating.
- [ ] Unit test in `encode.rs`: synthesize a 3-instruction LIR sequence
      `(LoadConst, LoadConst, LoadConst)` and verify with `rng = [0; 32]`
      that exactly 1 LCLC + 1 LC are emitted (after seeded coin flip
      lands favorably for the first pair).
- [ ] Run full corpus.

- [ ] Commit: `feat(emit): LoadConstLoadConst handler-fusion superop`.

## Task 2: Acceptance test

- [ ] `lc_lc_fusion_round_trips` — obfuscate a program with lots of
      adjacent LoadConsts (e.g., 50 lines of `local x = 1; local y = 2;
      ...`) over 5 seeds, run via `luau`, assert deterministic output.
- [ ] Commit: `test: Plan 30 acceptance — LCLC fusion preserves
      semantics`.

## Task 3 (optional, stretch): MoveMove

If the LCLC patch lands cleanly and corpus is green, repeat for `Move`.
Skip if anything else has been hairy.

## Task 4: Status

- [ ] README. Commit: `docs: mark Plan 30 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green.
- [ ] 25-file 8-seed corpus — green.
- [ ] Stage-1 VM template (in the obfuscator output) contains a
      handler whose body reads 4 u16 operands and does two table
      lookups — the LCLC handler.
- [ ] Decoded bytecode shows opcode 36 (= ALL_OPS.len()) referenced in
      at least one corpus program with adjacent LC instructions.
