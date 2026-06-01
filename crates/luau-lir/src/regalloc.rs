//! Trivially-simple register allocator: assigns each VLocal a unique register.
//! Plan 2 swaps this for live-range-based allocation that scatters registers.

use luau_mir::{MirFunction, VLocal};
use std::collections::HashMap;

pub struct RegMap {
    map: HashMap<VLocal, u16>,
    next: u16,
}

impl Default for RegMap {
    fn default() -> Self {
        Self::new()
    }
}

impl RegMap {
    pub fn new() -> Self {
        Self { map: HashMap::new(), next: 0 }
    }

    pub fn assign_all(f: &MirFunction) -> Self {
        let mut rm = Self::new();
        for v in &f.params {
            rm.get_or_alloc(*v);
        }
        for block in &f.blocks {
            for instr in &block.instrs {
                visit_vlocals(instr, |v| {
                    rm.get_or_alloc(v);
                });
            }
            visit_terminator_vlocals(&block.terminator, |v| {
                rm.get_or_alloc(v);
            });
        }
        rm
    }

    pub fn get(&self, v: VLocal) -> u16 {
        *self.map.get(&v).expect("vlocal must have a register")
    }

    pub fn get_or_alloc(&mut self, v: VLocal) -> u16 {
        if let Some(r) = self.map.get(&v) {
            *r
        } else {
            let r = self.next;
            self.next += 1;
            self.map.insert(v, r);
            r
        }
    }

    pub fn num_regs(&self) -> u16 {
        self.next
    }
}

fn visit_vlocals(instr: &luau_mir::Instr, mut f: impl FnMut(VLocal)) {
    use luau_mir::{Instr, Value};
    fn visit_value(v: Value, f: &mut impl FnMut(VLocal)) {
        if let Value::VLocal(l) = v {
            f(l);
        }
    }
    match instr {
        Instr::LoadConst { dst, .. } => f(*dst),
        Instr::Move { dst, src } => { f(*dst); f(*src); }
        Instr::BinOp { dst, lhs, rhs, .. } => {
            f(*dst);
            visit_value(*lhs, &mut f);
            visit_value(*rhs, &mut f);
        }
        Instr::UnOp { dst, operand, .. } => {
            f(*dst);
            visit_value(*operand, &mut f);
        }
        Instr::GetGlobal { dst, .. } => f(*dst),
        Instr::SetGlobal { value, .. } => visit_value(*value, &mut f),
        Instr::Call { dst, callee, args } => {
            if let Some(d) = dst { f(*d); }
            visit_value(*callee, &mut f);
            for a in args { visit_value(*a, &mut f); }
        }
        Instr::MakeClosure { dst, .. } => f(*dst),
        Instr::NewTable { dst } => f(*dst),
        Instr::GetIndex { dst, obj, key } => {
            f(*dst);
            visit_value(*obj, &mut f);
            visit_value(*key, &mut f);
        }
        Instr::SetIndex { obj, key, value } => {
            visit_value(*obj, &mut f);
            visit_value(*key, &mut f);
            visit_value(*value, &mut f);
        }
    }
}

fn visit_terminator_vlocals(t: &luau_mir::Terminator, mut f: impl FnMut(VLocal)) {
    use luau_mir::{Terminator, Value};
    match t {
        Terminator::Goto(_) => {}
        Terminator::Branch { cond, .. } => {
            if let Value::VLocal(l) = cond { f(*l); }
        }
        Terminator::Return(Some(Value::VLocal(l))) => f(*l),
        Terminator::Return(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luau_mir::{lower::lower as mir_lower, MirProgram};

    fn mir_of(src: &str) -> MirProgram {
        let ast = luau_parse::parse(src).unwrap();
        let hir = luau_hir::lower::lower(&ast).unwrap();
        mir_lower(&hir).unwrap()
    }

    #[test]
    fn assigns_registers_to_all_vlocals() {
        let p = mir_of("local x = 1 local y = x + 2");
        let rm = RegMap::assign_all(p.main());
        assert!(rm.num_regs() >= 2);
    }
}
