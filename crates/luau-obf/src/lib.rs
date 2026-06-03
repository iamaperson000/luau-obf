//! Top-level facade. One function: obfuscate.

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 32-byte seed. If None, a random seed is generated and surfaced via `seed_used`.
    pub seed: Option<[u8; 32]>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("parse: {0}")]
    Parse(#[from] luau_parse::ParseError),
    #[error("hir: {0}")]
    Hir(#[from] luau_hir::HirError),
    #[error("mir: {0}")]
    Mir(#[from] luau_mir::MirError),
    #[error("lir: {0}")]
    Lir(#[from] luau_lir::LirError),
    #[error("emit: {0}")]
    Emit(#[from] luau_emit::EmitError),
}

pub struct ObfuscateResult {
    /// The obfuscated Luau chunk.
    pub output: String,
    /// The seed actually used (whether user-provided or generated).
    pub seed_used: [u8; 32],
}

pub fn obfuscate(source: &str, opts: Options) -> Result<ObfuscateResult, Error> {
    let seed = opts.seed.unwrap_or_else(random_seed);
    let mut rng = ChaCha20Rng::from_seed(seed);

    let ast = luau_parse::parse(source)?;
    let hir = luau_hir::lower::lower(&ast)?;
    let mut mir = luau_mir::lower::lower(&hir)?;
    let plan = luau_passes::default_plan();
    plan.run(&mut mir, &mut rng);
    let mut lir = luau_lir::lower::lower(&mir)?;
    luau_lir::shuffle::shuffle_constants(&mut lir, &mut rng);
    let output = luau_emit::emit(&lir, &mut rng)?;
    Ok(ObfuscateResult { output, seed_used: seed })
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a Luau string literal body containing `\NNN` decimal escapes,
    /// `\\`, `\"`, `\n`, `\r`, `\t`, and printable ASCII into a byte vector.
    fn decode_luau_string_literal_body(body: &str) -> Vec<u8> {
        let bytes = body.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\\' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                if next.is_ascii_digit() {
                    // \N, \NN, or \NNN — Luau accepts 1-3 digits.
                    let mut n = 0u32;
                    let mut k = i + 1;
                    let end = (i + 4).min(bytes.len());
                    while k < end && bytes[k].is_ascii_digit() {
                        n = n * 10 + (bytes[k] - b'0') as u32;
                        k += 1;
                    }
                    out.push(n as u8);
                    i = k;
                } else {
                    match next {
                        b'\\' => out.push(b'\\'),
                        b'"' => out.push(b'"'),
                        b'\'' => out.push(b'\''),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        other => out.push(other),
                    }
                    i += 2;
                }
            } else {
                out.push(b);
                i += 1;
            }
        }
        out
    }

    /// Extract the stage-1 source from a stage-0-wrapped output. Parses the
    /// first two `local <name> = "<...>"` lines as payload and key, then
    /// reverses the XOR encryption to recover the stage-1 Luau source.
    fn decrypt_stage1_from_output(output: &str) -> String {
        // Find first quoted string literal — that's the payload.
        let (payload_body, after_payload) = extract_first_quoted(output)
            .expect("first quoted literal (payload) missing from stage-0 output");
        // Find next quoted literal — that's the key.
        let (key_body, _) = extract_first_quoted(after_payload)
            .expect("second quoted literal (key) missing from stage-0 output");
        let payload = decode_luau_string_literal_body(payload_body);
        let key = decode_luau_string_literal_body(key_body);
        assert_eq!(key.len(), 32, "stage-0 key isn't 32 bytes");
        let plain: Vec<u8> = payload.iter().enumerate().map(|(i, b)| {
            let k = key[i % 32];
            let pos = (i & 0xFF) as u8;
            b ^ k ^ pos
        }).collect();
        String::from_utf8(plain).expect("stage-1 source is UTF-8")
    }

    /// Find the first `"..."` string literal in the input, returning the
    /// body (without quotes) and the slice that follows the closing quote.
    fn extract_first_quoted(s: &str) -> Option<(&str, &str)> {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i] != b'"' { i += 1; }
        if i >= bytes.len() { return None; }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() {
            if bytes[j] == b'\\' && j + 1 < bytes.len() {
                j += 2;
                continue;
            }
            if bytes[j] == b'"' { break; }
            j += 1;
        }
        if j >= bytes.len() { return None; }
        Some((&s[start..j], &s[j+1..]))
    }

    #[test]
    fn obfuscate_empty() {
        let r = obfuscate("", Options::default()).unwrap();
        // After Plan 9 mangling, internal names are opaque. Just confirm
        // the output is a non-trivial Luau program.
        assert!(r.output.contains("return"));
        assert!(r.output.len() > 200);
        assert_eq!(r.seed_used.len(), 32);
    }

    #[test]
    fn obfuscate_simple_print() {
        let r = obfuscate("print(1)", Options { seed: Some([1u8; 32]) }).unwrap();
        assert_eq!(r.seed_used, [1u8; 32]);
        // After Plan 9 mangling, opcode names are opaque. Sanity-check
        // that "print" doesn't leak as a plaintext string literal.
        assert!(!r.output.contains("\"print\""));
        assert!(r.output.len() > 200);
    }

    #[test]
    fn deterministic_with_fixed_seed() {
        let a = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        let b = obfuscate("local x = 1 print(x)", Options { seed: Some([7u8; 32]) }).unwrap();
        assert_eq!(a.output, b.output);
    }

    #[test]
    fn different_seeds_produce_different_outputs() {
        let src = "local x = 1 + 2 print(x)";
        let a = obfuscate(src, Options { seed: Some([1u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([2u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }

    #[test]
    fn three_seeds_produce_three_distinct_outputs() {
        let src = "local function add(a, b) return a + b end print(add(3, 4))";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn output_does_not_contain_plaintext_global_name() {
        // Cardinal test: "print" must NOT appear in the obfuscated output.
        // Compile a program that uses print; the output should NOT contain
        // the literal string "print" inside a Luau string literal.
        let r = obfuscate("print(\"hello\")", Options { seed: Some([55u8; 32]) }).unwrap();
        // We allow the substring "print" to appear in things like comments or
        // template scaffolding; what we forbid is a Luau string literal
        // containing the word print. Check that no `"print"` substring exists.
        assert!(!r.output.contains("\"print\""), "found plaintext \"print\" in output");
        assert!(!r.output.contains("'print'"), "found plaintext 'print' in output");
        // Also the literal "hello" should be encrypted.
        assert!(!r.output.contains("\"hello\""), "found plaintext \"hello\" in output");
    }

    #[test]
    fn different_seeds_change_encrypted_byte_shape() {
        // Same source, two different seeds → the encrypted byte sequences
        // for "print" are different (because the keys are different).
        let a = obfuscate("print(1)", Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate("print(1)", Options { seed: Some([20u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
    }

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
        // Extract all _enc("...") payloads. After Plan 9 mangling, the
        // `_enc` name itself is rewritten — but the call-site syntax
        // `_xx("...")` is preserved. Search by the constant-pool form
        // which is `<name>("...")` for each encrypted constant.
        // Simpler: extract any quoted string literal that contains an
        // escape sequence (which encrypted bytes almost certainly do).
        let mut payloads: Vec<&str> = Vec::new();
        let mut rest = r.output.as_str();
        // We look for `("` literal pattern that's used in the encrypted
        // constant constructor (a name followed by `("…")`).
        while let Some(pos) = rest.find("(\"") {
            let after = &rest[pos + 2..];
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

    #[test]
    fn output_has_no_opcode_constants_block() {
        // Plan 10: the opcode table is inlined, so the output should NOT contain
        // 35 consecutive `local _xx = N` declarations of small integers.
        let r = obfuscate("print(1 + 2)", Options { seed: Some([100u8; 32]) }).unwrap();
        // Look for the canonical signature: many short `local NAME = SMALLNUM\n` lines.
        // Heuristic: count lines matching `^local _\w+ = \d+\s*$` (just an integer literal,
        // no operator). After Plan 10 there should be at most a handful (k0, k1, _KA-element
        // declarations could match) — well below 35.
        let lines_matching: usize = r.output.lines().filter(|l| {
            let trimmed = l.trim();
            // local _xx = NNN  (no operators on the RHS)
            if !trimmed.starts_with("local ") { return false; }
            let rest = trimmed.trim_start_matches("local ");
            let mut parts = rest.splitn(2, '=');
            let (lhs, rhs) = match (parts.next(), parts.next()) {
                (Some(l), Some(r)) => (l.trim(), r.trim()),
                _ => return false,
            };
            if !lhs.starts_with('_') { return false; }
            // RHS is a bare nonnegative integer.
            rhs.chars().all(|c| c.is_ascii_digit())
        }).count();
        assert!(lines_matching < 10,
            "expected fewer than 10 `local _x = <int>` lines, found {}", lines_matching);
    }

    #[test]
    fn output_has_no_top_level_meta_table() {
        let r = obfuscate("print(1)", Options { seed: Some([170u8; 32]) }).unwrap();
        // META is a renamed identifier in Plan-9's MANGLE_TARGETS, so the literal
        // "META" should never appear, AND no `local _xx = {` pattern that looks
        // like a 4-tuple-per-row table should appear at the top.
        assert!(!r.output.contains("META"));
        // Look for the canonical META shape: `{ 0, N, 0, 0 },` lines. After Plan
        // 11, this pattern should be absent (one occurrence is `_KA = { … }`
        // and another is `_KB = { … }` — those are byte arrays, single row each).
        let four_tuple_lines: usize = r.output.lines().filter(|l| {
            // Match e.g. "    { 0, 5, 0, 0 },"
            let t = l.trim();
            t.starts_with("{ ") && t.ends_with("},")
                && t.matches(',').count() == 3
        }).count();
        assert!(four_tuple_lines == 0,
            "found {} suspected META rows in output", four_tuple_lines);
    }

    #[test]
    fn output_has_no_bare_numeric_constants_in_const_pool() {
        // Plan 11: every constant of every type is wrapped in _cw(tag, bytes).
        // The constant pool tables should contain ONLY _cw(...) entries (or
        // potentially nothing, if a proto has no constants).
        let r = obfuscate(
            "local function f(a) return a + 100 end print(f(25))",
            Options { seed: Some([180u8; 32]) }
        ).unwrap();
        // The numbers 100 and 25 from the source should NOT appear as bare
        // integers in CONSTS rows (they're encrypted as _cw(1, "...")).
        //
        // We restrict the search to lines that look like a const-pool row:
        // an opening `{` followed by content, then `},`. The keystream byte
        // arrays `_KA` / `_KB` are 32-element numeric tables — those would
        // happen to contain bytes equal to any small integer purely by chance.
        // We filter them out by skipping `local NAME = { ... }` lines.
        let suspect_lines = |needle: &str| -> usize {
            r.output.lines().filter(|l| {
                let t = l.trim();
                if t.starts_with("local ") { return false; }
                // Plain const-pool row: starts with `{ ` and ends with `},`.
                if !(t.starts_with("{ ") && t.ends_with("},")) { return false; }
                l.contains(needle)
            }).count()
        };
        let patterns_100 = [", 100,", "{ 100,", ", 100 "];
        let patterns_25 = [", 25,", "{ 25,", ", 25 "];
        let total_100: usize = patterns_100.iter().map(|p| suspect_lines(p)).sum();
        assert_eq!(total_100, 0,
            "found bare `100` in a const-pool row, expected encrypted as _cw");
        let total_25: usize = patterns_25.iter().map(|p| suspect_lines(p)).sum();
        assert_eq!(total_25, 0,
            "found bare `25` in a const-pool row, expected encrypted as _cw");
    }

    #[test]
    fn output_has_no_bare_boolean_in_const_pool() {
        // A program with `return true` / `return false` puts those into the
        // constant pool. After Plan 11, they're _cw(2, ...) and never appear
        // as the bare words `true,` `false,` in a constants table.
        let r = obfuscate(
            "local function f() return true end print(f())",
            Options { seed: Some([190u8; 32]) }
        ).unwrap();
        // Heuristic: count occurrences of `true,` or `, true,` or `{ true,`
        // — these would be const-pool entries. After Plan 11, expected 0.
        let bare_true: usize = r.output.matches(", true,").count()
            + r.output.matches("{ true,").count();
        assert_eq!(bare_true, 0, "found bare `true,` in output");
    }

    #[test]
    fn cw_calls_have_exactly_one_argument() {
        // Plan 12: _cw(...) takes only the encrypted blob, no type tag.
        // After mangling, _cw is some `_xx` — but its call shape
        // `_xx("<bytes>")` should still be visible. Look for any
        // `_xx(N, "...")` form where N is a small integer (the old tag) —
        // there should be none.
        let r = obfuscate(
            "local function f(a) return a + 100 end print(f(25)) print(true)",
            Options { seed: Some([222u8; 32]) }
        ).unwrap();
        // Hand-rolled scanner: find every `_aa(` pattern (underscore + 2
        // lowercase letters + open paren) at the start of an identifier and
        // count occurrences where the next non-whitespace char is a digit
        // followed by `, "` — the old _cw(tag, "bytes") shape.
        let bytes = r.output.as_bytes();
        let mut i = 0;
        let mut bad_call_count = 0;
        while i + 6 < bytes.len() {
            if bytes[i] == b'_'
                && bytes[i + 1].is_ascii_lowercase()
                && bytes[i + 2].is_ascii_lowercase()
                && bytes[i + 3] == b'('
            {
                if i == 0
                    || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
                {
                    let next = bytes[i + 4];
                    if next.is_ascii_digit()
                        && i + 7 < bytes.len()
                        && bytes[i + 5] == b','
                        && bytes[i + 6] == b' '
                        && bytes[i + 7] == b'"'
                    {
                        bad_call_count += 1;
                    }
                }
                i += 4;
                continue;
            }
            i += 1;
        }
        assert_eq!(bad_call_count, 0,
            "{} suspicious `_xx(N, \"…\")` wrappers found — _cw should now be 1-arg",
            bad_call_count);
    }

    #[test]
    fn output_blobs_are_uniform_length() {
        // Find all string literals that appear inside `_xx("…")` single-arg
        // calls (the wrapper shape after Plan 12). Their decoded byte
        // length should all be exactly BLOB_SIZE (64 bytes in this build).
        let r = obfuscate(
            "local function f(a) return a + 100 end \
             print(f(25)) print(true) print(\"hi\")",
            Options { seed: Some([233u8; 32]) }
        ).unwrap();
        // Plan 14: the stage-1 source is encrypted inside the wrapper.
        // Decrypt it to inspect the constant-blob structure.
        let stage1 = decrypt_stage1_from_output(&r.output);
        // Crude extraction: find every `_xx("…")` and decode the `\NNN`
        // escapes to count the actual byte length.
        let mut blob_lengths: Vec<usize> = Vec::new();
        let bytes = stage1.as_bytes();
        let mut i = 0;
        while i + 6 < bytes.len() {
            if bytes[i] == b'_'
                && bytes[i + 1].is_ascii_lowercase()
                && bytes[i + 2].is_ascii_lowercase()
                && bytes[i + 3] == b'('
                && bytes[i + 4] == b'"'
            {
                if i == 0
                    || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
                {
                    let mut j = i + 5;
                    let mut byte_count = 0;
                    while j < bytes.len() && bytes[j] != b'"' {
                        if bytes[j] == b'\\' && j + 1 < bytes.len() {
                            if bytes[j + 1].is_ascii_digit() {
                                // `\NNN` form — skip 4 chars total
                                // (`\` + 3 digits).
                                j += 4;
                                byte_count += 1;
                            } else {
                                // `\\`, `\"`, `\n`, etc — escape + 1 char.
                                j += 2;
                                byte_count += 1;
                            }
                        } else {
                            j += 1;
                            byte_count += 1;
                        }
                    }
                    // Confirm the call closes with `")` to filter false
                    // positives such as a literal inside another expression.
                    if j + 1 < bytes.len() && bytes[j] == b'"' && bytes[j + 1] == b')' {
                        blob_lengths.push(byte_count);
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        // Filter to plausible blob sizes — bytecode CODE entries can also
        // appear as single-arg string literals (e.g. via `_tconcat(..)`
        // calls in the runtime, though those usually have multi-arg
        // signatures). The CODE entries themselves live in a top-level
        // table literal `{ "…", "…" }` (not a call), so they should not
        // match our `_xx("…")` pattern at all. To be defensive, we filter
        // to lengths ≤ 128 — anything larger is almost certainly bytecode.
        let blobs: Vec<usize> = blob_lengths.iter().copied().filter(|&n| n <= 128).collect();
        assert!(!blobs.is_empty(), "no constant blobs found in output");
        // Every constant blob should be exactly BLOB_SIZE bytes (32 or 64).
        for n in &blobs {
            assert!(*n == 32 || *n == 64,
                "blob length {} is not a uniform constant-blob size", n);
        }
        // Stronger: all blobs in a single build should have the same length.
        let first = blobs[0];
        for n in &blobs {
            assert_eq!(*n, first,
                "constant blobs are not uniformly sized: found {} and {}",
                first, n);
        }
    }

    #[test]
    fn bytecode_bytes_are_not_small_opcode_set() {
        // Plan 10: bytecode bytes are XOR-encoded, so the BYTE DISTRIBUTION inside
        // any CODE entry should NOT be tightly clustered in 1..=35.
        let r = obfuscate(
            "local function f(a, b) return a + b end print(f(3, 4))",
            Options { seed: Some([200u8; 32]) }
        ).unwrap();
        // Find the first CODE entry — look for a long `"\NNN…"` string literal.
        // Extract the first string literal that has more than 10 `\NNN` escapes.
        let mut bytes_found: Vec<u8> = Vec::new();
        for line in r.output.lines() {
            if let Some(q1) = line.find('"') {
                if let Some(q2_rel) = line[q1+1..].rfind('"') {
                    let payload = &line[q1+1..q1+1+q2_rel];
                    // Parse the \NNN escapes.
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut chars = payload.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '\\' {
                            let mut digits = String::new();
                            for _ in 0..3 {
                                if let Some(&dc) = chars.peek() {
                                    if dc.is_ascii_digit() { digits.push(dc); chars.next(); }
                                    else { break; }
                                }
                            }
                            if let Ok(n) = digits.parse::<u32>() {
                                if n <= 255 { bytes.push(n as u8); }
                            }
                        } else if c.is_ascii() {
                            bytes.push(c as u8);
                        }
                    }
                    if bytes.len() > 30 {
                        bytes_found = bytes;
                        break;
                    }
                }
            }
        }
        assert!(!bytes_found.is_empty(), "could not find a bytecode literal");
        // If the bytes were plaintext opcodes, every byte would be in 1..=35
        // (with operands as u16 LE pairs interleaved). After XOR encoding, the
        // bytes are pseudo-random and should hit values outside 0..50 frequently.
        let above_50: usize = bytes_found.iter().filter(|&&b| b > 50).count();
        let ratio = above_50 as f64 / bytes_found.len() as f64;
        assert!(ratio > 0.3,
            "fewer than 30% of bytecode bytes are above 50 ({} of {}); \
             suggests XOR encoding isn't applied", above_50, bytes_found.len());
    }

    #[test]
    fn per_proto_opcode_bytes_differ() {
        // A program with two functions that both perform an arithmetic operation.
        // Their bytecode bytes for that operation should differ because the
        // per-proto opcode permutations differ.
        // We can't easily inspect "which byte is ADD in proto N" without decoding
        // the XOR'd bytecode, but we can use a structural property:
        //   - Each proto's CODE entry is a string literal of \NNN escapes.
        //   - The first 40 bytes are the encrypted prologue.
        //   - Bytes 6..40 are the encrypted inv table for that proto.
        //   - Two protos with different inv tables -> bytes 6..40 differ.
        let r = obfuscate(
            "local function a(x, y) return x + y end \
             local function b(x, y) return x + y end \
             print(a(1, 2), b(3, 4))",
            Options { seed: Some([244u8; 32]) }
        ).unwrap();
        // Plan 14: bytecode entries live inside the encrypted stage-1 payload.
        // Recover the stage-1 source to scan for CODE entries.
        let stage1 = decrypt_stage1_from_output(&r.output);
        // Extract the first two CODE entries.
        // A CODE entry has the form: `"\NNN\NNN..."` inside the CODE table.
        // Find all such literals and grab the first two non-empty ones.
        let mut codes: Vec<Vec<u8>> = Vec::new();
        let bytes = stage1.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // Look for an opening `"` that's immediately preceded by a comma or
            // bracket (start of a table entry).
            if bytes[i] == b'"' {
                // Heuristic: the CODE entries are big and start near the top.
                // Decode \NNN escapes until closing `"`.
                let mut j = i + 1;
                let mut decoded: Vec<u8> = Vec::new();
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        if bytes[j + 1].is_ascii_digit() {
                            // \NNN form.
                            let mut n = 0u32;
                            let mut k = j + 1;
                            while k < bytes.len() && bytes[k].is_ascii_digit() && k < j + 4 {
                                n = n * 10 + (bytes[k] - b'0') as u32;
                                k += 1;
                            }
                            if n <= 255 {
                                decoded.push(n as u8);
                            }
                            j = k;
                        } else {
                            j += 2;
                        }
                    } else {
                        decoded.push(bytes[j]);
                        j += 1;
                    }
                }
                // Collect entries of length >= 40 (one full prologue) and
                // exclude the constant blob size (exactly 64 bytes). Bytecode
                // CODE entries are always >= 40 bytes (prologue) plus at least
                // one instruction, but small functions can land below 64.
                if decoded.len() >= 40 && decoded.len() != 64 {
                    codes.push(decoded);
                }
                i = j + 1;
                continue;
            }
            i += 1;
        }
        assert!(codes.len() >= 2, "expected at least 2 bytecode CODE entries, got {}", codes.len());
        // Compare the first two: bytes 5..40 (the inv-table region) must differ.
        // We can't decrypt; just assert the encrypted bytes 5..40 are not identical.
        let a_inv_region = &codes[0][5..40.min(codes[0].len())];
        let b_inv_region = &codes[1][5..40.min(codes[1].len())];
        assert_ne!(a_inv_region, b_inv_region,
            "two protos have identical encrypted inv tables - permutation isn't varying");
    }

    #[test]
    fn stage0_wrapper_present() {
        let r = obfuscate("print(1)", Options { seed: Some([121u8; 32]) }).unwrap();
        // The wrapper invokes `loadstring(...)`. Even after mangling, the
        // `loadstring` global stays unmangled (it's a Luau builtin).
        assert!(r.output.contains("loadstring"));
    }

    #[test]
    fn dispatcher_keywords_not_in_raw_output() {
        // The stage-1 source contains the dispatcher pattern `elseif op == N then`.
        // After encryption, no such pattern should appear in the raw output.
        let r = obfuscate("print(1 + 2)", Options { seed: Some([122u8; 32]) }).unwrap();
        // After Plan-14 wrapping, the stage-1 source is encrypted. The string
        // "elseif" should appear at most once (inside our stage-0 wrapper — but
        // actually the wrapper has no `elseif`; only `if/then`). So count == 0.
        let elseif_count = r.output.matches("elseif").count();
        assert!(elseif_count <= 1,
            "expected at most 1 'elseif' (in unlikely stage-0 use), found {}", elseif_count);
    }

    #[test]
    fn opcode_dispatch_pattern_not_in_raw_output() {
        // Pre-Plan-14, the output had `elseif _xy == 14 then` and similar
        // arms. Post-Plan-14, no such patterns should be visible in raw text.
        let r = obfuscate("local a = 1 + 2 print(a)",
                          Options { seed: Some([123u8; 32]) }).unwrap();
        // Search for any `== <integer>` pattern that looks like a dispatcher
        // arm. After stage-0 wrapping, none should appear in raw text.
        let bytes = r.output.as_bytes();
        let mut arm_pattern_count = 0;
        let mut i = 0;
        while i + 8 < bytes.len() {
            // Look for `== \d+ then` or `op == \d+`.
            if &bytes[i..i + 3] == b"== " {
                let mut j = i + 3;
                while j < bytes.len() && bytes[j].is_ascii_digit() { j += 1; }
                if j > i + 3 && j + 5 < bytes.len() && &bytes[j..j + 5] == b" then" {
                    arm_pattern_count += 1;
                }
            }
            i += 1;
        }
        assert_eq!(arm_pattern_count, 0,
            "found {} dispatcher arm patterns in raw output", arm_pattern_count);
    }

    #[test]
    fn output_runs_correctly_through_loadstring() {
        // Sanity: the wrapped output executes and produces the same value
        // as the plain source. The differential corpus harness exercises
        // this for many programs; here we just confirm one simple case.
        let src = "print(42)";
        let r = obfuscate(src, Options { seed: Some([124u8; 32]) }).unwrap();
        // Write the obfuscated chunk and execute it; capture stdout.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(), "luau failed: {:?}", out);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("42"), "expected 42 in output, got: {stdout}");
    }

    #[test]
    fn sub_heavy_program_produces_different_output_per_seed() {
        // A program with several `-` operations. Different seeds should
        // produce different rewrites, hence different output (variance
        // beyond what opcode renumbering / keystream alone provides).
        let src = "local x = 100 \
                   x = x - 10 \
                   x = x - 5 \
                   x = x - 2 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        // All three outputs must be distinct (variance preserved).
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn sub_rewrite_preserves_runtime_semantics() {
        // Just spot-check: an obfuscated `Sub` program produces the right
        // arithmetic answer at runtime.
        let src = "print(100 - 10 - 5 - 2)";  // 83
        let r = obfuscate(src, Options { seed: Some([55u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("83"));
    }

    #[test]
    fn add_padding_changes_output() {
        // A program with several `+` operations. Different seeds should
        // produce different rewrites (rate, k choices), hence different
        // output beyond what keystream alone provides.
        let src = "local x = 0 \
                   x = x + 10 \
                   x = x + 20 \
                   x = x + 30 \
                   x = x + 40 \
                   x = x + 50 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn add_padding_preserves_runtime_semantics() {
        // 100 + 200 + 300 = 600. Verify the obfuscated chunk prints 600.
        let src = "print(100 + 200 + 300)";
        let r = obfuscate(src, Options { seed: Some([77u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(),
            "luau exited with status {:?}; stderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("600"), "expected '600' in stdout, got: {}", stdout);
    }

    #[test]
    fn mul_scatter_changes_output() {
        let src = "local x = 1 \
                   x = x * 3 \
                   x = x * 5 \
                   x = x * 7 \
                   x = x * 11 \
                   print(x)";
        let a = obfuscate(src, Options { seed: Some([10u8; 32]) }).unwrap();
        let b = obfuscate(src, Options { seed: Some([20u8; 32]) }).unwrap();
        let c = obfuscate(src, Options { seed: Some([30u8; 32]) }).unwrap();
        assert_ne!(a.output, b.output);
        assert_ne!(b.output, c.output);
        assert_ne!(a.output, c.output);
    }

    #[test]
    fn mul_scatter_preserves_semantics() {
        let src = "print(7 * 11 * 13)";  // 1001
        let r = obfuscate(src, Options { seed: Some([88u8; 32]) }).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("obf.luau");
        std::fs::write(&path, &r.output).unwrap();
        let out = std::process::Command::new("luau").arg(&path).output().unwrap();
        assert!(out.status.success(),
            "luau exited {:?}; stderr: {}",
            out.status, String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("1001"), "expected '1001' in stdout, got: {}", stdout);
    }
}
