-- luau-obf runtime
-- Generated; do not edit by hand.

local _ENV_ = _ENV or getfenv(1)
local _select = select
local _error = error
local _pcall = pcall
local _tostring = tostring
local _type = type
local _sbyte = string.byte
local _ssub = string.sub
local _schar = string.char
local _tunpack = table.unpack or unpack
local _tcreate = function(n) local t = {} for i=1,n do t[i] = false end return t end
local _floor = math.floor
local _xor = bit32.bxor
local _tconcat = table.concat
local _tpack = table.pack or function(...)
    local t = {...}
    t.n = _select("#", ...)
    return t
end

local _KA = { 119, 229, 16, 139, 201, 232, 234, 218, 153, 49, 23, 182, 214, 234, 255, 198, 172, 122, 194, 179, 178, 57, 136, 59, 230, 14, 51, 95, 74, 139, 172, 51 }
local _KB = { 40, 37, 178, 77, 138, 227, 64, 65, 69, 161, 184, 81, 8, 0, 204, 246, 178, 169, 189, 152, 140, 38, 47, 216, 224, 201, 193, 17, 138, 63, 108, 103 }

local function _decrypt(enc)
    local out = {}
    for i = 1, #enc do
        local b = _sbyte(enc, i)
        local idx = ((i - 1) % 32) + 1
        local pos = (i - 1) % 256
        out[i] = _schar(_xor(_xor(_xor(b, _KA[idx]), pos), _KB[idx]))
    end
    return _tconcat(out)
end

local function _enc(bytes) return { _e = bytes } end

local function _const(consts, idx)
    local c = consts[idx]
    if type(c) == "table" and c._e then
        c = _decrypt(c._e)
        consts[idx] = c
    end
    return c
end

-- Opcode constants
local OP_LoadNil = 35
local OP_LoadTrue = 22
local OP_LoadFalse = 26
local OP_LoadConst = 13
local OP_Move = 10
local OP_Add = 25
local OP_Sub = 16
local OP_Mul = 29
local OP_Div = 19
local OP_Mod = 21
local OP_Pow = 14
local OP_Concat = 12
local OP_Lt = 8
local OP_Le = 2
local OP_Eq = 9
local OP_Not = 20
local OP_Neg = 32
local OP_Len = 34
local OP_GetGlobal = 33
local OP_SetGlobal = 23
local OP_Call = 3
local OP_Return = 17
local OP_Jmp = 15
local OP_JmpIfTrue = 11
local OP_JmpIfFalse = 4
local OP_Closure = 7
local OP_NewTable = 5
local OP_GetTable = 24
local OP_SetTable = 6
local OP_GetUpval = 28
local OP_SetUpval = 1
local OP_CallVar = 30
local OP_BuildResults = 27
local OP_Vararg = 18
local OP_ReturnMulti = 31


-- Constant pools per proto.
local CONSTS = {
    {  },
    { _enc("(\168\212\173#|\205\235"), _enc(",\180\205\168&|\213"), _enc(";\164\208\1704g\216"), _enc("0\182\206\1605"), _enc(",\169\207\178\024f\197\239\160\246\215\149") },
    { 30, 50, _enc("/\179\201\1713"), 2, 100, 3, 25, 0, _enc(";\164\208\1704g\216"), _enc("\r\164\195\160)z\140\244\189\234\209\131\160\158\007"), _enc(",\169\207\178\024f\197\239\160\246\215\149"), _enc("\030\173\201\166\""), 400, _enc(",\180\205\168&|\213"), 1000, 200, _enc("\029\174\194"), 1, _enc("(\168\212\173#|\205\235") },
    { 1, _enc(">\167\212\1605"), _enc(">\172\207\176)z"), _enc("4\168\206\161") },
    { false, _enc(";\164\208\1704g\216\188\185\236\214\152\242\133X\031~\173\030Q^c\199\145"), true, 0, _enc(";\164\208\1704g\216") },
    { _enc("(\168\212\173#|\205\235\181\245\133\129\167\148I\031l\167MHEy\216\128w\168\141"), false, true, 0, _enc("(\168\212\173#|\205\235"), _enc("6\175\211\176!h\197\255\189\252\203\152\242\129HQj\177") },
    { _enc("+\174\211\1775g\194\251"), _enc("\127\181\216\1714'"), _enc("\030\162\195\1702`\216\188"), _enc("\127\233"), _enc("e\225\132") },
    { _enc(">\172\207\176)z"), _enc("\127\225"), _enc("/\179\201\1713"), _enc("+\174\211\1775g\194\251"), _enc("\127\229"), _enc("4\168\206\161"), _enc("q\225"), _enc("\127\236\158\229c"), _enc(">\167\212\1605"), 1 },
}

