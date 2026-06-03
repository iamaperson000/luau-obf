


local _xc = _ENV or getfenv(1)
local _dq = select
local _bg = error
local _sy = pcall
local _pb = tostring
local _oe = type
local _za = string.byte
local _ya = string.sub
local _ed = string.char
local _si = table.unpack or unpack
local _yy = function(_od) local _pl = {} for _xp=1,_od do _pl[_xp] = false end return _pl end
local _yo = math.floor
local _ec = bit32.bxor
local _xy = table.concat
local _dm = table.pack or function(...)
    local _pl = {...}
    _pl.n = _dq("#", ...)
    return _pl
end

local _oq = { 244, 232, 180, 201, 176, 80, 85, 115, 142, 108, 52, 169, 192, 126, 53, 5, 223, 199, 49, 37, 102, 223, 232, 2, 165, 113, 165, 93, 129, 105, 29, 40 }
local _yn = { 223, 88, 115, 219, 103, 76, 17, 213, 181, 221, 248, 21, 234, 21, 24, 11, 2, 231, 238, 196, 30, 145, 130, 174, 56, 26, 68, 38, 92, 158, 58, 10 }

local function _ju(enc, salt)
    local _ez = {}
    local salt_term = (salt * 7919) % 4294967296
    for _xp = 1, #enc do
        local _rt = _za(enc, _xp)
        local _mo = ((_xp - 1) % 32) + 1
        local _ev = ((_xp - 1) + salt_term) % 256
        _ez[_xp] = _ed(_ec(_ec(_ec(_rt, _oq[_mo]), _ev), _yn[_mo]))
    end
    return _xy(_ez)
end

local function _ot(bytes) return { _e = bytes } end

local function _lr(_wg, _mo, _yt)
    local _md = _wg[_mo]
    if type(_md) == "table" and _md._e then
        _md = _ju(_md._e, _yt)
        _wg[_mo] = _md
    end
    return _md
end


local _of = 3
local _lj = 2
local _ak = 33
local _ek = 34
local _wt = 16
local _ha = 4
local _xa = 30
local _fe = 29
local _xd = 31
local _cu = 21
local _kj = 13
local _mi = 9
local _ja = 35
local _nx = 15
local _le = 23
local _ul = 18
local _px = 22
local _vg = 32
local _zn = 25
local _am = 12
local _pd = 14
local _lf = 6
local _ew = 5
local _jr = 8
local _ov = 27
local _ba = 11
local _oj = 1
local _ih = 17
local _hr = 10
local _ud = 20
local _cs = 26
local _hl = 19
local _ye = 7
local _ag = 24
local _ys = 28



local _lk = {
    {  },
    { _ot("\160%F\143W\129\197"), _ot("\179)B\136@\154\208'"), _ot("\1835[\141E\154\200"), _ot("\183(Y\151{\128\216#\184&G?"), _ot("\1717X\133V") },
    { 100, _ot("\130\006S\155Q\141\1934"), 25, 400, 50, _ot("\180\003N\144P"), 3, _ot("\167\nD\150[\139\128+\180%P:\178\249\251"), _ot("\145\nW\156F\150\212"), 0, _ot("\183\000E"), 1, 2, 30, 200, 1000, _ot("\134\007H\132j\151\2010\1699V,"), _ot("\134\026J\158T\141\217"), _ot("\133\029N\157A") },
    { _ot("\135\024|\167t"), _ot("\135\019g\183h\186"), 1, _ot("\141\023f\166") },
    { true, _ot("\243h\t\194d\180\242"), 0, _ot("\243h\t\194d\180\242E\146\001y\015\194\192\130\229a\130bG\220\246\206\026"), false },
    { false, _ot("\247u\030\212\028\222\148c"), 0, true, _ot("\233r\025\201\030\202\156w\225`\023~\189\181\225\218\002\239"), _ot("\247u\030\212\028\222\148c\233iYg\232\160\224\148\004\249B/\168\253\194\0267\217A") },
    { _ot("\145_#\225:\170"), _ot("\197D(\251;\234\138`"), _ot("\145\003"), _ot("\240H8\224<\237\144'"), _ot("\139\011\127") },
    { _ot("\210H%\240."), _ot("\130\023r\190~"), _ot("\195\\8\251("), _ot("\201S\"\250"), _ot("\140\026"), _ot("\130\030"), _ot("\195W#\2354\230"), 1, _ot("\214U?\234(\251\165Q"), _ot("\130\026") },
}


