# Luau Obfuscator — Plan 26: Junk arithmetic on live VLocal operands

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Make the junk dead-store chains in `junk_arith.rs` and `junk_sub.rs`
*sometimes* read **already-live** VLocals as the operands of the *first* binop
in the chain, instead of always pulling two fresh `LoadConst` values. This
defeats the adversary's constant-folding pivot from Round 5: a static analyzer
that sees `LoadConst δ1; LoadConst δ2; Add(δ1,δ2); Mul(prev,δ1)` can fold
everything to `dead = δ1*(δ1+δ2)` and erase the chain. If the operands are
live `regs[x]` (whose value isn't statically known), folding fails.

Why this is the adversary's *cheapest* pivot to kill: it requires zero new
runtime machinery and is contained to the two pass files.

---

## Threat-model framing

Current shape (both passes):

```
δ1 = LoadConst(c1)
δ2 = LoadConst(c2)
t1 = δ1 OP1 δ2          -- OP1 ∈ {Add, Sub}
t2 = t1 OP2 δ1          -- OP2 ∈ {Mul, Sub}
```

After Plan 26 (one possible shape, ~50% of injections):

```
t1 = lv1 OP1 δ1         -- lv1 is a live VLocal already assigned earlier in this block
t2 = t1   OP2 δ2        -- t2 still dead-stored
```

Where `lv1` is a `VLocal` that has been written by an earlier instruction
in the *same* block (so it's guaranteed live and well-defined at the
insertion point).

The result VLocals (`t1`, `t2`) remain dead — the chain still produces no
observable effect — but the *operands* are no longer constants.

**Safety.** Operands are arithmetic-typed live values. We must restrict
`lv1` to a VLocal whose most recent write is `LoadConst Number(_)` or
arithmetic, so the result type is a number. Reading the wrong type
(string, table, bool, nil) would change behavior because the BinOp would
either throw (table) or coerce (string). Picking only Number-producing
writes keeps the chain a pure no-op that still throws nothing.

---

## Implementation

**Mode pick.** At each injection site, draw `mode = rng.gen::<u8>() % 2`:
- `mode == 0`: existing behavior (two fresh consts).
- `mode == 1`: live-operand mode — replace the first BinOp's left operand
  (`lhs`) with a previously-written number-producing VLocal. Fall back to
  mode 0 if no candidate exists.

**Candidate VLocals.** Before injecting at position `pos`, scan
`block.instrs[..pos]` for instructions whose destination is provably a
`Number`. The conservative set is:
- `Instr::LoadConst { dst, src }` where `f.consts[src.0]` is
  `Constant::Number(_)`.
- `Instr::BinOp { dst, op, .. }` where `op ∈ {Add, Sub, Mul, Div, Mod, Pow}`
  AND both operands are themselves provably-Number (transitive). Simpler
  acceptance: only allow `BinOp` whose `lhs` and `rhs` are both
  `Value::VLocal(x)` where `x` is in the provably-Number set, or
  `Value::Const(c)` where `f.consts[c.0]` is Number. This is recursive;
  cache results in a `HashSet<u32>` (VLocal index) per block scan.
- (Skip `UnOp::Neg` for simplicity — small recall hit, no correctness risk.)

Pick uniformly from the candidate set with `rng.gen::<u32>() as usize % set.len()`.

**Drop one constant.** When `mode == 1`, only one constant is needed (for
the second BinOp's `rhs`). Drop the unused `LoadConst δ1` from the
emitted instruction sequence and reuse the existing `δ2` for the second
op. Update the emit accordingly: 3 instructions instead of 4. Update
`n_locals` bookkeeping (only 3 fresh VLocals: `δ2_vl`, `tmp1`, `tmp2`).

**RNG schedule.** Existing: 1 byte (inject roll) + 1 u32 (pos) + 2 u32s
(δ1, δ2) per injection. New: ALWAYS draw the same bytes regardless of
mode pick, so injection count and rng position stay seed-deterministic.
Order: roll → pos → mode → δ1 → δ2 → (live operand index if mode 1).
Always draw δ1 even if unused in mode 1 — it keeps the rng schedule
mode-invariant.

**Apply to both passes.** Make the change in `junk_arith.rs` AND
`junk_sub.rs`. Same percentage threshold each pass currently uses
(`INJECT_PERCENT` unchanged).

---

## File structure

```
crates/luau-passes/src/
  junk_arith.rs            # MODIFY: add candidate scan + mode pick
  junk_sub.rs              # MODIFY: same
  analysis.rs              # OPTIONAL: extract shared Number-typed VLocal scan
README.md                  # MODIFY: status line
docs/superpowers/plans/2026-06-03-plan-26-junk-arith-live-operands.md  # CREATE
```

A shared helper in `analysis.rs` is preferred (DRY), but a duplicated
helper inside each pass file is acceptable. Reviewer to decide.

---

## Task 1: Implement live-operand mode in both junk passes

- [ ] Write the shared scanner. Signature suggestion:
  ```rust
  /// Returns the set of VLocal indices that are provably Number-typed
  /// at the END of position `before` (exclusive) in this block, given the
  /// function's constant pool.
  pub fn number_vlocals_before(
      block: &luau_mir::BasicBlock,
      before: usize,
      consts: &[luau_mir::Constant],
  ) -> std::collections::HashSet<u32>;
  ```
  Implement in `crates/luau-passes/src/analysis.rs`.

- [ ] In `junk_arith.rs`: after picking `pos`, compute candidates. Draw
  `mode`. If mode==1 and candidates.is_empty(), fall through to mode 0.
  In mode 1, emit:
  ```
  d2_vl = LoadConst(δ2)
  tmp1  = Add(VLocal(live), VLocal(d2_vl))
  tmp2  = Mul(VLocal(tmp1), VLocal(d2_vl))
  ```
  (or any deterministic choice — but use d2_vl for both spots so the
  shape stays predictable for the spec reviewer).

- [ ] Same in `junk_sub.rs` with `Sub` substituted for `Add`/`Mul`.

- [ ] Existing unit tests must keep passing — but the assertion
  `instr_growth == 4 * (consts_growth / 2)` will break. Generalize:
  count *injections* by counting the new `tmp2` VLocal assignments
  (i.e., by counting Adds added with `lhs = ... rhs = ...`). Or change
  the assertion to: `instr_growth in {3*n, 4*n} for some n`. Pick one
  clear refactor.

- [ ] Add a new test `mixes_live_and_const_operands` to each pass file:
  sweep seeds 0..200, assert at least one seed produces a BinOp whose
  lhs is a VLocal that was *not* assigned by the pass itself
  (i.e., the index is `< n_locals_before_pass`).

- [ ] Run `cargo test -p luau-passes --release`. Commit:
  `feat(passes): junk-arith/junk-sub can read live VLocals as operands`.

## Task 2: Acceptance test in luau-obf

- [ ] Add `junk_arith_live_operand_changes_output` and run via `luau` to
  verify semantics — pow2(10) should still print 1024.

- [ ] Run full workspace tests + the 25-program corpus harness.

- [ ] Commit: `test: Plan 26 acceptance — live-operand junk preserves semantics`.

## Task 3: Status bump in README

- [ ] One paragraph update. Commit: `docs: mark Plan 26 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green.
- [ ] 25-file 8-seed corpus — green.
- [ ] Sweep 200 seeds × `pow2(10)` test — output always `1024`.
- [ ] `mixes_live_and_const_operands` passes (≥1 live-operand injection).
- [ ] RNG schedule mode-invariant (same seed → identical instruction
  count, even though shapes differ).
