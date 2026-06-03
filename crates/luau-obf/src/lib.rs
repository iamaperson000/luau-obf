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
}
