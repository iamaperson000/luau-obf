# Known Bugs

## Multi-assign swap miscompiles

**Symptom.** `a, b = b, expr(a, b)` produces wrong values after obfuscation,
even with NO obfuscation passes enabled beyond the identity transform.

Minimal repro:
```luau
local a, b = 1, 2
a, b = b, a
print(a, b)   -- plain: 2  1
              -- obf:   2  2
```

**Diagnosis (incomplete).** MIR lowering for `AssignMulti` emits a
`BuildResults` table from the RHS expressions, then `GetIndex` + `Move` for
each LHS target. The MIR appears semantically correct (the table snapshots
RHS values before any LHS slot is overwritten). The bug therefore lives in
LIR lowering, register allocation, or the VM template's `BuildResults` /
`Move` handler. Has been present at least since Plan 15 (likely earlier);
the current corpus doesn't include reassignment-style multi-assign so CI
never tripped on it.

**Impact.** Programs that use `a, b = b, ...` swap-style multi-assign get
miscompiled. Workaround: use explicit temporaries:

```luau
local t = b
b = a % b
a = t
```

**Action required.** Add corpus program(s) exercising reassignment-style
multi-assign once the bug is fixed, so future regressions are caught.
