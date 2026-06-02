# Luau Obfuscator — Design

**Date:** 2026-06-01
**Status:** Approved for implementation planning

## Goal

Build the most powerful Roblox-targeted Luau obfuscator we can: a VM-based,
loadstring-style protection tool comparable in strength to Luraph / Moonsec V3.
Input is a Luau source file; output is a single self-contained Luau chunk that
runs through a custom virtual machine.

## Use case and assumptions

- **Target:** Luau as it runs in Roblox (server, client, executor environments).
- **Output:** One self-contained Luau file that can be `loadstring`-ed or
  pasted directly.
- **Executor compatibility:** The obfuscated chunk must continue to work with
  executor-only globals such as `getgenv`, `hookfunction`, `getrawmetatable`,
  `loadstring`, etc. The VM does not sandbox the environment; it runs under
  the caller's `fenv` unchanged.
- **Tool itself:** Rust. The obfuscator binary stays server-side and is never
  shipped to end users (a web wrapper is anticipated but not in scope here).
- **Strength budget:** Strength over speed. A 10–100× runtime slowdown is
  acceptable. This is consistent with how heavyweight obfuscators in this
  space are used today.
- **Language scope:** Full Luau — core Lua 5.1, plus Luau control-flow extras
  (`continue`, compound assignments, if-then-else expressions, generalized
  iteration), Luau syntax extras (string interpolation, type annotations
  which are stripped, type assertions, attributes), and Luau runtime types
  (`vector`, `buffer`, `bit32`).

## Non-goals (v1)

- Web frontend and backend service. Will be added later as a thin wrapper
  around the CLI / library API.
- Module bundling (following `require(...)` and inlining modules). Single
  file in, single file out for v1.
- Fuzzing infrastructure. The curated test corpus plus property tests cover
  more ground for the effort.
- Sub-1× or near-1× performance modes. The strength target precludes this.

## Architecture

A Rust workspace with one crate per pipeline stage. Each stage has its own
IR type, so the type system enforces stage ordering and prevents accidental
cross-layer coupling.

```
luau-obf/
├── crates/
│   ├── luau-parse/        # Luau source → AST (wraps full_moon)
│   ├── luau-hir/          # AST → HIR: scope resolution, type-annotation stripping
│   ├── luau-mir/          # HIR → MIR: desugar, normalize CF, basic blocks, SSA-ish
│   ├── luau-passes/       # MIR → MIR obfuscation transforms (registry + scheduler)
│   ├── luau-lir/          # MIR → LIR: register allocation, custom-ISA bytecode
│   ├── luau-emit/         # LIR + VM template → final Luau output chunk
│   ├── luau-runtime/      # VM source as a templated asset (Tera or MiniJinja)
│   ├── luau-obf/          # Top-level facade: obfuscate(source, opts) → Result<String>
│   └── luau-obf-cli/      # Thin CLI over luau-obf: file in → file out, flags, seed
├── runtime-tests/         # Differential execution corpus + harness
└── docs/
```

The `luau-obf` facade crate exposes
`fn obfuscate(source: &str, opts: Options) -> Result<String>` for future
in-process use by a web service or other host.

### Two design invariants

1. **No crate above `luau-emit` knows the final opcode numbering.** Opcodes
   are assigned at emit time from the build's seed. All upstream stages refer
   to opcodes by symbolic name. This makes per-build randomization free.
2. **The VM is data, not code, to most of the pipeline.** `luau-runtime` ships
   templated Luau source. `luau-emit` renders it against the same seed that
   chose the opcode layout, so the VM and bytecode match by construction.

## Pipeline stages