local _hh = {
    "\011\000\000\001\000\000\000\000\000\016\001\000\000\000\011\002\000\002\000\001\000\001\000\000\001\000\016\003\000\002\000\016\004\000\003\000\014\255\255\004\000\000\000\006\255\255",
    "\016\002\000\001\000\001\003\000\016\004\000\003\000\011\005\000\003\000\000\000\001\000\000\004\000\016\006\000\005\000\011\007\000\004\000\001\000\002\000\000\002\000\000\006\000\016\008\000\007\000\011\t\000\005\000\002\000\002\000\000\002\000\000\006\000\016\n\000\t\000\011\011\000\006\000\003\000\003\000\000\000\000\000\002\000\000\004\000\016\012\000\011\000\011\r\000\007\000\004\000\001\000\000\004\000\016\014\000\r\000\001\015\000\"\016\000\000\000\n\015\000\016\000\008\000\"\017\000\001\000\n\015\000\017\000\n\000\"\018\000\002\000\n\015\000\018\000\012\000\"\019\000\003\000\n\015\000\019\000\014\000\"\020\000\004\000\n\015\000\020\000\000\000\006\015\000\006\255\255",
    "\020\000\000\000\000\"\001\000\005\000\"\002\000\000\000\0166\000\000\000\0167\000\001\000\0168\000\002\000\014\003\0006\000\002\000\016\004\000\003\000\025\005\000\018\000\"\006\000\017\000\017\007\000\004\000\006\000\0166\000\007\000\019\008\0006\000\000\000\255\255\002\000\0166\000\005\000\019\255\2556\000\000\000\008\000\000\000\"\t\000\008\000\017\n\000\004\000\t\000\"\011\000\004\000\0166\000\n\000\0167\000\011\000\014\255\2556\000\001\000\"\012\000\008\000\017\r\000\004\000\012\000\"\014\000\002\000\0166\000\r\000\0167\000\014\000\014\255\2556\000\001\000\"\015\000\001\000\017\016\000\004\000\015\000\"\017\000\014\000\0166\000\016\000\0167\000\017\000\019\018\0006\000\001\000\255\255\002\000\007\019\000\018\000\000\000\000\000\"\020\000\011\000\017\021\000\019\000\020\000\016\022\000\021\000\"\023\000\012\000\017\024\000\019\000\023\000\016\025\000\024\000\025\026\000\018\000\0166\000\026\000\0167\000\022\000\0168\000\025\000\014\255\2556\000\002\000\"\027\000\001\000\017\028\000\004\000\027\000\"\029\000\r\000\0166\000\028\000\0167\000\029\000\014\255\2556\000\001\000\025\030\000\018\000\"\031\000\017\000\017 \000\004\000\031\000\0166\000 \000\019!\0006\000\000\000\255\255\002\000\0166\000\030\000\019\255\2556\000\000\000!\000\000\000\025\"\000\018\000\"#\000\007\000\0166\000\"\000\0167\000#\000\014\255\2556\000\001\000\"$\000\016\000\017%\000\004\000$\000\"&\000\006\000\0166\000%\000\0167\000&\000\014\255\2556\000\001\000\020'\000\000\000\"(\000\n\000\")\000\t\000\0166\000'\000\0167\000(\000\0168\000)\000\014*\0006\000\002\000\016+\000*\000\",\000\008\000\017-\000+\000,\000\".\000\015\000\0166\000-\000\0167\000.\000\014\255\2556\000\001\000\"/\000\001\000\0170\000+\000/\000\"1\000\003\000\0166\0000\000\0167\0001\000\014\255\2556\000\001\000\0252\000\018\000\"3\000\017\000\0174\000+\0003\000\0166\0004\000\0195\0006\000\000\000\255\255\002\000\0166\0002\000\019\255\2556\000\000\0005\000\000\000\006\255\255",
    "\020\003\000\000\000\020\004\000\000\000 \005\000\004\000\"\006\000\002\000\004\007\000\005\000\006\000\001\008\000\"\t\000\003\000\n\008\000\t\000\000\000\"\n\000\001\000\n\008\000\n\000\001\000\"\011\000\000\000\n\008\000\011\000\002\000\n\003\000\007\000\008\000\006\255\255",
    "\"\001\000\002\000\015\002\000\000\000\001\000\027\002\000\026\000\"\003\000\004\000\"\004\000\003\000\007\005\000\255\255\000\000\002\000\003\000\004\000\028\005\000\020\006\000\000\000\004\007\000\006\000\000\000\026\000\000\007\000\020\008\000\001\000\"\t\000\001\000\020\n\000\000\000\016\014\000\008\000\016\015\000\t\000\016\016\000\000\000\016\017\000\n\000\014\255\255\014\000\003\000\"\011\000\000\000\020\012\000\000\000\007\r\000\255\255\001\000\002\000\011\000\012\000\028\r\000\005\168\255\006\255\255",
    "\"\001\000\002\000\015\002\000\000\000\001\000\027\002\000\026\000\"\003\000\000\000\"\004\000\005\000\007\005\000\255\255\000\000\002\000\003\000\004\000\028\005\000\020\006\000\000\000#\007\000\006\000\000\000\027\007\000 \000\005\003\000\005\233\255\"\008\000\000\000\"\t\000\004\000\007\n\000\255\255\001\000\002\000\008\000\t\000\028\n\000\020\011\000\000\000\030\012\000\011\000\000\000\026\000\000\012\000\020\r\000\001\000\"\014\000\001\000\020\015\000\000\000\016\019\000\r\000\016\020\000\014\000\016\021\000\000\000\016\022\000\015\000\014\255\255\019\000\003\000\"\016\000\003\000\020\017\000\000\000\007\018\000\255\255\002\000\002\000\016\000\017\000\028\018\000\005\168\255\006\255\255",
    "\"\000\000\003\000\025\001\000\001\000\020\002\000\000\000\016\020\000\001\000\016\021\000\002\000\014\003\000\020\000\001\000\t\004\000\000\000\003\000\"\005\000\004\000\t\006\000\004\000\005\000\025\007\000\001\000\020\008\000\001\000\016\020\000\007\000\016\021\000\008\000\014\t\000\020\000\001\000\t\n\000\006\000\t\000\"\011\000\002\000\t\012\000\n\000\011\000\025\r\000\001\000\020\014\000\002\000 \015\000\014\000\016\020\000\r\000\016\021\000\015\000\014\016\000\020\000\001\000\t\017\000\012\000\016\000\"\018\000\000\000\t\019\000\017\000\018\000\006\019\000\006\255\255",
    "\020\001\000\000\000 \002\000\001\000\016\003\000\002\000\030\004\000\003\000\000\000\"\005\000\007\000\004\006\000\004\000\005\000\016\007\000\006\000\"\008\000\007\000#\t\000\007\000\008\000\027\t\000\r\000\"\n\000\007\000\016\007\000\n\000\005\000\000\016\011\000\007\000\"\012\000\007\000\016\r\000\003\000\016\014\000\012\000\015\015\000\011\000\r\000\027\015\000\247\000\020\016\000\000\000\017\017\000\016\000\011\000\016\018\000\017\000\025\019\000\000\000\"\020\000\t\000\025\021\000\008\000\016.\000\021\000\016/\000\011\000\014\022\000.\000\001\000\t\023\000\020\000\022\000\"\024\000\004\000\t\025\000\023\000\024\000\025\026\000\008\000\"\027\000\003\000\017\028\000\018\000\027\000\016.\000\026\000\016/\000\028\000\014\029\000.\000\001\000\t\030\000\025\000\029\000\"\031\000\005\000\t \000\030\000\031\000\025!\000\008\000\"\"\000\006\000\017#\000\018\000\"\000\016.\000!\000\016/\000#\000\014$\000.\000\001\000\t%\000 \000$\000\"&\000\001\000\t'\000%\000&\000\025(\000\008\000\")\000\002\000\017*\000\018\000)\000\016.\000(\000\016/\000*\000\014+\000.\000\001\000\t,\000'\000+\000\016.\000\019\000\016/\000,\000\014\255\255.\000\001\000\004-\000\011\000\014\000\016\011\000-\000\005\253\254\006\255\255",
}


