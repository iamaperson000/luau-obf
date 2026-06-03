# Luau Obfuscator — Plan 9: VM Template Hardening

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four highest-impact weaknesses identified by a deobfuscation pass against the Plan-8 output: (a) every VM identifier (`vm_call`, `regs`, `pc`, `OP_LoadConst`, etc.) is renamed to an opaque seed-derived name; (b) all comments are stripped; (c) banner/explanatory text inside the template is gone; (d) string-constant encryption gets a per-proto salt so identical plaintexts produce different ciphertexts across protos.

**Background:** A subagent was given the Plan-8 obfuscated output of a ~60-line bank-account demo with strict rules (no peeking at our source). It reconstructed the source nearly verbatim in under 20 minutes. Top-ranked weaknesses, in order:

1. `local OP_LoadConst = 13`, `local OP_LoadNil = 35`, … — the opcode names are in plaintext. Per-build opcode renumbering buys nothing when the names that decode the bytes are right there.
2. Helper functions and locals are meaningfully named (`vm_call`, `read_u16`, `regs`, `pc`, `op`, `a`, `b`, `c`, `delta`, `n_upvals`, `frame_varargs`, `kind`, `payload`).
3. Comments explain the encoding: `-- Proto metadata: { num_params, num_regs, num_upvals, is_vararg }`, `-- LocalReg: build a cell referencing current frame's regs`, etc.
4. Ciphertext repetition — Plan-8's encryption uses position-only mixing (`i % 32`), so `"deposit"` encrypts to identical bytes across protos. An attacker spots repeated ciphertexts and infers structure before even running the decryptor.

This plan closes all four. None of it touches HIR/MIR/LIR — the work is all in the render pipeline and the template.

**Architecture:**
- **Identifier mangling** is done as a *post-render* token-aware substitution pass. The template is written naturally with names like `vm_call` and `OP_LoadConst`; render produces the readable form; a final pass rewrites those names to seed-derived opaque ones. The substitution is token-aware (it skips inside string literals and comments) so that mention of e.g. `consts` inside a Luau string literal in the template doesn't get rewritten.
- **Comment stripping** is done as another post-render pass that removes `-- …` single-line comments line-by-line and `--[[ … ]]` block comments. The Jinja `{# … #}` comments never reach the output. Run BEFORE identifier mangling so the mangler doesn't have to handle comments.
- **Per-proto salt** is a one-line change in `encrypt_string` and `_decrypt`: mix the proto id into the per-byte mask. Rust side computes `b ^ key_a[i%32] ^ ((i+proto_id*7919) & 0xFF) ^ key_b[i%32]` (the prime `7919` is just a constant that varies the salt enough between consecutive proto ids); the VM side does the same. The `render` step passes `proto_id` to `format_const_pool` so it knows which proto it's encrypting.

**Tech Stack:** Same as Plans 1-8.

---

## Implementation notes

**Names to mangle.** Enumerate the locals declared in the template — these are the only names that can be safely renamed without risking aliasing of a Luau builtin. Compose the list once in Rust and bind it to seed-derived opaque names.