```
source.lua
  │
  ▼ luau-parse                       full_moon + Luau extensions
AST  — concrete syntax; every Luau feature representable
  │
  ▼ luau-hir                         strip types, name-resolve, assign SymbolIds
HIR  — Luau-shaped, typed-out; every identifier carries a unique SymbolId
  │
  ▼ luau-mir                         desugar + normalize control flow
MIR  — sugar-free; control flow is explicit basic blocks with terminators
       (continue/break/compound-assign/if-expr/string-interp all lowered;
        for-in uses explicit iterator triple; locals in SSA-ish form)
  │
  ▼ luau-passes                      ordered MIR → MIR transforms
MIR' — same shape, semantically equivalent, mangled per the pass plan
  │
  ▼ luau-lir                         register allocation + bytecode lowering
LIR  — linear custom-ISA bytecode; symbolic opcodes; constant pool
  │
  ▼ luau-emit                        per-build randomization + VM template render
out.lua — one self-contained Luau chunk:
            • encrypted constant pool
            • encoded bytecode
            • templated VM with randomized opcode IDs and handler layout
            • bootstrap that decrypts, sets up env, dispatches
```

### Why this many layers

Each stage holds one idea in mind:

- **HIR** — what identifiers refer to, without worrying about control flow.
- **MIR** — what the control-flow graph looks like, without register pressure.
- **LIR** — register layout and opcode selection, without source-level constructs.
- **Emit** — making it look unrecognizable, without semantic concerns.

Obfuscation passes live between MIR and LIR. They operate on a canonical,
sugar-free representation, which is dramatically smaller surface area than
raw AST.

### Pass scheduler

`luau-passes` exposes a `PassPlan`: an ordered list of named passes with
parameters. The CLI builds the plan from flags + seed. A future
"tunable strength" mode is just a different plan. Passes can be repeated
(`flatten → predicate → flatten` is intentional — the first pass exposes
structure that the second can flatten further).

## Obfuscation pass catalog

Passes are grouped by which stage of the pipeline they run at. The grouping
isn't cosmetic — each stage's IR is what makes the pass cheap to write
correctly.

### HIR (semantic) passes

- **Symbol renaming** — every local, parameter, and upvalue gets an opaque
  name (`_, __, ___, …` or visually-confusable Unicode if requested). Cheap
  because HIR already carries `SymbolId`s.

### MIR (control-flow / expression) passes — the bulk of static obfuscation

- **Expression mangling** — arithmetic and boolean ops rewritten into
  equivalent forms (`a + b → (a XOR b) + ((a AND b) << 1)`,
  `not x → x == nil or x == false`, identity-preserving table indirection).
  Multiple recipes per construct; the pass picks per node with seeded RNG.
- **Constant lifting** — every literal extracted into a single constant pool
  with an opaque index. Actual encryption is deferred to emit so keys are
  per-build.
- **Opaque predicates** — inject branches guarded by predicates that are
  statically hard to evaluate (`(x * x) % 4 < 2` for integer `x`;
  pair-of-functions whose composition is identity; etc.). The pass also
  marks which side is "real."
- **Control-flow flattening** — for each function body, rebuild the CFG as
  a single `while true / switch(state)` dispatcher. Original block order
  erased; state-variable updates are themselves obfuscated.
- **Junk MIR insertion** — add basic blocks that look real, are reachable
  only through always-false predicates, and call into bogus handlers in
  the VM.

### LIR (bytecode) passes

- **Register layout shuffling** — register allocator deliberately scatters
  rather than packing tightly; long-lived values get cold registers; same
  source produces different layouts per build.
- **Constant pool shuffling** — order randomized; index references rewritten.

### Emit-stage (per-build randomization)

- **Opcode renumbering** — opcode integer IDs assigned freshly per build
  from seed. No build shares an opcode map with another.
- **Handler dispatch mutation** — randomly pick one of: nested `if/elseif`
  tree, table-of-closures, or string-keyed dispatch. Per build.
- **Bytecode encoding** — variable-length per-build encoding; XOR roll with
  key derived from a non-trivial function of the byte index.
- **Constant pool encryption** — multi-layer (XOR → byte-permute → XOR)
  with per-build keys; decryption happens lazily on first use inside the VM.
- **VM template render** — handler order shuffled; internal variable names
  mangled; optional split of the VM across multiple closures with
  cross-references.
