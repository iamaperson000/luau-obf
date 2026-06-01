//! Renders the VM template against the LIR + opcode map to produce a single
//! Luau chunk.

use crate::encode::encode_function;
use crate::opmap::{OpMap, ALL_OPS};
use crate::EmitError;
use luau_lir::{LirProgram, OpKind};
use luau_mir::Constant;
use minijinja::Environment;
use rand_chacha::ChaCha20Rng;
use serde::Serialize;

pub fn render(
    program: &LirProgram,
    opmap: &OpMap,
    _rng: &mut ChaCha20Rng,
) -> Result<String, EmitError> {
    let opcodes: Vec<(String, u8)> = ALL_OPS
        .iter()
        .map(|k| (opname(*k).to_string(), opmap.opcode_of(*k)))
        .collect();

    let consts: Vec<String> = program
        .functions
        .iter()
        .map(|f| format_const_pool(&f.consts))
        .collect();

    let codes: Vec<String> = program
        .functions
        .iter()
        .map(|f| {
            let bytes = encode_function(f, opmap);
            format!("\"{}\"", encode_luau_string_literal(&bytes))
        })
        .collect();

    let meta: Vec<MetaEntry> = program
        .functions
        .iter()
        .map(|f| MetaEntry {
            num_params: f.num_params,
            num_regs: std::cmp::max(f.num_regs, f.num_params),
        })
        .collect();

    let mut env = Environment::new();
    env.add_template("vm", luau_runtime::VM_TEMPLATE)
        .map_err(|e| EmitError::Template(e.to_string()))?;
    let tmpl = env.get_template("vm").unwrap();
    tmpl.render(minijinja::context! {
        opcodes => opcodes,
        consts => consts,
        codes => codes,
        meta => meta,
    })
    .map_err(|e| EmitError::Template(e.to_string()))
}

#[derive(Serialize)]
struct MetaEntry {
    num_params: u16,
    num_regs: u16,
}

fn opname(k: OpKind) -> &'static str {
    match k {
        OpKind::LoadNil => "LoadNil",
        OpKind::LoadTrue => "LoadTrue",
        OpKind::LoadFalse => "LoadFalse",
        OpKind::LoadConst => "LoadConst",
        OpKind::Move => "Move",
        OpKind::Add => "Add", OpKind::Sub => "Sub", OpKind::Mul => "Mul",
        OpKind::Div => "Div", OpKind::Mod => "Mod", OpKind::Pow => "Pow",
        OpKind::Concat => "Concat",
        OpKind::Lt => "Lt", OpKind::Le => "Le", OpKind::Eq => "Eq",
        OpKind::Not => "Not", OpKind::Neg => "Neg", OpKind::Len => "Len",
        OpKind::GetGlobal => "GetGlobal", OpKind::SetGlobal => "SetGlobal",
        OpKind::Call => "Call", OpKind::Return => "Return",
        OpKind::Jmp => "Jmp",
        OpKind::JmpIfTrue => "JmpIfTrue", OpKind::JmpIfFalse => "JmpIfFalse",
        OpKind::Closure => "Closure",
        OpKind::NewTable => "NewTable",
        OpKind::GetTable => "GetTable",
        OpKind::SetTable => "SetTable",
    }
}

fn format_const_pool(consts: &[Constant]) -> String {
    let parts: Vec<String> = consts.iter().map(format_const).collect();
    parts.join(", ")
}

fn format_const(c: &Constant) -> String {
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
        Constant::String(s) => format!("\"{}\"", escape_luau_string(s)),
    }
}

fn escape_luau_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\{}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn encode_luau_string_literal(bytes: &[u8]) -> String {
    // Use zero-padded 3-digit decimal escapes (\NNN) for every byte that is not
    // safe printable ASCII.  Three digits is Luau's maximum for numeric string
    // escapes, so the next character can *never* be consumed as part of this
    // escape.  This avoids the subtle bug where the unpadded form (e.g. `\14`)
    // followed by a printable digit (e.g. `0`) would be mis-parsed by Luau as a
    // single 3-digit escape `\140` (byte 140) instead of byte 14 + character '0'.
    let mut out = String::with_capacity(bytes.len() * 4);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"'  => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            // Safe printable ASCII (not a special character above).
            // These are never confused with a preceding \NNN escape because
            // \NNN is 3 digits — the maximum — leaving no room for extension.
            0x20..=0x7E => out.push(b as char),
            // All other bytes: zero-padded 3-digit decimal.
            _ => out.push_str(&format!("\\{:03}", b)),
        }
    }
    out
}
