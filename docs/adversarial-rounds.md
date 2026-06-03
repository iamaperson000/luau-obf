# Adversarial Deobfuscation Rounds (post-Plan-25)

Five rounds were run against a non-trivial target program
(`deobf-cycle/target.luau` — sieve + factorial + GCD + Fibonacci + digit-sum,
~70 lines exercising tables, recursion, while/for loops, modulo, comparison,
`table.concat`, string concat) using the full default obfuscation pipeline
(Plans 1-25). Each round was a fresh subagent with **no access to the
obfuscator's source repository**, given only `/tmp/adversary_input.luau`
and shell tools / `luau`.

---

## Round 1 — full break in ~20 min

**Seed:** sha256("adv_round_1")
**Method:** Peel stage-0 XOR (positional, trivial to reverse) → reuse the
obfuscator's OWN decoder functions inside stage-1 to dump disassembly of
all 6 protos → name algorithms by structural recognition.
**Outcome:** Full source-level pseudocode recovered.

**Top weaknesses identified:**
1. VM dispatcher + decoder helpers ship as plaintext Luau source after
   stage-0.
2. Opcode permutation lives at a fixed header offset with fixed canonical
   1-35 numbering — once the byte-decoder is known, the permutation is a
   Caesar cipher.
3. Stage-0 XOR mask is positional only (`key[i%32] XOR (i%256)`); no
   keyed feedback. Statically decryptable offline.
4. Opaque arithmetic from Plans 20/21/25 has only constant operands;
   constant folding strips it.

**Fix applied:** none for Round 1→2; just used a different seed.

---

## Round 2 — broken in ~3 min

**Seed:** sha256("adv_round_2")
**Method:** Round-1 pipeline applied **straight-through**. The analyst
literally `loadstring`'d the obfuscator's own VM and called the entrypoint,
recovering outputs without disassembling.

**Finding:** Per-build randomization (Plans 7/11/12/13/14) varies XOR keys,
identifier names, and numeric salts — but the VM topology and dispatcher
shape are template-fixed and CALLABLE by anyone running the script. Opcode
numbering is dense 1-35 in both rounds.

**Fix applied:** none for Round 2→3 (deferred to in-template work).

---

## Round 3 — synthesis: build-invariant fingerprint

**Seed:** sha256("adv_round_3")
**Focus shifted to "diff attack" angle.**

**Key finding:** A 16-integer structural signature
`(proto_count, per_proto_const_counts, per_proto_bytecode_lens, opcode_handler_set, ...)`
is identical across same-source builds and serves as a build-invariant
program fingerprint. A SHA-256 of that tuple is sufficient to recognize
the same program regardless of seed.

**Killing detail:** The literal error string `"luau-obf: unknown opcode "`
inside the VM template was a free brand-name fingerprint.

**Fix applied between Rounds 3 and 4:** removed the brand-name from
`vm.luau.j2` line 358 (`_error(_tostring(op))` only) and line 1 header
comment. Committed as `d0472db`.

---

## Round 4 — known-plaintext attack on stage-0 XOR

**Seed:** sha256("adv_round_4")
**New finding:** stage-0 XOR is vulnerable to per-slot known-plaintext
recovery. Because stage-1 contains many escaped string literals, the
plaintext has very high backslash (`\\` = 0x5C) density (~16%). Bucketing
ciphertext bytes by `i mod 32`, the modal decrypted byte per slot is
0x5C. XOR'ing the modal-byte against 0x5C and the appropriate
`(i mod 256)` constant recovers the 32-byte key **without reading the
full ciphertext or executing the chunk**. ~30 lines of pure Python.

**Brand-fix verification:** confirmed effective for outer wrapper; analyst's
"still leaks in stage-1" claim was tmp-file cross-contamination from prior
rounds (verified by direct decryption — current output is brand-free).

**Fix applied between Rounds 4 and 5:** none (a real stream cipher is the
proper fix; out of scope for this session).

---

## Round 5 — threat model synthesis

**Verdict (effective strength 0-100):**
- Casual / script-kiddie attacker: **~72** (generic deobf tools won't peel
  the custom VM).
- Determined attacker (~1 day of focused RE): **~18** (every layer fell
  within hours across Rounds 1-5).
- Sophisticated multi-day attacker: **~5** (no fundamental barrier
  remains — only nuisance work).

**Top three fixes ranked impact-to-effort:**
1. **Bind stage-0 key to runtime environment** (script-hash, `game.PlaceId`,
   or remote attestation). Trivial patch to the bootstrap loader. *Kills
   VM-reuse entirely* because the inner blob no longer decrypts off-host.
2. **Split the dispatcher across multiple closures with opaque upvalue
   indirection** — currently it's one big `while/if` ladder with stable
   opcode field offsets. Forces adversaries to rebuild a custom interpreter
   instead of calling the existing one.
3. **Per-proto distinct XOR keys** instead of one stream shared across the
   constant blob. Tiny codegen change; defeats the per-slot known-plaintext
   pivot.

**Hardest-to-fix, highest-leverage weakness:**
The VM is a plain Lua-level interpreter that runs in the attacker's
process. Fixing this requires either moving execution off-host (server-side
eval, native plugin) or genuine control-flow virtualization with handler
fusion + inter-op state encryption. That's a rewrite, not a patch — but it
would force adversaries from "host the VM, dump state" to symbolic
execution or live tracing, an order-of-magnitude harder.

---

## Summary of patches landed in this session

| Round | Patch | Commit |
|-------|-------|--------|
| 3→4   | Strip "luau-obf"/`unknown opcode` brand-name string from VM template | `d0472db` |

## Patches NOT landed (recommended follow-up plans)

1. Stage-0 key bound to runtime environment (Roblox PlaceId, `_ENV` hash, etc.).
2. Per-proto distinct XOR keys for constant pool.
3. Dispatcher splitting / closure indirection (replaces the `while/if` ladder).
4. Stream-cipher replacement for stage-0 XOR (defeats per-slot known-plaintext).
5. Handler fusion / superoperators (so opcode permutation actually matters).
6. Junk-arithmetic operand replacement (live VLocals instead of fresh LoadConsts;
   addresses constant-folder vulnerability).

## Known pre-existing bug surfaced

The adversarial target initially used `a, b = b, a % b` swap-style
multi-assign, which the obfuscator miscompiles even with NO obfuscation
passes (just identity transform). See `docs/known-bugs.md`. The target
was rewritten to use explicit temporaries to proceed with the rounds.