-- Bytecode strings per proto.
local CODE = {
    "\007\000\000\001\000\000\000\000\000\n\001\000\000\000\007\002\000\002\000\001\000\001\000\000\001\000\n\003\000\002\000\n\004\000\003\000\003\255\255\004\000\000\000\017\255\255",
    "\n\002\000\001\000\005\003\000\n\004\000\003\000\007\005\000\003\000\000\000\001\000\000\004\000\n\006\000\005\000\007\007\000\004\000\001\000\002\000\000\002\000\000\006\000\n\008\000\007\000\007\t\000\005\000\002\000\002\000\000\002\000\000\006\000\n\n\000\t\000\007\011\000\006\000\003\000\003\000\000\000\000\000\002\000\000\004\000\n\012\000\011\000\007\r\000\007\000\004\000\001\000\000\004\000\n\014\000\r\000\005\015\000\r\016\000\002\000\006\015\000\016\000\008\000\r\017\000\000\000\006\015\000\017\000\n\000\r\018\000\001\000\006\015\000\018\000\012\000\r\019\000\004\000\006\015\000\019\000\014\000\r\020\000\003\000\006\015\000\020\000\000\000\017\015\000\017\255\255",
    "\028\000\000\000\000\r\001\000\011\000\r\002\000\004\000\n6\000\000\000\n7\000\001\000\n8\000\002\000\003\003\0006\000\002\000\n\004\000\003\000!\005\000\002\000\r\006\000\r\000\024\007\000\004\000\006\000\n6\000\007\000\030\008\0006\000\000\000\255\255\002\000\n6\000\005\000\030\255\2556\000\000\000\008\000\000\000\r\t\000\008\000\024\n\000\004\000\t\000\r\011\000\001\000\n6\000\n\000\n7\000\011\000\003\255\2556\000\001\000\r\012\000\008\000\024\r\000\004\000\012\000\r\014\000\006\000\n6\000\r\000\n7\000\014\000\003\255\2556\000\001\000\r\015\000\018\000\024\016\000\004\000\015\000\r\017\000\015\000\n6\000\016\000\n7\000\017\000\030\018\0006\000\001\000\255\255\002\000\027\019\000\018\000\000\000\000\000\r\020\000\017\000\024\021\000\019\000\020\000\n\022\000\021\000\r\023\000\003\000\024\024\000\019\000\023\000\n\025\000\024\000!\026\000\002\000\n6\000\026\000\n7\000\022\000\n8\000\025\000\003\255\2556\000\002\000\r\027\000\018\000\024\028\000\004\000\027\000\r\029\000\000\000\n6\000\028\000\n7\000\029\000\003\255\2556\000\001\000!\030\000\002\000\r\031\000\r\000\024 \000\004\000\031\000\n6\000 \000\030!\0006\000\000\000\255\255\002\000\n6\000\030\000\030\255\2556\000\000\000!\000\000\000!\"\000\002\000\r#\000\t\000\n6\000\"\000\n7\000#\000\003\255\2556\000\001\000\r$\000\n\000\024%\000\004\000$\000\r&\000\005\000\n6\000%\000\n7\000&\000\003\255\2556\000\001\000\028'\000\000\000\r(\000\016\000\r)\000\007\000\n6\000'\000\n7\000(\000\n8\000)\000\003*\0006\000\002\000\n+\000*\000\r,\000\008\000\024-\000+\000,\000\r.\000\014\000\n6\000-\000\n7\000.\000\003\255\2556\000\001\000\r/\000\018\000\0240\000+\000/\000\r1\000\012\000\n6\0000\000\n7\0001\000\003\255\2556\000\001\000!2\000\002\000\r3\000\r\000\0244\000+\0003\000\n6\0004\000\0305\0006\000\000\000\255\255\002\000\n6\0002\000\030\255\2556\000\000\0005\000\000\000\017\255\255",
    "\028\003\000\000\000\028\004\000\000\000\"\005\000\004\000\r\006\000\000\000\025\007\000\005\000\006\000\005\008\000\r\t\000\003\000\006\008\000\t\000\000\000\r\n\000\002\000\006\008\000\n\000\001\000\r\011\000\001\000\006\008\000\011\000\002\000\006\003\000\007\000\008\000\017\255\255",
    "\r\001\000\003\000\002\002\000\000\000\001\000\004\002\000\026\000\r\003\000\000\000\r\004\000\001\000\027\005\000\255\255\000\000\002\000\003\000\004\000\031\005\000\028\006\000\000\000\025\007\000\006\000\000\000\001\000\000\007\000\028\008\000\001\000\r\t\000\004\000\028\n\000\000\000\n\014\000\008\000\n\015\000\t\000\n\016\000\000\000\n\017\000\n\000\003\255\255\014\000\003\000\r\011\000\002\000\028\012\000\000\000\027\r\000\255\255\001\000\002\000\011\000\012\000\031\r\000\015\168\255\017\255\255",
    "\r\001\000\003\000\002\002\000\000\000\001\000\004\002\000\026\000\r\003\000\001\000\r\004\000\000\000\027\005\000\255\255\000\000\002\000\003\000\004\000\031\005\000\028\006\000\000\000\008\007\000\006\000\000\000\004\007\000 \000\015\003\000\015\233\255\r\008\000\001\000\r\t\000\005\000\027\n\000\255\255\001\000\002\000\008\000\t\000\031\n\000\028\011\000\000\000\016\012\000\011\000\000\000\001\000\000\012\000\028\r\000\001\000\r\014\000\004\000\028\015\000\000\000\n\019\000\r\000\n\020\000\014\000\n\021\000\000\000\n\022\000\015\000\003\255\255\019\000\003\000\r\016\000\002\000\028\017\000\000\000\027\018\000\255\255\002\000\002\000\016\000\017\000\031\018\000\015\168\255\017\255\255",
    "\r\000\000\002\000!\001\000\000\000\028\002\000\000\000\n\020\000\001\000\n\021\000\002\000\003\003\000\020\000\001\000\012\004\000\000\000\003\000\r\005\000\004\000\012\006\000\004\000\005\000!\007\000\000\000\028\008\000\001\000\n\020\000\007\000\n\021\000\008\000\003\t\000\020\000\001\000\012\n\000\006\000\t\000\r\011\000\003\000\012\012\000\n\000\011\000!\r\000\000\000\028\014\000\002\000\"\015\000\014\000\n\020\000\r\000\n\021\000\015\000\003\016\000\020\000\001\000\012\017\000\012\000\016\000\r\018\000\001\000\012\019\000\017\000\018\000\017\019\000\017\255\255",
    "\028\001\000\000\000\"\002\000\001\000\n\003\000\002\000\016\004\000\003\000\000\000\r\005\000\t\000\025\006\000\004\000\005\000\n\007\000\006\000\r\008\000\t\000\008\t\000\007\000\008\000\004\t\000\r\000\r\n\000\t\000\n\007\000\n\000\015\000\000\n\011\000\007\000\r\012\000\t\000\n\r\000\003\000\n\014\000\012\000\002\015\000\011\000\r\000\004\015\000\247\000\028\016\000\000\000\024\017\000\016\000\011\000\n\018\000\017\000!\019\000\002\000\r\020\000\001\000!\021\000\003\000\n.\000\021\000\n/\000\011\000\003\022\000.\000\001\000\012\023\000\020\000\022\000\r\024\000\006\000\012\025\000\023\000\024\000!\026\000\003\000\r\027\000\005\000\024\028\000\018\000\027\000\n.\000\026\000\n/\000\028\000\003\029\000.\000\001\000\012\030\000\025\000\029\000\r\031\000\004\000\012 \000\030\000\031\000!!\000\003\000\r\"\000\000\000\024#\000\018\000\"\000\n.\000!\000\n/\000#\000\003$\000.\000\001\000\012%\000 \000$\000\r&\000\007\000\012'\000%\000&\000!(\000\003\000\r)\000\008\000\024*\000\018\000)\000\n.\000(\000\n/\000*\000\003+\000.\000\001\000\012,\000'\000+\000\n.\000\019\000\n/\000,\000\003\255\255.\000\001\000\025-\000\011\000\014\000\n\011\000-\000\015\253\254\017\255\255",
}