local _bz = {
    { 0, 5, 0, 0 },
    { 2, 21, 0, 0 },
    { 0, 57, 1, 0 },
    { 3, 12, 1, 0 },
    { 1, 18, 2, 0 },
    { 1, 23, 2, 0 },
    { 0, 22, 3, 0 },
    { 1, 48, 1, 0 },
}


local function _kp(_kr, _ev)
    local lo = _za(_kr, _ev)
    local hi = _za(_kr, _ev + 1)
    return lo + hi * 256
end


local function _uo(_kr, _ev)
    local v = _kp(_kr, _ev)
    if v >= 32768 then v = v - 65536 end
    return v
end

local _xg
_xg = function(_yt, _xx, _rk, _uw)
    local _kr = _hh[_yt + 1]
    local _wg = _lk[_yt + 1]
    local _zi = _bz[_yt + 1]
    local _pe = _zi[1]
    local _ne = _zi[2]
    local _af = _zi[4] == 1
    _uw = _uw or {}
    local _cr = _yy(_ne + 1)
    for _xp = 1, _pe do
        _cr[_xp - 1] = _xx[_xp]
    end
    local _wf
    if _af then
        local _dc = _rk - _pe
        if _dc < 0 then _dc = 0 end
        _wf = {}
        for _xp = 1, _dc do
            _wf[_xp] = _xx[_pe + _xp]
        end
        _wf.n = _dc
    end
    local _eq = 1
    local _aw = #_kr
    while _eq <= _aw do
        local _rf = _za(_kr, _eq)
        _eq = _eq + 1
        if _rf == _of then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = nil
        elseif _rf == _lj then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = true
        elseif _rf == _ak then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = false
        elseif _rf == _ek then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _lr(_wg, _rt + 1, _yt)
        elseif _rf == _wt then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt]
        elseif _rf == _ha then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] + _cr[_md]
        elseif _rf == _xa then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] - _cr[_md]
        elseif _rf == _fe then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] * _cr[_md]
        elseif _rf == _xd then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] / _cr[_md]
        elseif _rf == _cu then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] % _cr[_md]
        elseif _rf == _kj then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] ^ _cr[_md]
        elseif _rf == _mi then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _pb(_cr[_rt]) .. _pb(_cr[_md])
        elseif _rf == _ja then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] < _cr[_md]
        elseif _rf == _nx then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] <= _cr[_md]
        elseif _rf == _le then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt] == _cr[_md]
        elseif _rf == _ul then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = not _cr[_rt]
        elseif _rf == _px then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = -_cr[_rt]
        elseif _rf == _vg then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = #_cr[_rt]
        elseif _rf == _zn then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _xc[_lr(_wg, _rt + 1, _yt)]
        elseif _rf == _am then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            _xc[_lr(_wg, _wr + 1, _yt)] = _cr[_rt]
        elseif _rf == _pd then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _od = _uo(_kr, _eq); _eq = _eq + 2
            local _tc = _cr[_rt]
            local _lw = {}
            for _xp = 1, _od do
                _lw[_xp] = _cr[_rt + _xp]
            end
            local result = _tc(_si(_lw, 1, _od))
            if _wr ~= 0xFFFF then
                _cr[_wr] = result
            end
        elseif _rf == _lf then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            if _wr == 0xFFFF then
                return nil
            else
                return _cr[_wr]
            end
        elseif _rf == _ew then
            local _db = _uo(_kr, _eq); _eq = _eq + 2
            _eq = _eq + _db
        elseif _rf == _jr then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _db = _uo(_kr, _eq); _eq = _eq + 2
            if _cr[_wr] then
                _eq = _eq + _db
            end
        elseif _rf == _ov then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _db = _uo(_kr, _eq); _eq = _eq + 2
            if not _cr[_wr] then
                _eq = _eq + _db
            end
        elseif _rf == _ba then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _kk = _kp(_kr, _eq); _eq = _eq + 2
            
            _eq = _eq + 2
            local _dz = _kp(_kr, _eq); _eq = _eq + 2
            local _ru = {}
            for _lc = 1, _dz do
                local _ry = _za(_kr, _eq); _eq = _eq + 1
                local _df = _kp(_kr, _eq); _eq = _eq + 2
                if _ry == 0 then
                    
                    _ru[_lc] = { _t = _cr, _k = _df }
                else
                    
                    _ru[_lc] = _uw[_df + 1]
                end
            end
            local _gu = _ru
            _cr[_wr] = function(...)
                local _gx = { ... }
                local _od = _dq("#", ...)
                return _xg(_kk, _gx, _od, _gu)
            end
        elseif _rf == _oj then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = {}
        elseif _rf == _ih then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _cr[_rt][_cr[_md]]
        elseif _rf == _hr then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _md = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr][_cr[_rt]] = _cr[_md]
        elseif _rf == _ud then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _mo = _kp(_kr, _eq); _eq = _eq + 2
            local _td = _uw[_mo + 1]
            _cr[_wr] = _td._t[_td._k]
        elseif _rf == _cs then
            local _mo = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _td = _uw[_mo + 1]
            _td._t[_td._k] = _cr[_rt]
        elseif _rf == _hl then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _rt = _kp(_kr, _eq); _eq = _eq + 2
            local _od = _uo(_kr, _eq); _eq = _eq + 2
            local _nv = _kp(_kr, _eq); _eq = _eq + 2
            local _jj = _uo(_kr, _eq); _eq = _eq + 2
            local _tc = _cr[_rt]
            local _lw = {}
            local _jt = _od
            for _xp = 1, _od do
                _lw[_xp] = _cr[_rt + _xp]
            end
            if _nv ~= 0xFFFF then
                local _xo = _cr[_nv]
                local _hn = _xo.n or 0
                for _xp = 1, _hn do
                    _lw[_jt + 1] = _xo[_xp]
                    _jt = _jt + 1
                end
            end
            if _jj == 0 then
                _tc(_si(_lw, 1, _jt))
            elseif _jj == 1 then
                local _sq = _tc(_si(_lw, 1, _jt))
                if _wr ~= 0xFFFF then
                    _cr[_wr] = _sq
                end
            else
                local _vr = _dm(_tc(_si(_lw, 1, _jt)))
                _cr[_wr] = _vr
            end
        elseif _rf == _ye then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _nv = _kp(_kr, _eq); _eq = _eq + 2
            _eq = _eq + 2  
            local _vf = _kp(_kr, _eq); _eq = _eq + 2
            local _tu = {}
            local _nu = 0
            for _ = 1, _vf do
                local _sq = _kp(_kr, _eq); _eq = _eq + 2
                _nu = _nu + 1
                _tu[_nu] = _cr[_sq]
            end
            if _nv ~= 0xFFFF then
                local _xo = _cr[_nv]
                local _hn = _xo.n or 0
                for _xp = 1, _hn do
                    _nu = _nu + 1
                    _tu[_nu] = _xo[_xp]
                end
            end
            _tu.n = _nu
            _cr[_wr] = _tu
        elseif _rf == _ag then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            _cr[_wr] = _wf
        elseif _rf == _ys then
            local _wr = _kp(_kr, _eq); _eq = _eq + 2
            local _tu = _cr[_wr]
            return _si(_tu, 1, _tu.n)
        else
            _bg("luau-obf: unknown opcode " .. _pb(_rf))
        end
    end
    return nil
end


local _do = { ... }
local _gc = _dq("#", ...)
return _xg(0, _do, _gc, {})