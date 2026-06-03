# Roadmap — Post-Plan-15

**Last updated:** 2026-06-03
**Shipped through:** Plan 15 (MIR expression mangling — Sub → Neg+Add)

Working list of the next 10 plans, grouped by theme and ordered by "small lift
first, then escalating impact". Order is a recommendation, not a contract — any
plan can be promoted or deferred if priorities shift.

---

## Group A — Finish the MIR expression-mangling family

Each plan reuses the per-instance, seed-keyed rewrite scaffolding introduced
in Plan 15. Small, parallel-shaped passes; ship fast.

### Plan 16 — Add identity-padding

Rewrite `a + b` → `a + b + k − k` (or `(a + k) + (b − k)`), with `k`
seed-mixed per instance. Removes the 1:1 of Add-in-source ↔ Add-in-bytecode,
same as Plan 15 did for Sub.

### Plan 17 — Mul rewrites

`x*2 → x+x`; `x*2^n → x<<n` for integer-typed muls; algebraic identities on
integer literals. Forces a deobfuscator to track type information.

### Plan 18 — Boolean & comparison mangling

De Morgan rewrites, `==` ↔ `not (… ~= …)`, `>` ↔ `not (… <= …)`,
branch-polarity flips. Cheap to implement, devastating to pattern-matchers.

### Plan 19 — Constant decomposition

Literal `N` → seed-derived arithmetic on smaller constants; composes with
Plans 11/12 (encrypted constants) so even the post-decryption value is
synthetic.

---

## Group B — Control-flow obfuscation

New MIR territory. Biggest single defense win against LLM / static analysis.
Ordered so the harder passes land on top of the prerequisites they need to
remain durable.

### Plan 20 — Opaque predicates

Insert always-true / always-false guards built from seed-known arithmetic
identities; route real code one way, junk the other.

### Plan 21 — Bogus basic-block insertion

Unreachable-but-typecheck-valid junk blocks woven into the CFG, referenced
from dead opaque branches produced by Plan 20.

### Plan 22 — Control-flow flattening

Lift every function body into a `while true / switch state` dispatcher.
The big one. Only durable once Plans 20 + 21 are in place — without them a
deflattener trivially recovers the original block order.

---

## Group C — VM diversification

Extends Plan 13's per-proto permutation idea upward. Pays off most when the
front-end (A + B) already produces highly varied bytecode.

### Plan 23 — Handler-shape mutation

Per build, randomly split / merge VM handlers, reorder operand decoding, vary
register-vs-stack access mix.

### Plan 24 — Synonym opcodes

Multiple distinct opcodes that mean the same thing (e.g. three different ADDs
that decode operands differently). Inflates the opcode space, breaks
frequency analysis.

### Plan 25 — Dispatcher diversification

Pick from a small pool of dispatch shapes (token-threaded loop, computed-
jump-style chain, nested-if ladder) at build time. Combined with Plans 14 +
23, the static "shape" of two builds shares almost nothing.

---

## Sequencing rationale

- **A first** — mechanical, uses infrastructure that already exists, locks in
  the per-instance / seed-keyed rewrite pattern.
- **B next** — where deobfuscation difficulty really climbs. Flattening (22)
  depends on opaque predicates (20) and junk blocks (21) to muddy any
  deflattener pass.
- **C last** — deepest lift; biggest payoff once front-end already produces
  highly varied bytecode for the diversified VM to consume.