-- Proto metadata: { num_params, num_regs, num_upvals, is_vararg }
local META = {
    { 0, 5, 0, 0 },
    { 2, 21, 0, 0 },
    { 0, 57, 1, 0 },
    { 3, 12, 1, 0 },
    { 1, 18, 2, 0 },
    { 1, 23, 2, 0 },
    { 0, 22, 3, 0 },
    { 1, 48, 1, 0 },
}

-- Read a little-endian u16 at byte offset `pos` (1-based) of `code`.
local function read_u16(code, pos)
    local lo = _sbyte(code, pos)
    local hi = _sbyte(code, pos + 1)
    return lo + hi * 256
end

-- Read signed i16 at 1-based offset.
local function read_i16(code, pos)
    local v = read_u16(code, pos)
    if v >= 32768 then v = v - 65536 end
    return v
end

local vm_call
vm_call = function(proto_id, args, nargs, upvals)
    local code = CODE[proto_id + 1]
    local consts = CONSTS[proto_id + 1]
    local meta = META[proto_id + 1]
    local num_params = meta[1]
    local num_regs = meta[2]
    local is_vararg = meta[4] == 1
    upvals = upvals or {}
    local regs = _tcreate(num_regs + 1)
    for i = 1, num_params do
        regs[i - 1] = args[i]
    end
    local frame_varargs
    if is_vararg then
        local vn = nargs - num_params
        if vn < 0 then vn = 0 end
        frame_varargs = {}
        for i = 1, vn do
            frame_varargs[i] = args[num_params + i]
        end
        frame_varargs.n = vn
    end
    local pc = 1
    local code_len = #code
    while pc <= code_len do
        local op = _sbyte(code, pc)
        pc = pc + 1
        if op == OP_LoadNil then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = nil
        elseif op == OP_LoadTrue then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = true
        elseif op == OP_LoadFalse then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = false
        elseif op == OP_LoadConst then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = _const(consts, b + 1)
        elseif op == OP_Move then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b]
        elseif op == OP_Add then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] + regs[c]
        elseif op == OP_Sub then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] - regs[c]
        elseif op == OP_Mul then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] * regs[c]
        elseif op == OP_Div then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] / regs[c]
        elseif op == OP_Mod then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] % regs[c]
        elseif op == OP_Pow then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] ^ regs[c]
        elseif op == OP_Concat then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = _tostring(regs[b]) .. _tostring(regs[c])
        elseif op == OP_Lt then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] < regs[c]
        elseif op == OP_Le then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] <= regs[c]
        elseif op == OP_Eq then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b] == regs[c]
        elseif op == OP_Not then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = not regs[b]
        elseif op == OP_Neg then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = -regs[b]
        elseif op == OP_Len then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = #regs[b]
        elseif op == OP_GetGlobal then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            regs[a] = _ENV_[_const(consts, b + 1)]
        elseif op == OP_SetGlobal then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            _ENV_[_const(consts, a + 1)] = regs[b]
        elseif op == OP_Call then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local n = read_i16(code, pc); pc = pc + 2
            local fn = regs[b]
            local call_args = {}
            for i = 1, n do
                call_args[i] = regs[b + i]
            end
            local result = fn(_tunpack(call_args, 1, n))
            if a ~= 0xFFFF then
                regs[a] = result
            end
        elseif op == OP_Return then
            local a = read_u16(code, pc); pc = pc + 2
            if a == 0xFFFF then
                return nil
            else
                return regs[a]
            end
        elseif op == OP_Jmp then
            local delta = read_i16(code, pc); pc = pc + 2
            pc = pc + delta
        elseif op == OP_JmpIfTrue then
            local a = read_u16(code, pc); pc = pc + 2
            local delta = read_i16(code, pc); pc = pc + 2
            if regs[a] then
                pc = pc + delta
            end
        elseif op == OP_JmpIfFalse then
            local a = read_u16(code, pc); pc = pc + 2
            local delta = read_i16(code, pc); pc = pc + 2
            if not regs[a] then
                pc = pc + delta
            end
        elseif op == OP_Closure then
            local a = read_u16(code, pc); pc = pc + 2
            local pid = read_u16(code, pc); pc = pc + 2
            -- Skip the closure_idx operand (Rust-side encoding bookkeeping only).
            pc = pc + 2
            local n_upvals = read_u16(code, pc); pc = pc + 2
            local new_upvals = {}
            for u = 1, n_upvals do
                local kind = _sbyte(code, pc); pc = pc + 1
                local payload = read_u16(code, pc); pc = pc + 2
                if kind == 0 then
                    -- LocalReg: build a cell referencing current frame's regs.
                    new_upvals[u] = { tbl = regs, key = payload }
                else
                    -- ParentUpval: share the existing cell from this frame's upvals.
                    new_upvals[u] = upvals[payload + 1]
                end
            end
            local captured = new_upvals
            regs[a] = function(...)
                local pa = { ... }
                local n = _select("#", ...)
                return vm_call(pid, pa, n, captured)
            end
        elseif op == OP_NewTable then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = {}
        elseif op == OP_GetTable then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a] = regs[b][regs[c]]
        elseif op == OP_SetTable then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local c = read_u16(code, pc); pc = pc + 2
            regs[a][regs[b]] = regs[c]
        elseif op == OP_GetUpval then
            local a = read_u16(code, pc); pc = pc + 2
            local idx = read_u16(code, pc); pc = pc + 2
            local cell = upvals[idx + 1]
            regs[a] = cell.tbl[cell.key]
        elseif op == OP_SetUpval then
            local idx = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local cell = upvals[idx + 1]
            cell.tbl[cell.key] = regs[b]
        elseif op == OP_CallVar then
            local a = read_u16(code, pc); pc = pc + 2
            local b = read_u16(code, pc); pc = pc + 2
            local n = read_i16(code, pc); pc = pc + 2
            local sp = read_u16(code, pc); pc = pc + 2
            local mode = read_i16(code, pc); pc = pc + 2
            local fn = regs[b]
            local call_args = {}
            local total_n = n
            for i = 1, n do
                call_args[i] = regs[b + i]
            end
            if sp ~= 0xFFFF then
                local sp_tbl = regs[sp]
                local m = sp_tbl.n or 0
                for i = 1, m do
                    call_args[total_n + 1] = sp_tbl[i]
                    total_n = total_n + 1
                end
            end
            if mode == 0 then
                fn(_tunpack(call_args, 1, total_n))
            elseif mode == 1 then
                local r = fn(_tunpack(call_args, 1, total_n))
                if a ~= 0xFFFF then
                    regs[a] = r
                end
            else
                local results = _tpack(fn(_tunpack(call_args, 1, total_n)))
                regs[a] = results
            end
        elseif op == OP_BuildResults then
            local a = read_u16(code, pc); pc = pc + 2
            local sp = read_u16(code, pc); pc = pc + 2
            pc = pc + 2  -- skip the br_idx operand (Rust-side encoding bookkeeping)
            local n_values = read_u16(code, pc); pc = pc + 2
            local tbl = {}
            local count = 0
            for _ = 1, n_values do
                local r = read_u16(code, pc); pc = pc + 2
                count = count + 1
                tbl[count] = regs[r]
            end
            if sp ~= 0xFFFF then
                local sp_tbl = regs[sp]
                local m = sp_tbl.n or 0
                for i = 1, m do
                    count = count + 1
                    tbl[count] = sp_tbl[i]
                end
            end
            tbl.n = count
            regs[a] = tbl
        elseif op == OP_Vararg then
            local a = read_u16(code, pc); pc = pc + 2
            regs[a] = frame_varargs
        elseif op == OP_ReturnMulti then
            local a = read_u16(code, pc); pc = pc + 2
            local tbl = regs[a]
            return _tunpack(tbl, 1, tbl.n)
        else
            _error("luau-obf: unknown opcode " .. _tostring(op))
        end
    end
    return nil
end

-- Bootstrap: main proto is index 0; forward varargs.
local main_args = { ... }
local main_n = _select("#", ...)
return vm_call(0, main_args, main_n, {})