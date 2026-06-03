# Luau Obfuscator — Plan 31: Dispatcher split via closure table

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal.** Round 1/5 finding: the dispatch in `vm.luau.j2` is one big
`if/elseif` ladder anchored to opcode constants. The adversary identifies
all 35 handlers by reading one function. Splitting the dispatcher into
N closure-shaped handler groups forces the adversary to identify each
group separately — they need to confirm group membership and the
per-group sub-dispatch.

**Scope (minimum viable).** Split into **3 closure groups**:
- Group A: arithmetic + comparison (Add, Sub, Mul, Div, Mod, Pow, Lt,
  Le, Eq, Not, Neg, Len, Concat) — ~13 ops.
- Group B: load/move/global/table/upval (LoadNil, LoadTrue, LoadFalse,
  LoadConst, Move, GetGlobal, SetGlobal, NewTable, GetTable, SetTable,
  GetUpval, SetUpval, Vararg) — ~13 ops.
- Group C: control flow + calls/closures (Call, Return, Jmp, JmpIfTrue,
  JmpIfFalse, Closure, CallVar, BuildResults, ReturnMulti) — ~9 ops.

Plus LCLC from Plan 30 (if landed): goes in Group B.

**Dispatch shape.**

```luau
local function handle_group_a(op, regs, code, pc, pid, consts)
    -- Returns: new_pc, "continue" / "return" / "return_multi", optional value
    if op == ADD then ...; return pc, "continue"
    elseif op == SUB then ...; return pc, "continue"
    ...
    end
    return pc, "miss"  -- not in this group
end
```

Main loop:

```luau
while pc <= code_len do
    local raw = _byte(code, pc, proto_id)
    local op = inv[raw]
    pc = pc + 1
    local status
    pc, status = handle_group_a(op, regs, code, pc, proto_id, consts)
    if status == "miss" then pc, status = handle_group_b(op, regs, code, pc, proto_id, consts) end
    if status == "miss" then pc, status, ret_val = handle_group_c(op, regs, code, pc, proto_id, consts, upvals, frame_varargs) end
    if status == "miss" then _error(_tostring(op)) end
    if status == "return" then return ret_val end
    if status == "return_multi" then return _tunpack(ret_val, 1, ret_val.n) end
end
```

---

## Constraints / design notes

- **Mutable `regs`** — `regs` is a Lua table; the handlers receive it
  and mutate its contents (table mutation propagates).
- **`upvals` and `frame_varargs`** — only Group C needs these. Pass them
  only to Group C to keep groups A/B narrower.
- **`pc`** — passed and returned; non-jump opcodes increment by their
  operand size; jumps return a different value.
- **Return signals** — `Return`/`ReturnMulti` need to break out of the
  main `while` loop. Use status strings (`"return"`, `"return_multi"`)
  with the optional return value as a third return.
- **Group C is closure-heavy** — `Closure` captures `vm_call` and
  `upvals`. `vm_call` is a recursive local; the group-C closure needs to
  see it via its own upvalue. This works because the group functions
  are declared AFTER `vm_call` is forward-declared.

---

## Implementation

### `crates/luau-runtime/assets/vm.luau.j2`

This is a pure-template change — no Rust-side changes needed since
opcode counts and rendering are unchanged. Just refactor the existing
`while/if` ladder into:

1. `vm_call` declaration (existing).
2. Forward-declare `handle_group_a, handle_group_b, handle_group_c`.
3. Define each group as a `local function group_X(...)` body — copy the
   corresponding `elseif` clauses verbatim, adjust `pc` to be a
   parameter + return.
4. Main loop calls the groups in fixed sequence (A, B, C). At runtime
   the order is fixed but the same op always goes to the same group;
   the order itself doesn't matter for correctness.

**Important:** when an `elseif` body modifies `pc` mid-handler (e.g.,
`Call`'s `pc = pc + 2`), make those modifications happen on the local
`pc` parameter inside the group function, then return the new `pc`.

**Important:** the `Return` and `ReturnMulti` handlers don't increment
`pc`; they early-return. In the group-function form, return `(pc,
"return", value)` or `(pc, "return_multi", tbl)`.

### Closures and recursive call

`Closure` handler creates `function(...)` that calls `vm_call(...)`.
Inside Group C, the closure can reference `vm_call` because the group
function is defined inside `vm_call`'s lexical scope (i.e., after the
`local vm_call` forward declaration). **Define the groups INSIDE
`vm_call`** so they see `vm_call`, `upvals`, etc. naturally.

Wait — that would lose the upvalue-indirection benefit, because the
group closures would still all share the `vm_call` scope.

**Compromise.** Define groups OUTSIDE `vm_call`, accept `vm_call_ref`
as a parameter to Group C. Use a forward-declaration pattern: declare
`local vm_call` at top of file before defining the groups, then assign
`vm_call = function(...) ... end` afterwards. Groups close over
`vm_call` via upvalue capture.

This is the conventional Luau forward-reference pattern.

### Per-build randomization (stretch)

Plan 7's per-build randomization could shuffle the group order at the
template level: a seed-driven u8 decides whether to dispatch A→B→C or
C→A→B etc. Group membership stays fixed (otherwise the partition has
to be communicated to the dispatcher); only the dispatch-trial order
varies. This is a small win — defer to Task 3 if time permits.

---

## File structure

```
crates/luau-runtime/assets/vm.luau.j2   # MODIFY: full restructure into 3 groups
README.md                                # MODIFY: status
docs/superpowers/plans/2026-06-03-plan-31-dispatcher-split.md  # CREATE
```

(No Rust changes expected — `render.rs` passes the same minijinja
context; the template internally restructures.)

---

## Task 1: Refactor template into 3 groups

- [ ] Identify exact opcode-to-group mapping (use the lists above).
- [ ] Forward-declare `vm_call` and the three group functions.
- [ ] Move the existing `elseif` clauses into the corresponding group's
      body. Adjust `pc` handling.
- [ ] Update the main loop to call groups in A→B→C order with `"miss"`
      fallback.
- [ ] Handle `Return`/`ReturnMulti` signals correctly.

- [ ] Run the corpus: `cargo test --workspace --release`. **Expect
      multiple iterations of fixing.** The dispatcher is the most
      stress-tested code in the codebase; subtle off-by-ones in `pc`
      handling will fail the corpus immediately.

- [ ] Commit: `feat(runtime): split dispatcher into 3 closure-based
      handler groups`.

## Task 2: Acceptance test

- [ ] Existing 25-program corpus IS the acceptance test. No additional
      unit test needed — the dispatcher change is pure structural and
      must produce identical observable behavior.
- [ ] Add a smoke test in `crates/luau-obf/src/lib.rs`:
      `dispatcher_split_works` that obfuscates and runs the same pow2(10)
      program under 4 seeds and asserts "1024" is printed each time.
- [ ] Commit: `test: Plan 31 acceptance — split dispatcher round-trips`.

## Task 3 (optional, stretch): Per-build group order randomization

Defer if Task 1 needed >1 iteration. If landing cleanly, add seed-driven
dispatch order.

## Task 4: Status

- [ ] README. Commit: `docs: mark Plan 31 status`.

---

## Verification checklist

- [ ] `cargo test --workspace --release` — green.
- [ ] 25-file 8-seed corpus — green.
- [ ] Stage-1 VM template contains three top-level
      `local function handle_group_*` definitions, each with a subset
      of the original handlers.
- [ ] Main dispatcher loop is now ≤ 30 lines (was ~240).