Concrete list (the renamer's ALLOW list — anything NOT in this list is left alone):

```
_ENV_           _select        _error          _pcall          _tostring
_type           _sbyte          _ssub           _schar          _tunpack
_tcreate        _floor          _tpack          _xor            _tconcat
_KA             _KB             _decrypt        _enc            _const
CONSTS          CODE            META
read_u16        read_i16        vm_call
proto_id        args            nargs           upvals
code            consts          meta            num_params      num_regs
is_vararg       regs            frame_varargs   vn              pc
code_len        op              a               b               c
delta           pid             n_upvals        new_upvals      kind
payload         captured        pa              n               fn
call_args       total_n         sp              m               sp_tbl
mode            r               results         tbl             n_values
count           main_args       main_n
OP_LoadNil      OP_LoadTrue     OP_LoadFalse    OP_LoadConst    OP_Move
OP_Add          OP_Sub          OP_Mul          OP_Div          OP_Mod
OP_Pow          OP_Concat       OP_Lt           OP_Le           OP_Eq
OP_Not          OP_Neg          OP_Len          OP_GetGlobal    OP_SetGlobal
OP_Call         OP_Return       OP_Jmp          OP_JmpIfTrue    OP_JmpIfFalse
OP_Closure      OP_NewTable     OP_GetTable     OP_SetTable     OP_GetUpval
OP_SetUpval     OP_CallVar      OP_BuildResults OP_Vararg       OP_ReturnMulti
sources         src             u               idx             out
i               j               len             cell            cells
loop            t               pos             new             value
```

(Some of these are inner-loop variables like `i` and `j`. Those are *also* mangled — the allow list is exhaustive over identifiers that appear as locals/params in the template.)

The allow list MUST NOT contain Luau global / standard-library names (`bit32`, `string`, `table`, `math`, `type`, `select`, `error`, `pcall`, `tostring`, `getfenv`, `_ENV`, `nil`, `true`, `false`, `function`, `local`, `end`, `if`, `then`, `else`, `elseif`, `while`, `do`, `for`, `in`, `return`, `break`, `continue`, `not`, `and`, `or`, `repeat`, `until`). The renamer is a *substitution by allow list*, not a "rename every identifier" — anything outside the list passes through.

**Generated opaque names.** Seed-derived. Use two-letter lowercase prefixed with `_`, so e.g. `_aa`, `_ab`, … `_zz` — 676 unique names. Plenty for ~80 identifiers. The mapping is `HashMap<String, String>`. Verify there are no collisions (two source names mapped to the same target).

**Tokenizer scope.** The render output is a Luau chunk. The tokenizer needs to:

- Skip `--…\n` single-line comments (already stripped pre-mangling, but defensive).
- Skip `--[[…]]` and `--[=[…]=]` block comments (also stripped pre-mangling).
- Skip double-quoted and single-quoted strings, with `\` escape handling.
- Skip long-bracket strings `[[…]]` and `[=[…]=]`.
- Skip numeric literals (`123`, `0xFF`, `0.5`, `1e3`).
- Identify Luau identifiers (`[A-Za-z_][A-Za-z0-9_]*`) and pass each through the substitution.

The template does NOT use backtick (interpolated) strings, so the tokenizer doesn't need to handle them. Verify with `grep '\`' crates/luau-runtime/assets/vm.luau.j2` — should be empty.

**Comment stripper.** A simpler tokenizer that finds `--` outside of strings and removes from `--` to end of line; for `--[[`, finds the matching `]]` (or `--[=[`/`]=]`). Run before the identifier mangler.

**Per-proto salt encoding.** Rust:

```rust
pub(crate) fn encrypt_string_for_proto(
    plaintext: &str,
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
) -> Vec<u8> {
    let bytes = plaintext.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}
```

The salt is `proto_id` itself — render iterates `program.functions.iter().enumerate()`, passing `i as u64` as the salt.

Luau side: `_decrypt` becomes `_decrypt(enc, proto_salt)`, called as `_decrypt(c._e, P)` where `P` is the current proto's salt. The `_const` helper takes proto_salt as an extra argument. Plumbing: each handler that reads constants knows its own `proto_id` (the function parameter at the top of `vm_call`).

**Don't break determinism.** Same seed → same opaque-name mapping. The RNG state must be consumed in a determined order. The renamer derives names from a *separate* sub-RNG produced from `rng.gen()`-derived bytes, so it's order-independent w.r.t. the encryption-key derivation.

---

## File Structure (changes only)

```
crates/luau-emit/src/
  render.rs            # MODIFY: derive name map, post-process to strip comments + mangle identifiers,
                       #         add proto_salt to encrypt_string call sites
  mangle.rs            # CREATE: token-aware comment stripper + identifier substitutor
crates/luau-runtime/assets/
  vm.luau.j2           # MODIFY: _decrypt takes a salt parameter; _const passes proto_id;
                       #         remove all banner/explanatory comments (they get stripped anyway,
                       #         but cleaning the template helps maintainers)
crates/luau-obf/src/
  lib.rs               # MODIFY: add tests asserting the readable strings are gone from output
README.md              # MODIFY: status line
```

---

## Task 1: Comment stripper

**Files:**
- Create: `crates/luau-emit/src/mangle.rs`
- Modify: `crates/luau-emit/src/lib.rs` (add `pub mod mangle;`)

The simpler half first — we can verify it works in isolation before layering identifier mangling on top.

- [ ] **Step 1: Create `crates/luau-emit/src/mangle.rs`**

```rust
//! Post-render passes that strip comments and mangle internal identifiers
//! in the emitted Luau chunk.

/// Strip all `-- …` single-line comments and `--[[ … ]]` (any depth)
/// block comments from a Luau source string. Skips comment markers that
/// appear inside string literals.
pub fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // String literal?
        if b == b'"' || b == b'\'' {
            let quote = b;
            out.push(b);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c);
                i += 1;
                if c == b'\\' && i < bytes.len() {
                    out.push(bytes[i]);
                    i += 1;
                } else if c == quote {
                    break;
                }
            }
            continue;
        }
        // Comment?
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            // Could be a block comment: --[[ ... ]] or --[=[ ... ]=]
            // Probe for `--[` then any number of `=`s then `[`.
            let mut j = i + 2;
            if j < bytes.len() && bytes[j] == b'[' {
                let mut eq = 0;
                let mut k = j + 1;
                while k < bytes.len() && bytes[k] == b'=' {
                    eq += 1;
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b'[' {
                    // Block comment. Find the matching `]<eq '='>]`.
                    let mut p = k + 1;
                    let needle: Vec<u8> = {
                        let mut v = vec![b']'];
                        for _ in 0..eq { v.push(b'='); }
                        v.push(b']');
                        v
                    };
                    while p < bytes.len() && !bytes[p..].starts_with(&needle) {
                        p += 1;
                    }
                    if p < bytes.len() {
                        i = p + needle.len();
                    } else {
                        i = bytes.len();
                    }
                    continue;
                }
            }
            // Single-line comment: consume to end of line.
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).expect("output is valid utf-8 since input was")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_single_line_comment() {
        let s = "local x = 1 -- this is a comment\nlocal y = 2";
        let out = strip_comments(s);
        assert!(!out.contains("comment"));
        assert!(out.contains("local x = 1"));
        assert!(out.contains("local y = 2"));
    }

    #[test]
    fn strips_block_comment() {
        let s = "before --[[ block\ncomment\n]] after";
        let out = strip_comments(s);
        assert!(!out.contains("block"));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn preserves_double_dash_in_string() {
        let s = "local s = \"this -- is not a comment\"";
        let out = strip_comments(s);
        assert!(out.contains("-- is not"));
    }

    #[test]
    fn preserves_double_dash_in_single_quote_string() {
        let s = "local s = 'a -- b'";
        let out = strip_comments(s);
        assert!(out.contains("-- b"));
    }

    #[test]
    fn handles_escape_in_string() {
        let s = "local s = \"\\\"-- still in string\"";
        let out = strip_comments(s);
        assert!(out.contains("-- still in string"));
    }
}
```

- [ ] **Step 2: Expose the module**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/lib.rs`, add at the top alongside the other `pub mod` declarations:

