# Luau Obfuscator — Plan 6: Metatables and Luau-Native Runtime Types

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cover the rest of Luau's everyday programming surface — metatables (`setmetatable`, `__index`, `__newindex`, `__call`, arithmetic metamethods) and the standalone-Luau built-in modules `bit32`, `buffer`, and `vector` — through the differential corpus.

**Architecture insight (key to why this plan is small):** Our VM's `GetTable`, `SetTable`, `Call`/`CallVar`, and arithmetic opcodes delegate to Luau's host `[]`, `[]=`, `(call)`, and `+`/`-`/etc. operators. Those operators *already* dispatch metamethods on the host VM. Closures created by our `Closure` opcode are wrapped in a thin Lua function (`function(...) return vm_call(...) end`), so when a metamethod is one of our closures, the host VM calls the wrapper and execution re-enters the obfuscated bytecode. Similarly, `setmetatable`, `getmetatable`, `rawget`, `rawset`, `bit32.*`, `buffer.*`, and `vector.*` are reached via the existing `GetGlobal` path — no new opcodes required.

If this theory is wrong, the corpus tests will fail. Tasks 6–7 explicitly handle that case: if a feature requires real pipeline work, fold the required work into a follow-on sub-task at that point rather than guessing now.

**Tech Stack:** Same as Plans 1-5.

**Plan 6 language additions:**

| Newly covered | Still deferred |
|---|---|
| `setmetatable(t, mt)`, `getmetatable(t)`, `rawget`, `rawset` | First obfuscation pass — Plan 7 |
| `__index` as table (inheritance) and as function (computed lookup) | `__metatable` field (protected metatables) — defer until needed |
| `__newindex` as table and as function | `__gc`, `__mode` (weak tables / finalizers) — defer until needed |
| `__call` (a callable non-function value) | `__pairs` / `__iter` — host VM handles whatever it supports |
| Arithmetic metamethods through the host VM: `__add`, `__sub`, `__mul`, `__div`, `__mod`, `__pow`, `__unm`, `__concat`, `__eq`, `__lt`, `__le`, `__len`, `__tostring` | Roblox-specific globals (`Vector3`, `Instance`, etc.) — out of scope; standalone-Luau `luau` CLI doesn't expose them anyway |
| `bit32.band`, `bor`, `bxor`, `bnot`, `lshift`, `rshift`, `arshift`, `extract`, `replace`, `countlz`, `countrz`, `bswap` | |
| `buffer.create`, `buffer.fromstring`, `buffer.tostring`, `buffer.len`, `buffer.read*`, `buffer.write*`, `buffer.copy` | |
| `vector.create(x, y, z)`, component fields (`v.x`, `v.y`, `v.z`), arithmetic, `vector.magnitude`, `vector.dot`, `vector.cross` (whatever standalone Luau ships) | |

**Out-of-scope reminder:** This plan does NOT introduce Luau-specific opcodes (`VECTOR3_NEW`, `BUFFER_READ_*`, etc.) from the design spec. The host VM's existing operator dispatch handles every case the corpus exercises. Those opcodes are a *performance* optimization the design spec lists for later; this plan validates *correctness* first.

---

## Implementation notes

**Closure-as-metamethod invariant.** Every Plan 1–5 closure is wrapped in `function(...) return vm_call(proto_id, pa, n, captured) end` inside `OP_Closure`. When such a closure is stored as `__index = our_fn` on a metatable and then triggered by a missing-key read, the host VM calls the wrapper — which kicks off a fresh `vm_call`. Captured upvalues stay correct because they were already bound at closure creation time. The same applies to `__newindex`, `__call`, and arithmetic metamethods.

**Standalone Luau host capabilities.** The differential harness runs `luau <file>` on a standalone Luau CLI. That CLI supports `bit32`, `buffer`, and `vector` natively; it does NOT define `Vector3`, `CFrame`, `Instance`, or other Roblox-engine types. Limit the corpus to what `luau` exposes.

**Vector library naming.** Standalone Luau exposes the vector type via two surfaces: `vector.create(x, y, z)` returns a `vector` (a native non-table value), and the field-access syntax `v.x` works via the host VM's projection logic. Component-wise arithmetic (`v1 + v2`) goes through Luau's `+` operator, which our `OP_Add` invokes unchanged. If a particular host doesn't ship `vector.create` (older standalone builds used `Vector3.new`-style names), the corpus may need to gate `vector` cases or skip the file — handle case-by-case during Task 6.

