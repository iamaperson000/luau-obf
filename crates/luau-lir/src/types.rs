//! Low-level IR. One per VM proto. Bytecode is linear; jumps refer to
//! byte offsets that the emit stage finalizes.

use luau_mir::Constant;

/// Physical register index, 0-based per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg(pub u16);

/// Constant pool index, per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstIdx(pub u16);

/// Function id, per program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtoId(pub u16);

/// Symbolic opcode. The actual numeric byte is assigned by `luau-emit::opmap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    LoadNil, LoadTrue, LoadFalse, LoadConst,
    Move,
    Add, Sub, Mul, Div, Mod, Pow,
    Concat,
    Lt, Le, Eq,
    Not, Neg, Len,
    GetGlobal, SetGlobal,
    Call, Return,
    Jmp, JmpIfTrue, JmpIfFalse,
    Closure,
    NewTable, GetTable, SetTable,
    GetUpval, SetUpval,
}

#[derive(Debug, Clone)]
pub struct LirInstr {
    pub op: OpKind,
    pub operands: Vec<Operand>,
}

#[derive(Debug, Clone, Copy)]
pub enum Operand {
    Reg(Reg),
    Const(ConstIdx),
    Proto(ProtoId),
    JmpTarget(BlockLabel),
    SmallInt(i16),
    UpvalIdx(u16),
}

/// Per-Closure-instruction upvalue source. Stored on LirFunction (variable-length,
/// outside the operand stream because operands are uniformly 2 bytes).
#[derive(Debug, Clone, Copy)]
pub enum UpvalSource {
    /// Capture from the current frame's local at this register.
    LocalReg(u16),
    /// Re-capture from the current frame's upvalue at this index.
    ParentUpval(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockLabel(pub u32);

#[derive(Debug, Clone)]
pub struct LirFunction {
    pub id: ProtoId,
    pub num_params: u16,
    pub num_regs: u16,
    /// Number of upvalues this function reads. Renders into the proto meta.
    pub num_upvals: u16,
    pub consts: Vec<Constant>,
    pub instrs: Vec<LirInstr>,
    pub label_positions: Vec<(BlockLabel, u32)>,
    /// Per-Closure-instruction upvalue source lists. Consumed by the encoder
    /// in order — the i-th Closure instruction's sources are at index i here.
    pub closure_upval_sources: Vec<Vec<UpvalSource>>,
}

#[derive(Debug, Clone)]
pub struct LirProgram {
    pub functions: Vec<LirFunction>,
}
