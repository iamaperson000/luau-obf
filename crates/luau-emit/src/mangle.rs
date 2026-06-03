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
            let j = i + 2;
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