**Buffer library surface.** Use a minimal subset in the corpus: `buffer.create(n)`, `buffer.writeu8(buf, offset, value)`, `buffer.readu8(buf, offset)`, `buffer.fromstring(s)`, `buffer.tostring(buf)`, `buffer.len(buf)`. These are enough to exercise both function-call and table-method paths.

**bit32 surface.** Use `bit32.band`, `bor`, `bxor`, `bnot`, `lshift`, `rshift`. The rest behave identically through the same pipeline; covering all of them is busywork.

**No code changes anticipated outside `runtime-tests/corpus/`.** If any task uncovers a real pipeline gap, treat it as a separate sub-task: capture the failure, fix the underlying issue, then return to the corpus. Don't try to fix-and-extend in the same commit — keep history bisectable.

---

## File Structure (changes only)

```
runtime-tests/corpus/
  metatables.luau       # CREATE — __index, __newindex, __call, __add, getmetatable, rawget/set
  bit32_ops.luau        # CREATE — bit32 module exhaustive enough to validate
  buffer_ops.luau       # CREATE — buffer read/write/length/string roundtrip
  vector_ops.luau       # CREATE — vector.create, field access, arithmetic
README.md               # MODIFY: status line
```

If any task discovers a real pipeline bug, that task's plan grows by however many sub-tasks the fix requires. List the additional file changes there, not here.

---

## Task 1: Smoke-test the theory (one tiny corpus file, end-to-end)

**Files:**
- Create: `runtime-tests/corpus/metatables_smoke.luau`

The point: validate the "metatables Just Work" hypothesis with the smallest possible program BEFORE building a full corpus. If this fails, the whole plan needs rethinking.

- [ ] **Step 1: Create `runtime-tests/corpus/metatables_smoke.luau`**

```luau
-- The bare-minimum metatable test: __index falling through to a parent table.
local parent = { kind = "parent" }
local child = setmetatable({}, { __index = parent })
print(child.kind)  -- "parent" — only works if __index dispatch is reached
print(getmetatable(child).__index == parent)  -- true
```

- [ ] **Step 2: Run the differential harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: every existing corpus program (Plans 1–5) still passes, AND the new `metatables_smoke.luau` produces identical stdout under plain and obfuscated execution.

- [ ] **Step 3 (CONDITIONAL): If `metatables_smoke.luau` fails — STOP and investigate**