```rust
pub mod mangle;
```

- [ ] **Step 3: Verify**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-emit`

Expected: the 5 new tests pass. The pass is not yet called from `render`, so nothing else changes.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit/src/mangle.rs crates/luau-emit/src/lib.rs
git commit -m "feat(emit): comment-stripper for rendered Luau chunks"
```

---

## Task 2: Identifier mangler

**Files:**
- Modify: `crates/luau-emit/src/mangle.rs`

- [ ] **Step 1: Add the identifier substitution function**

Append to `mangle.rs`:

```rust
use std::collections::HashMap;

/// The static allow list of identifiers eligible for mangling.
/// This is exhaustive over locals/params declared in the VM template.
/// Anything not in this list passes through unchanged — including all
/// Luau globals (`bit32`, `string`, `table`, `math`, `type`, etc.) and
/// keywords. Keep alphabetically sorted within sections for maintainability.
pub const MANGLE_TARGETS: &[&str] = &[
    // Top-level aliases.
    "_ENV_", "_select", "_error", "_pcall", "_tostring", "_type",
    "_sbyte", "_ssub", "_schar", "_tunpack", "_tcreate", "_floor",
    "_tpack", "_xor", "_tconcat",
    // Encryption.
    "_KA", "_KB", "_decrypt", "_enc", "_const",
    // Tables of per-proto data.
    "CONSTS", "CODE", "META",
    // Helper functions.
    "read_u16", "read_i16", "vm_call",
    // vm_call params and frame-locals.
    "proto_id", "args", "nargs", "upvals",
    "code", "consts", "meta", "num_params", "num_regs", "is_vararg",
    "regs", "frame_varargs", "vn", "pc", "code_len",
    // Per-handler locals.
    "op", "a", "b", "c", "delta", "pid", "n_upvals", "new_upvals",
    "kind", "payload", "captured", "pa", "n", "fn", "call_args",
    "total_n", "sp", "m", "sp_tbl", "mode", "r", "results", "tbl",
    "n_values", "count",
    // Bootstrap.
    "main_args", "main_n",
    // Opcode constants.
    "OP_LoadNil", "OP_LoadTrue", "OP_LoadFalse", "OP_LoadConst", "OP_Move",
    "OP_Add", "OP_Sub", "OP_Mul", "OP_Div", "OP_Mod", "OP_Pow",
    "OP_Concat", "OP_Lt", "OP_Le", "OP_Eq", "OP_Not", "OP_Neg", "OP_Len",
    "OP_GetGlobal", "OP_SetGlobal", "OP_Call", "OP_Return",
    "OP_Jmp", "OP_JmpIfTrue", "OP_JmpIfFalse",
    "OP_Closure", "OP_NewTable", "OP_GetTable", "OP_SetTable",
    "OP_GetUpval", "OP_SetUpval",
    "OP_CallVar", "OP_BuildResults", "OP_Vararg", "OP_ReturnMulti",
    // Misc inner-loop names.
    "sources", "src", "u", "idx", "out", "i", "j", "len", "cell",
    "t", "pos", "new", "value",
];

/// Build a deterministic mangling map: each source name → an opaque `_xy`-style
/// name. Uses the rng to pick a permutation of two-letter suffixes.
pub fn build_name_map(rng: &mut rand_chacha::ChaCha20Rng) -> HashMap<String, String> {
    use rand::seq::SliceRandom;
    // Generate all 26*26 = 676 two-letter suffixes.
    let alphabet: Vec<char> = ('a'..='z').collect();
    let mut suffixes: Vec<String> = Vec::with_capacity(26 * 26);
    for a in &alphabet {
        for b in &alphabet {
            suffixes.push(format!("_{}{}", a, b));
        }
    }
    suffixes.shuffle(rng);
    let mut map: HashMap<String, String> = HashMap::new();
    for (i, name) in MANGLE_TARGETS.iter().enumerate() {
        if i >= suffixes.len() {
            // 80 names, 676 suffixes — this never trips.
            panic!("MANGLE_TARGETS exceeded suffix space");
        }
        map.insert((*name).to_string(), suffixes[i].clone());
    }
    map
}

/// Token-aware identifier substitution. Walks the source, identifies
/// identifier tokens, and replaces those in `map`. Skips string literals
/// and numeric literals. Does NOT handle comments — caller should
/// `strip_comments` first.
pub fn mangle_identifiers(src: &str, map: &HashMap<String, String>) -> String {
    let bytes = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // String literal — copy verbatim including escapes.
        if b == b'"' || b == b'\'' {
            let quote = b;
            out.push(b);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c);
                i += 1;
                if c == b'\\' && i < bytes.len() {
                    out.push(bytes[i]);
                    i += 1;
                } else if c == quote {
                    break;
                }
            }
            continue;
        }
        // Identifier start: [A-Za-z_]
        if b.is_ascii_alphabetic() || b == b'_' {
            // Read full identifier: [A-Za-z0-9_]*
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            // Ensure this identifier isn't a continuation of a number/identifier-prefixed
            // sequence — but Luau identifiers don't follow digits, so a starting `[A-Za-z_]`
            // is unambiguously the start.
            let name = std::str::from_utf8(&bytes[i..j]).unwrap();
            // Only mangle if the preceding char isn't `.` (member access — left alone)
            // OR if the identifier is one of ours (some of ours appear as table keys via `.`).
            // Actually: we ALWAYS replace if name is in the map. Member access like
            // `bit32.bxor` doesn't have `bxor` in the map, so it's safe.
            let prev_is_dot = i > 0 && bytes[i - 1] == b'.';
            let replacement = if !prev_is_dot {
                map.get(name).map(|s| s.as_str())
            } else {
                // Member access of a global like `bit32.bxor`. `bit32` is NOT in our map.
                // But `bxor` is also not. So this branch is mostly defensive.
                None
            };
            match replacement {
                Some(new_name) => out.extend_from_slice(new_name.as_bytes()),
                None => out.extend_from_slice(name.as_bytes()),
            }
            i = j;
            continue;
        }
        // Numeric literal — skip past it without checking identifiers inside.
        if b.is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.') {
                j += 1;
            }
            out.extend_from_slice(&bytes[i..j]);
            i = j;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).expect("output is valid utf-8")
}

#[cfg(test)]
mod mangle_tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    fn build_test_map() -> HashMap<String, String> {
        let mut rng = ChaCha20Rng::from_seed([7u8; 32]);
        build_name_map(&mut rng)
    }

    #[test]
    fn replaces_known_identifier() {
        let m = build_test_map();
        let out = mangle_identifiers("local vm_call = 1", &m);
        assert!(!out.contains("vm_call"), "found vm_call in {out}");
        let expected = m.get("vm_call").unwrap();
        assert!(out.contains(expected.as_str()));
    }

    #[test]
    fn leaves_luau_globals_alone() {
        let m = build_test_map();
        let out = mangle_identifiers("local x = bit32.bxor(a, b)", &m);
        assert!(out.contains("bit32.bxor"), "bit32.bxor was renamed in {out}");
    }

    #[test]
    fn does_not_rename_inside_string_literal() {
        let m = build_test_map();
        let out = mangle_identifiers("local s = \"vm_call\"", &m);
        assert!(out.contains("\"vm_call\""), "renamed inside string in {out}");
    }

    #[test]
    fn deterministic_for_same_rng_seed() {
        let m1 = build_test_map();
        let m2 = build_test_map();
        assert_eq!(m1.get("vm_call"), m2.get("vm_call"));
    }

    #[test]
    fn different_seeds_produce_different_mappings() {
        let mut r1 = ChaCha20Rng::from_seed([1u8; 32]);
        let mut r2 = ChaCha20Rng::from_seed([2u8; 32]);
        let m1 = build_name_map(&mut r1);
        let m2 = build_name_map(&mut r2);
        assert_ne!(m1.get("vm_call"), m2.get("vm_call"));
    }

    #[test]
    fn all_targets_get_a_unique_mapping() {
        let m = build_test_map();
        let unique: std::collections::HashSet<&String> = m.values().collect();
        assert_eq!(unique.len(), MANGLE_TARGETS.len(),
            "duplicate target names in mangle map");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-emit`