- **Bogus handler insertion** — emit handlers for opcodes that aren't
  actually used; junk MIR's "fake" calls target these so the dispatch table
  looks fully populated.

### Runtime (baked into VM template, not a transform)

- **Global pre-cache** — `pcall`, `error`, `select`, `string.sub/byte/char`,
  `table.unpack/create`, `math.floor`, `bit32.*`, captured into upvalues at
  VM init. Later global hooks can't affect VM internals.
- **Integrity self-check** — VM hashes its own bytecode and dispatch table
  at init and intermittently; mismatch quietly corrupts state (no crash,
  no error string).
- **Debug-env detection** — probes `debug.getinfo` against a sentinel; if
  introspection is happening, VM diverges silently.

### Strength compounding

Two structural reasons strength is more than the sum of passes:

1. **Pass repetition.** The default plan runs `flatten → predicate → mangle
   → flatten` — once exposes structure to the second pass.
2. **Per-build determinism via seed.** Every randomization above pulls from
   one `ChaCha20` seeded by `--seed` (or a fresh random one). Same input +
   same seed → bit-identical output. Critical for testing and reproducible
   builds.

## VM execution model

Register-based interpreter, written in Luau, templated by `luau-runtime`.
Register-based (over stack-based) because:

- Luau itself is register-based; opcode count and dispatch overhead are lower,
  which matters at 10–100× slowdown.
- Maps cleanly to MIR's SSA-ish locals during register allocation.
- Easier to obfuscate: registers are just indices into a Luau table, easy to
  mutate mid-execution.

### Per-function frame

On VM function entry the runtime allocates a frame table with:

- `regs` — array of registers (one Luau table; indices scrambled per build).
- `pc` — program counter into the function's encoded bytecode.
- `consts` — reference to the function's constant pool slice.
- `upvals` — captured upvalues. Luau closures wrap real Luau closures around
  frame slots, so capture semantics are correct for free.
- `varargs` — `...` storage when the function takes them.

### Instruction format

Logical shape (physical encoding is randomized per build):

- 1 byte opcode + variable-length operands.
- Operands typed: register index, constant index, jump offset, immediate
  small int, upvalue index.
- Each operand goes through a per-build XOR-roll using a key that depends
  on the operand's position in the stream.

### Dispatch

Main loop is the templated dispatch — if-tree, closure-table, or string-key,
picked per build. After every `N` instructions (`N` itself randomized) it
runs an integrity tick.

### Calling convention

- Multi-return is native: `CALL` writes results back into a destination
  register range; `RETURN` packs a variadic.
- Tail calls: `TAILCALL` opcode reuses the current frame to preserve
  Luau's tail-call semantics.

### Closures

A VM closure value is `{ proto_id, upvalues_table }`. When called, the VM
allocates a fresh frame for proto `proto_id` and binds upvalues.
Lua-host-callable: closures are wrapped in a thin native Luau function
(`function(...) return vm_call(proto_id, upvals, ...) end`) so passing them
to `table.sort`, event connections, etc., works correctly.

### Luau-specific opcodes

First-class instead of decomposed, for correctness and to avoid catastrophic
slowdown on idiomatic Luau:

