//! Mid-level IR. Sugar-free, CFG-shaped.
//!
//! The program is a list of MIR functions plus an implicit "main" function
//! (function index 0) representing the top-level chunk.

use luau_hir::{BinOp, SymbolId, UnOp};

pub use luau_hir::UpvalueSource;

/// MIR-level upvalue source: like HIR's UpvalueSource, but using VLocal IDs
/// (which the LIR layer can then translate to register numbers via regalloc).
#[derive(Debug, Clone, Copy)]
pub enum MirUpvalSource {
    /// Capture the current function's VLocal.
    Local(VLocal),
    /// Re-capture the current function's upvalue at this index.
    ParentUpval(u32),
}

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
    LoadConst { dst: VLocal, src: ConstId },
    Move { dst: VLocal, src: VLocal },
    BinOp { dst: VLocal, op: BinOp, lhs: Value, rhs: Value },
    UnOp { dst: VLocal, op: UnOp, operand: Value },
    GetGlobal { dst: VLocal, name: ConstId },
    SetGlobal { name: ConstId, value: Value },
    Call { dst: Option<VLocal>, callee: Value, args: Vec<Value> },
    /// Create a closure. `upvalues` lists how each of the new closure's upvalues
    /// is sourced from the CURRENT function's frame (locals or own upvalues).
    MakeClosure { dst: VLocal, function: FunctionId, upvalues: Vec<MirUpvalSource> },
    /// Read this function's upvalue at the given index into dst.
    GetUpval { dst: VLocal, idx: u32 },
    /// Write to this function's upvalue at the given index.
    SetUpval { idx: u32, value: Value },
    NewTable { dst: VLocal },
    GetIndex { dst: VLocal, obj: Value, key: Value },
    SetIndex { obj: Value, key: Value, value: Value },
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
    pub params: Vec<VLocal>,
    pub blocks: Vec<BasicBlock>,
    pub consts: Vec<Constant>,
    pub n_locals: u32,
    /// Upvalue sources for this function. Index = upvalue id used by GetUpval/SetUpval.
    pub upvalues: Vec<UpvalueSource>,
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