Expected: all 11 tests pass (5 strip-comments + 6 mangle tests). The mangler is still not invoked from render, so workspace tests stay green.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-emit/src/mangle.rs
git commit -m "feat(emit): identifier mangler with allow list of VM template names"
```

---

## Task 3: Wire mangle + strip into render

**Files:**
- Modify: `crates/luau-emit/src/render.rs`

- [ ] **Step 1: Apply both passes at the end of `render`**

In `/Volumes/External Drive/luau-obf/crates/luau-emit/src/render.rs`, modify the bottom of `render`:

```rust
    let output = tmpl.render(minijinja::context! {
        opcodes => opcodes,
        consts => consts,
        codes => codes,
        meta => meta,
        key_a => key_a_lit,
        key_b => key_b_lit,
    })
    .map_err(|e| EmitError::Template(e.to_string()))?;

    let stripped = crate::mangle::strip_comments(&output);
    let name_map = crate::mangle::build_name_map(rng);
    let mangled = crate::mangle::mangle_identifiers(&stripped, &name_map);
    Ok(mangled)
}
```

(Note: `tmpl.render` previously returned the result directly via `.map_err(...)`. Now we bind it to a `let output =`, post-process, and return. The `?` propagates the template error.)

- [ ] **Step 2: Run the full workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Now this is the moment of truth: identifier mangling changes EVERY local name in the rendered output. Every Plan 1–6 corpus program must still produce identical stdout under plain and obfuscated runs. If anything fails:

- The most likely cause is a missed identifier in `MANGLE_TARGETS` that the renamer left UN-mangled — that's harmless (it's still a valid Luau program). The more dangerous failure is the opposite: an identifier that's a Luau built-in accidentally got mangled because it appeared as a local elsewhere (e.g., `i`, `j`, `t`). Those ARE in our list; the renamer treats every occurrence as the local, which is fine IF there's no `_ENV.i` global the template depends on (it doesn't).
- Another possibility: the mangler's string-literal detection misses a long-bracket string. The template doesn't use long-bracket strings, but verify.

If a specific test fails, paste the divergence and report BLOCKED.

- [ ] **Step 3: Spot-check the output**

After tests pass, spot-check what an obfuscated program looks like:

```bash
PATH="$HOME/.cargo/bin:$PATH" cargo run -p luau-obf-cli --release -- runtime-tests/corpus/arithmetic.luau -o /tmp/spotcheck.luau --seed 0000000000000000000000000000000000000000000000000000000000000000 --quiet
grep -c 'OP_' /tmp/spotcheck.luau   # should be 0
grep -c 'vm_call' /tmp/spotcheck.luau  # should be 0
grep -c 'read_u16' /tmp/spotcheck.luau  # should be 0
head -20 /tmp/spotcheck.luau           # should show no readable names
```

If any of those greps returns > 0, the mangler missed something. Add the missing name to `MANGLE_TARGETS` and re-run.

- [ ] **Step 4: Commit**

```bash
git add crates/luau-emit/src/render.rs
git commit -m "feat(emit): post-render comment-strip + identifier mangle"
```

---

## Task 4: Per-proto salt for string encryption

**Files:**
- Modify: `crates/luau-emit/src/render.rs`
- Modify: `crates/luau-runtime/assets/vm.luau.j2`

- [ ] **Step 1: Update the Rust encryption to accept a salt**

In `render.rs`, change `encrypt_string` to take a `proto_salt: u64` parameter. Update its body:

```rust
pub(crate) fn encrypt_string(
    plaintext: &str,
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
) -> Vec<u8> {
    let bytes = plaintext.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    for (i, b) in bytes.iter().enumerate() {
        let ka = key_a[i % 32];
        let kb = key_b[i % 32];
        let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
        out.push(b ^ ka ^ pos ^ kb);
    }
    out
}
```

Update the existing `format_const` and `format_const_pool` to accept and forward the salt:

```rust
fn format_const_pool(
    consts: &[Constant],
    key_a: &[u8; 32],
    key_b: &[u8; 32],
    proto_salt: u64,
) -> String {
    let parts: Vec<String> = consts
        .iter()
        .map(|c| format_const(c, key_a, key_b, proto_salt))
        .collect();
    parts.join(", ")
}

