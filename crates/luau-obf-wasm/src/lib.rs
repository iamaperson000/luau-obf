//! WebAssembly bindings for `luau-obf`. Exposes one function that obfuscates a
//! Luau source string and returns the resulting Luau chunk (plus the seed used).

use luau_obf::{obfuscate, Options};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ObfuscateOk {
    ok: bool,
    output: String,
    seed_used: String,
}

#[derive(Serialize)]
struct ObfuscateErr {
    ok: bool,
    error: String,
}

/// Obfuscate the given Luau source.
///
/// `seed_hex` may be `None`/`null`/empty for a random seed, or a 64-character hex
/// string for a deterministic build.
///
/// Returns a JS object with either `{ ok: true, output, seed_used }` or
/// `{ ok: false, error }`.
#[wasm_bindgen(js_name = obfuscate)]
pub fn obfuscate_js(source: &str, seed_hex: Option<String>) -> JsValue {
    let seed = match parse_seed(seed_hex.as_deref()) {
        Ok(s) => s,
        Err(e) => return err(&e),
    };

    match obfuscate(source, Options { seed }) {
        Ok(r) => match serde_wasm_bindgen::to_value(&ObfuscateOk {
            ok: true,
            output: r.output,
            seed_used: hex_of(&r.seed_used),
        }) {
            Ok(v) => v,
            Err(e) => err(&format!("serialization error: {e}")),
        },
        Err(e) => err(&e.to_string()),
    }
}

fn err(msg: &str) -> JsValue {
    serde_wasm_bindgen::to_value(&ObfuscateErr {
        ok: false,
        error: msg.to_string(),
    })
    .unwrap_or(JsValue::NULL)
}

fn parse_seed(hex: Option<&str>) -> Result<Option<[u8; 32]>, String> {
    let Some(h) = hex else {
        return Ok(None);
    };
    let h = h.trim();
    if h.is_empty() {
        return Ok(None);
    }
    if h.len() != 64 {
        return Err(format!(
            "seed must be 64 hex chars (32 bytes); got {}",
            h.len()
        ));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let byte = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16)
            .map_err(|_| "invalid hex in seed".to_string())?;
        out[i] = byte;
    }
    Ok(Some(out))
}

fn hex_of(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}
