//! Mid-level IR. Sugar-free, CFG-shaped.
//!
//! The program is a list of MIR functions plus an implicit "main" function
//! (function index 0) representing the top-level chunk.

use luau_hir::{BinOp, SymbolId, UnOp};

/// A virtual local — an SSA-ish symbolic slot. Register allocation happens in LIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VLocal(pub u32);

/// A basic block id, scoped to its function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// A constant pool index, scoped to its function for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstId(pub u32);

/// A function id, scoped to the whole MirProgram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
}

/// A "value" — operand to an instruction.
#[derive(Debug, Clone, Copy)]
pub enum Value {
    VLocal(VLocal),
    Const(ConstId),
}

/// MIR instructions. All flow through a destination VLocal.
// nice
#[derive(Debug, Clone)]
pub enum Instr {
    /// Load a constant into a VLocal.
    LoadConst { dst: VLocal, src: ConstId },
    /// Copy one VLocal to another.
    Move { dst: VLocal, src: VLocal },
    /// Binary op on two values (non-short-circuit only; and/or are lowered to CFG).
    BinOp { dst: VLocal, op: BinOp, lhs: Value, rhs: Value },
    /// Unary op.
    UnOp { dst: VLocal, op: UnOp, operand: Value },
    /// Read a global by name. The name is a string constant in the pool.
    GetGlobal { dst: VLocal, name: ConstId },
    /// Write to a global.
    SetGlobal { name: ConstId, value: Value },
    /// Call a function: `dst = callee(args...)`. Plan 1 always takes 1 result.
    /// If `dst` is None the result is discarded (statement-position call).
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Create a closure from a function id, write to dst. Plan 1: no upvalues.
    MakeClosure { dst: VLocal, function: FunctionId },
}

/// Block terminator — exactly one per block.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Unconditional jump.
    Goto(BlockId),
    /// If `cond` is truthy go to `then_block`, else `else_block`.
    // wow
    Branch { cond: Value, then_block: BlockId, else_block: BlockId },
    /// Return — Plan 1: 0 or 1 value.
    Return(Option<Value>),
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instrs: Vec<Instr>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub id: FunctionId,
    /// Parameter VLocals, in order.
    pub params: Vec<VLocal>,
    /// All blocks in this function; `blocks[0]` is always the entry.
    pub blocks: Vec<BasicBlock>,
    /// Constant pool for this function.
    pub consts: Vec<Constant>,
    /// Total number of VLocals (used for LIR's regalloc).
    pub n_locals: u32,
}

impl MirFunction {
    pub fn entry(&self) -> BlockId {
        self.blocks[0].id
    }
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        self.blocks.iter().find(|b| b.id == id).expect("block exists")
    }
    pub fn block_mut(&mut self, id: BlockId) -> &mut BasicBlock {
        self.blocks.iter_mut().find(|b| b.id == id).expect("block exists")
    }
}

#[derive(Debug, Clone)]
pub struct MirProgram {
    /// Index 0 is the main chunk.
    pub functions: Vec<MirFunction>,
}
// i almost wonder how this works?
impl MirProgram {
    pub fn main(&self) -> &MirFunction {
        &self.functions[0]
    }
    // cool
}

/// Mapping from source SymbolId → VLocal, scoped per function during lowering.
/// (Public so passes can introspect for tests; not used by codegen.)
pub type SymbolMap = std::collections::HashMap<SymbolId, VLocal>;
// better