fn format_const(c: &Constant, key_a: &[u8; 32], key_b: &[u8; 32], proto_salt: u64) -> String {
    match c {
        Constant::Nil => "nil".into(),
        Constant::Bool(true) => "true".into(),
        Constant::Bool(false) => "false".into(),
        Constant::Number(n) => {
            if n.is_nan() { "(0/0)".into() }
            else if n.is_infinite() && *n > 0.0 { "(1/0)".into() }
            else if n.is_infinite() { "(-1/0)".into() }
            else { format!("{}", n) }
        }
        Constant::String(s) => {
            let enc = encrypt_string(s, key_a, key_b, proto_salt);
            format!("_enc(\"{}\")", encode_luau_string_literal(&enc))
        }
    }
}
```

Update the `consts:` computation in `render` to pass the proto index:

```rust
    let consts: Vec<String> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| format_const_pool(&f.consts, &key_a, &key_b, i as u64))
        .collect();
```

- [ ] **Step 2: Update the Rust unit tests**

The existing `encrypt_decrypt_round_trips` test now needs a salt:

```rust
    #[test]
    fn encrypt_decrypt_round_trips() {
        let key_a = [42u8; 32];
        let key_b = [99u8; 32];
        let plaintext = "print";
        let proto_salt = 3u64;
        let enc = encrypt_string(plaintext, &key_a, &key_b, proto_salt);
        let dec: Vec<u8> = enc.iter().enumerate().map(|(i, b)| {
            let ka = key_a[i % 32];
            let kb = key_b[i % 32];
            let pos = ((i as u64).wrapping_add(proto_salt.wrapping_mul(7919)) & 0xFF) as u8;
            b ^ ka ^ pos ^ kb
        }).collect();
        assert_eq!(dec, plaintext.as_bytes());
    }

    #[test]
    fn encrypt_changes_bytes_for_typical_input() {
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        let enc = encrypt_string("hello world", &key_a, &key_b, 0);
        assert_ne!(enc, b"hello world");
    }

    #[test]
    fn empty_string_encrypts_to_empty() {
        let enc = encrypt_string("", &[0u8; 32], &[0u8; 32], 0);
        assert!(enc.is_empty());
    }

    #[test]
    fn different_proto_salts_change_ciphertext() {
        let key_a = [3u8; 32];
        let key_b = [4u8; 32];
        let e1 = encrypt_string("deposit", &key_a, &key_b, 1);
        let e2 = encrypt_string("deposit", &key_a, &key_b, 2);
        assert_ne!(e1, e2);
    }