Read the diff between plain and obfuscated stdout. Capture the failing transcript in your status report. The most likely failure modes:
- Obfuscated run prints `nil` for `child.kind` → `GetTable` is bypassing `__index`. Investigate `vm.luau.j2`'s `OP_GetTable` handler. Theoretically Luau's `[]` dispatches `__index` natively, so this would only fail if our handler uses some lower-level access path. (It doesn't — it uses `regs[b][regs[c]]` which IS `[]`.)
- Obfuscated run prints `false` for `getmetatable(child).__index == parent` → reference identity is broken somewhere, probably because our table-constructor lowering is creating a fresh table per call. Inspect MIR/HIR.

If a failure occurs, do NOT proceed to later tasks. Report BLOCKED with the transcript and your hypothesis, then await direction.

- [ ] **Step 4: Commit (only if Step 2 passed)**

```bash
git add runtime-tests/corpus/metatables_smoke.luau
git commit -m "test(corpus): metatable smoke test (__index, getmetatable)"
```

---

## Task 2: Metatables — full corpus

**Files:**
- Create: `runtime-tests/corpus/metatables.luau`

- [ ] **Step 1: Create `runtime-tests/corpus/metatables.luau`**

```luau
-- __index as a table (inheritance).
local Animal = { kind = "animal" }
function Animal:describe() return self.name .. " is " .. self.kind end
local dog = setmetatable({ name = "Rex" }, { __index = Animal })
print(dog:describe())  -- "Rex is animal"

-- __index as a function (computed).
local computed = setmetatable({}, {
    __index = function(_, k) return "computed:" .. k end,
})
print(computed.anything)   -- "computed:anything"
print(computed.somethingElse)  -- "computed:somethingElse"

-- Nested __index chain.
local base = { from = "base" }
local mid = setmetatable({}, { __index = base })
local top = setmetatable({}, { __index = mid })
print(top.from)  -- "base"

-- __newindex as a function: log writes, defer to rawset.
local writes = {}
local logged = setmetatable({}, {
    __newindex = function(t, k, v)
        writes[#writes + 1] = k .. "=" .. tostring(v)
        rawset(t, k, v)
    end,
})
logged.a = 1
logged.b = 2
print(writes[1])  -- "a=1"
print(writes[2])  -- "b=2"
print(logged.a)   -- 1 (rawset stored it)

-- __newindex as a table: redirects writes.
local backing = {}
local redirected = setmetatable({}, { __newindex = backing })
redirected.x = 42
print(rawget(redirected, "x"))  -- nil (write went to backing, not redirected itself)
print(backing.x)  -- 42

-- __call: a "callable" table.
local callable = setmetatable({}, {
    __call = function(_self, a, b) return a * 10 + b end,
})
print(callable(3, 4))  -- 34

-- Arithmetic metamethods on a record-like wrapper.
local Vec2 = {}
Vec2.__index = Vec2
function Vec2.__add(a, b) return setmetatable({ x = a.x + b.x, y = a.y + b.y }, Vec2) end
function Vec2.__tostring(v) return "(" .. v.x .. "," .. v.y .. ")" end
local function new(x, y) return setmetatable({ x = x, y = y }, Vec2) end
local sum = new(1, 2) + new(3, 4)
print(tostring(sum))  -- "(4,6)"

-- rawget / rawset bypass __index / __newindex.
local guarded = setmetatable({}, {
    __index = function() return "guarded" end,
    __newindex = function() error("blocked") end,
})
rawset(guarded, "stored", "ok")
print(rawget(guarded, "stored"))  -- "ok"
print(rawget(guarded, "missing"))  -- nil (raw read does NOT trigger __index)
print(guarded.missing)             -- "guarded" (real read DOES trigger __index)
```

- [ ] **Step 2: Run the harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: pass. If a specific subtest fails, trim THAT subtest from the file (leave a comment explaining), re-run, and commit.

- [ ] **Step 3: Commit**

```bash
git add runtime-tests/corpus/metatables.luau
git commit -m "test(corpus): metatables — index/newindex/call/arithmetic/raw"
```

---

## Task 3: bit32 corpus

**Files:**
- Create: `runtime-tests/corpus/bit32_ops.luau`

- [ ] **Step 1: Create `runtime-tests/corpus/bit32_ops.luau`**

```luau
print(bit32.band(0xFF, 0x0F))   -- 15
print(bit32.bor(0xF0, 0x0F))    -- 255
print(bit32.bxor(0xAA, 0xFF))   -- 85
print(bit32.bnot(0))            -- 4294967295
print(bit32.lshift(1, 4))       -- 16
print(bit32.rshift(256, 4))     -- 16
print(bit32.arshift(-16, 1))    -- 2147483640 (signed-shift on 32-bit u32 reinterpretation)
print(bit32.extract(0xF0F0, 4, 8))  -- 15 (extract 8 bits starting at bit 4)
print(bit32.replace(0xF0F0, 0x0F, 0, 4))  -- 0xF0FF

-- Combined: count parity bit pattern.
local function popcount(x)
    local n = 0
    while x ~= 0 do
        n = n + bit32.band(x, 1)
        x = bit32.rshift(x, 1)
    end
    return n
end
print(popcount(0x12345678))  -- bits set in 0x12345678
```

- [ ] **Step 2: Run the harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

If `bit32.arshift` fails (host-version-dependent representation of the shifted negative value), trim that line. Same for `extract`/`replace` if standalone Luau's host build doesn't ship them.

- [ ] **Step 3: Commit**

```bash
git add runtime-tests/corpus/bit32_ops.luau
git commit -m "test(corpus): bit32 module operations"
```

---

## Task 4: buffer corpus

**Files:**
- Create: `runtime-tests/corpus/buffer_ops.luau`

- [ ] **Step 1: Create `runtime-tests/corpus/buffer_ops.luau`**

```luau
local b = buffer.create(8)
print(buffer.len(b))  -- 8

buffer.writeu8(b, 0, 0xAB)
buffer.writeu8(b, 1, 0xCD)
print(buffer.readu8(b, 0))  -- 171
print(buffer.readu8(b, 1))  -- 205

-- Wider writes/reads.
buffer.writeu16(b, 2, 0xBEEF)
print(buffer.readu16(b, 2))  -- 48879
buffer.writeu32(b, 4, 0xDEADBEEF)
print(buffer.readu32(b, 4))  -- 3735928559

-- fromstring / tostring round-trip.
local s = "hello!"
local b2 = buffer.fromstring(s)
print(buffer.len(b2))      -- 6
print(buffer.tostring(b2)) -- "hello!"

-- buffer.copy: move bytes between buffers.
local src = buffer.fromstring("WXYZ")
local dst = buffer.create(8)
buffer.copy(dst, 2, src, 0, 4)
print(buffer.readu8(dst, 2))  -- W (0x57 = 87)
print(buffer.readu8(dst, 5))  -- Z (0x5A = 90)
```

- [ ] **Step 2: Run the harness**

If a host doesn't ship `buffer.copy` with the 5-arg form, simplify the copy test. If `buffer.writeu16`/`writeu32` aren't exposed by name, switch to whatever the host uses (e.g. `writei16`/`writei32`).

- [ ] **Step 3: Commit**

```bash
git add runtime-tests/corpus/buffer_ops.luau
git commit -m "test(corpus): buffer module read/write/copy"
```

---

## Task 5: vector corpus

**Files:**
- Create: `runtime-tests/corpus/vector_ops.luau`

- [ ] **Step 1: Determine which vector surface the host CLI exposes**

Run: `PATH="$HOME/.cargo/bin:$PATH" echo 'print(type(vector))' | luau -`

If it prints `function` or `table`, `vector.create(x, y, z)` should work. If it errors with "nil indexed", standalone Luau on this host doesn't ship a global `vector`. In that case, SKIP the rest of this task — write a small note in the status report and proceed to Task 6 directly. Do NOT commit an empty corpus file.

- [ ] **Step 2 (conditional on Step 1): Create `runtime-tests/corpus/vector_ops.luau`**

```luau
local v1 = vector.create(1, 2, 3)
local v2 = vector.create(4, 5, 6)

-- Component access.
print(v1.x)  -- 1
print(v1.y)  -- 2
print(v1.z)  -- 3

-- Component arithmetic.
local sum = v1 + v2
print(sum.x)  -- 5
print(sum.y)  -- 7
print(sum.z)  -- 9

local diff = v2 - v1
print(diff.x)  -- 3
print(diff.y)  -- 3
print(diff.z)  -- 3

-- Scalar multiplication if the host overloads it.
local scaled = v1 * 2
print(scaled.x)  -- 2
print(scaled.y)  -- 4
print(scaled.z)  -- 6

-- Identity round-trip through a function.
local function passthrough(v) return v end
local p = passthrough(v1)
print(p.x)  -- 1
print(p.z)  -- 3
```

If a specific case prints `nil` or errors on plain Luau, that operation isn't supported by this host build — trim it. The goal is "every line that runs plain also matches obfuscated."

- [ ] **Step 3: Run the harness**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

- [ ] **Step 4: Commit (only if file was created)**

```bash
git add runtime-tests/corpus/vector_ops.luau
git commit -m "test(corpus): vector type — component access and arithmetic"
```

---

## Task 6: Investigate any unexpected failures

This task is a placeholder. If Tasks 1–5 all pass cleanly, skip it entirely. If any specific feature failed, expand here with concrete sub-tasks describing:

1. The exact failing transcript (plain stdout vs obfuscated stdout).
2. Which IR layer is the leading suspect (HIR / MIR / LIR / emit / VM template).
3. The fix proposal and how to verify it.

The point of leaving this in the plan: clarify that "metatables Just Work" is a *hypothesis* the corpus is testing, not a foregone conclusion. If reality disagrees, this is where the divergence gets reconciled — not by squeezing a fix into the corpus tasks.

---

## Task 7: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 6 — metatables and Luau-native runtime types. The obfuscator
round-trips an expanded Luau subset (Plan 5 features + `setmetatable` /
`getmetatable` / `__index` / `__newindex` / `__call` / arithmetic metamethods,
plus the `bit32`, `buffer`, and `vector` standard modules) through a custom VM
with no obfuscation passes yet.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 6 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All corpus programs from Plans 1–6 pass differential testing.
- [ ] No source-code changes were made outside `runtime-tests/corpus/` and `README.md` (if anything else changed, that change has its own commit and a corresponding sub-task added to Task 6).
- [ ] The closures-as-metamethods invariant holds: any metamethod set to one of our wrapped closures dispatches through `vm_call` correctly.
- [ ] If a specific built-in (e.g., `bit32.arshift`, `buffer.copy`, `vector.create`) isn't exposed by the host's standalone Luau CLI, the corpus file's affected case was simplified or removed with a one-line comment explaining the omission.
