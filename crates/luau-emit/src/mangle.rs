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