- `VECTOR3_NEW`, `VECTOR3_OP` (component arithmetic short-circuits)
- `BUFFER_READ_*`, `BUFFER_WRITE_*` (otherwise we lose buffer's speed entirely)
- `BIT32_*` family
- `STRING_INTERP_BUILD` (cheaper as one op than N concats)
- `FORG_PREP`, `FORG_STEP` (generalized iteration — handles the `__iter` path)

### Environment

The obfuscated chunk runs in its caller's environment unchanged. The VM
does not sandbox or override globals. Executor globals (`getgenv`,
`hookfunction`, `loadstring`, `getrawmetatable`, etc.) are accessed through
the same `fenv` as the host script, so executor-dependent code keeps working.

### Errors

VM uses `xpcall` to catch Luau errors and re-raises them with the original
message scrubbed of VM-internal frames. The user sees their own `error("…")`
message, not VM internals.

## CLI and error handling

```
luau-obf <input.lua> -o <output.lua>
         [--seed <hex>]           # default: random; printed to stderr for reproduction
         [--plan <name|path>]     # default: "max"; built-in presets or path to a TOML
         [--report <path>]        # optional: write strength stats as JSON
         [--quiet]                # suppress stderr progress
```

**Exit codes**

- `0` — success.
- `1` — user error (bad input file, parse failure).
- `2` — internal error (bug in the obfuscator). Backends use the
  distinction to decide whether to retry or surface.

**Output destinations**

- Stdout is never used for the obfuscated chunk (avoids accidental terminal
  streaming).
- Errors are one JSON object per line on stderr when `--quiet` is off, so a
  backend can parse them without regex.
- Obfuscated chunk is written only to the `-o` path.

**Error model** — `thiserror`-based, three layers:

- `ParseError` — `full_moon` errors, wrapped with source span. Always
  user-facing.
- `LoweringError` — unsupported construct (escape hatch for future Luau
  features). User-facing with "please file an issue."
- `InternalError` — pass invariant violation, register-allocator panic, etc.
  Caught at the CLI boundary, logged with the seed and a redacted source
  snippet, surfaced as exit-code 2.

The pipeline crates do not print or log. Only `luau-obf-cli` formats output.
This keeps the core library-clean for an eventual web wrap.

**Determinism guarantee** — if a build with seed `S` fails, re-running with
`--seed S` reproduces the failure. The seed is the first thing printed on
stderr.

**Library API** — `luau-obf-cli` is the thin shell over the top-level
`luau-obf` crate's `obfuscate(source, opts)` function. A future web service
imports this and skips the subprocess entirely.

## Testing strategy

The whole design lives or dies on semantic equivalence: obfuscated output
must behave identically to the source for every Luau feature, including the
weird ones.

1. **Unit tests per crate.** Each pass tested against hand-written IR
   fixtures. Fast; run on every commit.

2. **Snapshot tests for IR transforms.** `insta`-based. Each MIR/LIR pass
   has `before → after` snapshots. Regressions are loud; intentional
   changes are a one-command review.

3. **Differential execution tests — the core safety net.** A corpus in
   `runtime-tests/corpus/` that:
   - Covers every language feature (one program each for: closures,
     varargs, multi-return, metatables, `continue`, compound assign, string
     interp, generalized for, vector, buffer, bit32, mixed
     array/hash table constructors, …).
   - Covers obfuscator-stressing patterns (deep recursion, tail calls,
     error/pcall round-trips, coroutine yielding through obfuscated code).
   - Each program is self-checking: it computes something and `assert`s on
     the result.

   The harness runs each program twice — plain and obfuscated — and requires
   identical `print` output and exit status. Under two interpreters:

   - **Luau standalone** (`luau` CLI), every commit.
   - **Roblox Studio headless** via `run-in-roblox`, nightly. Catches
     environment-parity issues (`task.wait` semantics, executor-shaped
     globals).

4. **Property tests.** `proptest` over a small Luau-expression grammar.
   Generate random expressions, run plain vs obfuscated, assert equal.
   Catches arithmetic-mangling and operand-encoding regressions.

5. **Determinism tests.** Same input + same seed must produce
   byte-identical output. CI runs the corpus with a fixed seed and diffs
   against a committed hash manifest.

6. **Strength tests (qualitative, not gated).** A `--report` mode emits
   stats: opcodes used, dispatch shape, junk ratio, constant-pool entropy,
   predicate count. Used to tune the default plan and notice accidental
   weakening.

### Explicitly deferred

- **Fuzzing.** Curated corpus + proptest cover more ground for the effort.
  Fuzzing the parser is mostly fuzzing `full_moon`, which is upstream's job.
- **Roblox Studio tests on every commit.** Headless Studio is slow and
  flaky; nightly is the right cadence.

## Open questions

None at spec time. Any opcode-set additions or specific predicate recipes
will be decided during implementation against the test corpus, not in
advance.