```

- [ ] **Step 3: Update the VM template's `_decrypt` to use the salt**

In `/Volumes/External Drive/luau-obf/crates/luau-runtime/assets/vm.luau.j2`, replace the `_decrypt` and `_const` functions:

```luau
local function _decrypt(enc, salt)
    local out = {}
    local salt_term = (salt * 7919) % 4294967296
    for i = 1, #enc do
        local b = _sbyte(enc, i)
        local idx = ((i - 1) % 32) + 1
        local pos = ((i - 1) + salt_term) % 256
        out[i] = _schar(_xor(_xor(_xor(b, _KA[idx]), pos), _KB[idx]))
    end
    return _tconcat(out)
end

local function _enc(bytes) return { _e = bytes } end

local function _const(consts, idx, proto_id)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        c = _decrypt(c._e, proto_id)
        consts[idx] = c
    end
    return c
end
```

And update each `_const(consts, X)` call site to pass `proto_id`:

```luau
elseif op == OP_LoadConst then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    regs[a] = _const(consts, b + 1, proto_id)
elseif op == OP_GetGlobal then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    regs[a] = _ENV_[_const(consts, b + 1, proto_id)]
elseif op == OP_SetGlobal then
    local a = read_u16(code, pc); pc = pc + 2
    local b = read_u16(code, pc); pc = pc + 2
    _ENV_[_const(consts, a + 1, proto_id)] = regs[b]
```

`proto_id` is the first parameter to `vm_call` and is in scope throughout.

- [ ] **Step 4: Run the full workspace + corpus**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all 23 corpus programs still pass. If any fails, the most likely cause is a mismatch between Rust's `proto_salt.wrapping_mul(7919) & 0xFF` and Luau's `(salt * 7919) % 256` — verify they produce the same byte for `salt = 0..10`.

- [ ] **Step 5: Variance test for cross-proto ciphertext**

Append to the `tests` mod in `crates/luau-obf/src/lib.rs`:

```rust
    #[test]
    fn identical_plaintext_encrypts_differently_across_protos() {
        // The bank-style program defines "deposit" / "withdraw" as field names
        // that recur in multiple protos. After per-proto salt, the ciphertexts
        // should differ — we can't easily extract the ciphertexts from the
        // rendered Luau, but we can search the output for any duplicate
        // _enc("…") literal and assert no duplicates appear above some
        // threshold. Practical heuristic: count `_enc(` occurrences and
        // assert distinct argument byte sequences.
        let src = r#"
            local function make()
                return {
                    deposit = function() return "deposit" end,
                    withdraw = function() return "withdraw" end,
                }
            end
            local m = make()
            print(m.deposit())
            print(m.withdraw())
        "#;
        let r = obfuscate(src, Options { seed: Some([42u8; 32]) }).unwrap();
        // Extract all _enc("...") payloads.
        let mut payloads: Vec<&str> = Vec::new();
        let mut rest = r.output.as_str();
        while let Some(pos) = rest.find("_enc(\"") {
            let after = &rest[pos + 6..];
            // Find the closing quote, respecting backslash escapes.
            let mut j = 0;
            let bytes = after.as_bytes();
            while j < bytes.len() {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                } else if bytes[j] == b'"' {
                    break;
                } else {
                    j += 1;
                }
            }
            if j >= bytes.len() { break; }
            payloads.push(&after[..j]);
            rest = &after[j..];
        }
        // We expect "deposit" appears at least twice as a key in the source,
        // and similarly "withdraw". The total number of _enc literals depends
        // on how the compiler organizes constants. Just assert: no two
        // payloads with length >= 7 (deposit/withdraw size) are identical.
        let long: Vec<&&str> = payloads.iter().filter(|p| p.len() >= 7).collect();
        let dedup: std::collections::HashSet<&&str> = long.iter().copied().collect();
        assert_eq!(long.len(), dedup.len(),
            "found duplicate _enc payload of length >= 7 — per-proto salt is not working");
    }
```

- [ ] **Step 6: Run the new test**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p luau-obf --release`

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/luau-emit/src/render.rs crates/luau-runtime/assets/vm.luau.j2 crates/luau-obf/src/lib.rs
git commit -m "feat(obf): per-proto salt for string-constant encryption"
```

---

## Task 5: Strong-readability assertion tests

**Files:**
- Modify: `crates/luau-obf/src/lib.rs`

- [ ] **Step 1: Add post-Plan-9 readability assertions**

Append to the `tests` mod in `crates/luau-obf/src/lib.rs`:

```rust
    #[test]
    fn output_does_not_contain_op_constant_names() {
        let r = obfuscate("print(1 + 2)", Options { seed: Some([77u8; 32]) }).unwrap();
        // None of the OP_X names should survive mangling.
        for name in &["OP_LoadConst", "OP_LoadNil", "OP_Add", "OP_GetGlobal", "OP_Call", "OP_Return"] {
            assert!(!r.output.contains(name), "found {} in output", name);
        }
    }

    #[test]
    fn output_does_not_contain_vm_helper_names() {
        let r = obfuscate("print(1)", Options { seed: Some([88u8; 32]) }).unwrap();
        for name in &["vm_call", "read_u16", "read_i16", "_decrypt", "_KA", "_KB"] {
            assert!(!r.output.contains(name), "found {} in output", name);
        }
    }

    #[test]
    fn output_does_not_contain_handler_comments() {
        let r = obfuscate("print(1)", Options { seed: Some([99u8; 32]) }).unwrap();
        // Banner / annotation comments from the template MUST NOT survive.
        for snippet in &[
            "Proto metadata",
            "Bootstrap",
            "Rust-side",
            "luau-obf runtime",
            "do not edit",
            "Skip the closure_idx",
        ] {
            assert!(!r.output.contains(snippet),
                "found comment fragment {:?} in output", snippet);
        }
    }
```

- [ ] **Step 2: Run all tests**

Run: `PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace --release`

Expected: all tests pass. The three new readability tests are the post-Plan-9 acceptance gate.

- [ ] **Step 3: Commit**

```bash
git add crates/luau-obf/src/lib.rs
git commit -m "test: post-Plan-9 readability assertions"
```

---

## Task 6: README — status bump

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the status paragraph**

```
**Status:** Plan 9 — VM template hardening. Internal identifiers (`vm_call`,
`OP_LoadConst`, `read_u16`, register/upvalue locals, etc.) are renamed to
seed-derived opaque names; all comments are stripped from the rendered output;
string-constant encryption now mixes a per-proto salt so identical plaintexts
produce different ciphertexts across protos. A reader of the obfuscated chunk
no longer sees a labeled VM — they see ~400 lines of two-letter locals
operating on opaque tables and bytes. Full corpus passes under every seed.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: mark Plan 9 status"
```

---

## Verification checklist

- [ ] `cargo test --workspace --release` passes.
- [ ] All 23 corpus programs pass differential testing.
- [ ] `output_does_not_contain_op_constant_names` passes — `OP_LoadConst` etc. don't appear in output.
- [ ] `output_does_not_contain_vm_helper_names` passes — `vm_call`, `read_u16`, `_decrypt` etc. don't appear.
- [ ] `output_does_not_contain_handler_comments` passes — no banner/explanatory comments.
- [ ] `identical_plaintext_encrypts_differently_across_protos` passes — ciphertext repetition broken.
- [ ] Spot-check: `cargo run … -- bank.luau` produces an output where `grep -c 'vm_call\|OP_\|read_u16' output.luau` is `0`.
- [ ] Same-seed determinism preserved.
- [ ] Different-seed variance preserved.
