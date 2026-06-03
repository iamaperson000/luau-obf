


local _lp = _ENV or getfenv(1)
local _ec = select
local _zx = error
local _lc = pcall
local _yd = tostring
local _jn = type
local _qz = string.byte
local _et = string.sub
local _na = string.char
local _fx = table.unpack or unpack
local _tp = function(_nz) local _tc = {} for _te=1,_nz do _tc[_te] = false end return _tc end
local _dm = math.floor
local _wi = bit32.bxor
local _wp = table.concat
local _pg = table.pack or function(...)
    local _tc = {...}
    _tc.n = _ec("#", ...)
    return _tc
end

local _zy = { 220, 137, 130, 57, 61, 214, 142, 166, 41, 242, 74, 214, 101, 174, 191, 45, 144, 226, 95, 2, 147, 107, 188, 139, 228, 75, 237, 134, 110, 208, 114, 86 }
local _oj = { 143, 237, 92, 253, 79, 205, 252, 12, 225, 214, 83, 169, 253, 193, 157, 135, 84, 11, 144, 14, 246, 198, 123, 145, 124, 74, 188, 148, 123, 198, 185, 65 }

local _lw = 475656584
local _xe = 493031561

local function _xk(_bc, _ez, _rb)
    local _py = _qz(_bc, _ez)
    local mixed = ((_ez * _xe) + _lw + (_rb * 2654435761)) % 4294967296
    return _wi(_py, mixed % 256)
end

local function _lx(enc, salt)
    local _fl = {}
    local salt_term = (salt * 7919) % 4294967296
    for _te = 1, #enc do
        local _py = _qz(enc, _te)
        local _nt = ((_te - 1) % 32) + 1
        local _tu = ((_te - 1) + salt_term) % 256
        _fl[_te] = _na(_wi(_wi(_wi(_py, _zy[_nt]), _tu), _oj[_nt]))
    end
    return _wp(_fl)
end

local function _gt(bytes) return { _e = bytes } end

local function _qr(_pn, _nt, _no)
    local _mt = _pn[_nt]
    if type(_mt) == "table" and _mt._e then
        _mt = _lx(_mt._e, _no)
        _pn[_nt] = _mt
    end
    return _mt
end


local _gf = {
    {  },
    { _gt("\211\227AS\243"), _gt("\207\252@A\222\135\238/K\179\146\252"), _gt("\207\225B[\224\157\254"), _gt("\203\253[^\229\157\230+"), _gt("\216\241_Y\242\134\243") },
    { _gt("\233\222NJ\227\145\226"), 3, 1, 1000, _gt("\207\212\\"), 50, 400, _gt("\254\211QR\207\144\255<Z\172\131\239"), _gt("\254\206SH\241\138\239"), _gt("\253\201WK\228"), _gt("\204\215WF\245"), _gt("\250\210JM\244\138\2478"), 0, 25, 200, _gt("\223\222]@\254\140\182'G\176\133\249\000\253\244"), 100, 30, 2 },
    { 1, _gt("\245\195\127p"), _gt("\255\199~a\205\189"), _gt("\255\204eq\209") },
    { false, _gt("\139\188\016\020\193\179\196"), _gt("\139\188\016\020\193\179\196Ia\148\172\204p\196\141AxKr\170\193\021c\172"), 0, true },
    { _gt("\143\161\007\002\185\217\162o"), 0, _gt("\143\161\007\002\185\217\162o\026\252\140\164Z\164\2390\0290R\194\181\030o\1722\179\241"), false, _gt("\145\166\000\031\187\205\170{\018\245\194\189\015\177\238~\027&"), true },
    { _gt("\233\215"), _gt("\233\139:7\159\173"), _gt("\136\156!6\153\234\166+"), _gt("\243\223f"), _gt("\189\1441-\158\237\188l") },
    { _gt("\174\129&<\141\252\147]"), _gt("\250\202"), _gt("\244\206"), _gt("\187\131:=\145\225"), _gt("\170\156<&\139"), 1, _gt("\250\206"), _gt("\177\135;,"), _gt("\187\136!-\141"), _gt("\250\195kh\219") },
}


local _qc = {
    "\008\154#\1735\190G\208Y\247j\244}\006\150\026\161(\179=\197O\215`\232r\238\135\r\148\031\1895\186@\204X!\152\244y\002\139\018b\217",
    "\215I\212\\\230`\251\129\031\151\028\166.\174E\201Q\219d\237w\255\136\021\1546\1705\187G\201^\226o\244|\006\141\024\161(\179<\195N\194h\233u\251\157\004\150\026\1683\186A\204U\220g\240\127\002\158\030\157/\175!\202J\213\\\230n\244\128\t\146\027\164/\182?\204Q\207o\236~\254\158\029\153%\1710\189G\207X\229j\230r\005\131\023\175&\178<\212M\210_\247~\250\147\012\157\030\160!\185A\203K\210f\254x\011\138\020\142%\1727\223F\210I\228a\246x\027\145\027\1633\186>\212P\215b\236`\253\134\015\135.\170'\188E\206Q\239i\244\132\251",
    "q\252\133\014\151'\1682\177D\202T\223x\241o5\140\021\1582\1359\195K\193e\230m\248\140\t\147*\165,\183U\205R\216d\230s\255\129\017\157%\172=\190S\215Y\230k\242}\019\185\024\166*\1684\197x\215`\233\141\004\134\r\131)\1684\186X3\170\232g\240y\n\139\020\157!\1668\193J\199V\229j\247\137\t\149\016\164(\182*\254Q\208c\249B\254\140\016\148\221T\002\189G\207_\237j\243|\017\131\023\164)\190;\195C\214R\232d\204\131\001\149\011\1440\183B\198\171\"P\239y\001\141\028\156.\174#\208I\214[\235m\241n\008\159\026\182\026\181.\199E\238b\250t\230\148\015\174!\1713C\186\204W\228z\242i\004\141\022\159(\182.\195N\213J\242p\234\130\031\148\008\176/\173A\205D\220w\238c\024\137\001\1553\173#\166H\201Z\232v\245w\007\133/\1621\180(\241O\206a\255K\252\156\014\154\223V\004\187F\205Q\196h\250z\023\144\021\154'\1719\197V\212L\230z\206\129\022\147\t\146.\170@\196\173$R\237w\255\131\015\154*\1722\161G\216Y\246K\244y\006\144\024\180\028\179\028\197U\246`\223r\251\132\242i\029\168$\140C\210U\197\152\015O\002\139\020\188&\1758\202h\211U\229i\212\128\006\146\014\146-\148?\221f\218@\236x\001x&\153#\1713\153F\200X\245O\243x\005\170\023\167\015\178:\196X\224_\205q\239\180\012\179\030\170\207Ft\203U\221d\200x\001\138\020\180%\1707\199`\210W\228x\192\127/\145\015\148,\157>\210h\217K\235y\215\1349\152#\170&\151E\228W\231E\242{\004\153;\159\003\177\022\195K\251^\228p\236\180\011\185\029\179\024\184o\202^#\154\216w\001\137\021\180$\1666\171x\209q\227C\245y6\144\031\162>\130=\246O\205V\234B\252\136\241h\022\1693\187O\255V\214h\246I\003\132\021\138\019\176\018\194x\212H\208o\204\129\017\166\028\147.\183@6\173\217d\248@\255\186\017\129\220S\003\190G\208l\226k\244{\249p",
    "&\1746\191H\211^\227l\245v\002\144\029\162,\178=\198O\206f\234v\252\131\014\152(\1695\178D\204V\192`\241s\003\140\021\153-\176;\194T\220]\236o\249\129\r\152\028\166.\168H\201Y\219f\237i\252\136\022\154+\1723A\184",
    "\210_\231s\249\161\t\148\029\166.\184c\200S\198e\233t\000\137\018\156 \1734\191L\212Z\028\147\245~\005\144\026\162/\180,\195O\218g\234s\252\147\t\151&\1692\187J\205V\216h\243r\003\141\021\153.\1768\194I\222]\230o\237\143\n\155\028\176!\183I\201G\203d\237v\234\153\017\144#\161\202AI\208Z\226l\255}\002\143\026\173*\179<\193C\215\159\022s\251\134\r\157\031\1641\171N\204Pv\152\246\134\253",
    "\129\014\152 \170\016\190E\206W\225i\208y\004\151\022\152+\1779\195K\209^\229p\253\135\011k\226\166/\186A\201S\216e\255r\000\139\020\155$\173*\184H\215Z\227l\215y\007\176\025\167(\1808/\176\223i\234p\252\130\007\151$\1696\177D2\169\222h\243z\011\140\028\1586\1869\192@\212]\230l\244\129\001\147\028\165 \183@\197R\217i\237w\255\143\031\154#\1727\177G\208Y\247x\244p\006\154\012\161$\179)\208N\215`\252d\251\139\r\155\224W\"\186@\204R\206g\245y\000\154\020\157&\171*\193\181,^\229l\247\144\t\131\027\181?\182:`\174\220\156\019",
    "0\192I\208[\239l\246{\008\147\024\163,\181+\211P\216b\254a\253\132\015\149\"\170'\188D\206^\228i\242{\007\141\017\154(\178:\202J\213Z\231u\249\137\012\148\025\166-\176A\203S\201q\238p\000\156\007\155,\173;\182H\197Z\226l\252t\007\150\025\171+\1796\198O\216h\230s\246\133\005\151+\1642\191D\207X\223j\241r\012\140\027\1582\1649\207K\193H\230`\248\140\026\147\008\165/\183I\216R\215d\253v\248\154\017\155#\165&\190V\208K\226m\231}\000p\231",
    "\234p\250\131\012\157\028\1671\185W\200T\223f\236|\001\137\019\156%\1692\192L\210M\226m\242\127\r\145\015\164,\179>\192X\217g\235h\244\134\008\152)\170\017\181E\195W\231c\242~\004\152\017\159\"\177?\195L\192U\231w\249\133\007\148\024\166:\181A\201S\201k\238{\000\170\029\155/\173;\191j\222Z\020l\247n\007\144\025\182:\180-\198D\216t\248s\237\133\005\132 \1732\188P\205P\223c\228z\003\140\000\176'\1659\215d\212V\230b\238\129$\147\029\165'\160@\221R\205d\234n\255\138\017\147:\172\"\190_\208R\248k\244}\001\148\024\166*\167 \197\\\215{\233g\213\132\023\150\n\1351\166C\193H\222I\240x\002\130\n\157?\175%\193M\204\\\228n\254\160\t\140\027\187-\189\030\200Q\218d\206u\253\135\004\186\"\1854\159F\218v\225K\243i*\1424\160$\150;\234M\215_\225T\250\163\012\177\030\160\022\185K\203]\250f\202x'\138\024\180%\1747\199`\210S\228y\220\127\026\1453\1639\155>\239P\204M\235^\253\139$\152\015\1702\188L\226W\199i\217{\017\163\022\140(\164\021\195`\213S\024\143\215\130\n\148\011\139/\179A\196S\201n\238Z\000\140\239e\"R\201",
}


local _ou = {
    { 0, 5, 0, 0 },
    { 2, 21, 0, 0 },
    { 0, 57, 1, 0 },
    { 3, 12, 1, 0 },
    { 1, 18, 2, 0 },
    { 1, 23, 2, 0 },
    { 0, 22, 3, 0 },
    { 1, 48, 1, 0 },
}


local function _ip(_bc, _tu, _rb)
    local lo = _xk(_bc, _tu, _rb)
    local hi = _xk(_bc, _tu + 1, _rb)
    return lo + hi * 256
end


local function _wu(_bc, _tu, _rb)
    local v = _ip(_bc, _tu, _rb)
    if v >= 32768 then v = v - 65536 end
    return v
end

local _bm
_bm = function(_no, _qj, _ll, _yx)
    local _bc = _qc[_no + 1]
    local _pn = _gf[_no + 1]
    local _uf = _ou[_no + 1]
    local _jm = _uf[1]
    local _ne = _uf[2]
    local _dt = _uf[4] == 1
    _yx = _yx or {}
    local _jv = _tp(_ne + 1)
    for _te = 1, _jm do
        _jv[_te - 1] = _qj[_te]
    end
    local _hf
    if _dt then
        local _vd = _ll - _jm
        if _vd < 0 then _vd = 0 end
        _hf = {}
        for _te = 1, _vd do
            _hf[_te] = _qj[_jm + _te]
        end
        _hf.n = _vd
    end
    local _ez = 1
    local _lb = #_bc
    while _ez <= _lb do
        local _zp = _xk(_bc, _ez, _no)
        _ez = _ez + 1
        if _zp == 29 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = nil
        elseif _zp == 10 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = true
        elseif _zp == 32 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = false
        elseif _zp == 7 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _qr(_pn, _py + 1, _no)
        elseif _zp == 21 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py]
        elseif _zp == 22 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] + _jv[_mt]
        elseif _zp == 3 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] - _jv[_mt]
        elseif _zp == 12 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] * _jv[_mt]
        elseif _zp == 23 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] / _jv[_mt]
        elseif _zp == 33 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] % _jv[_mt]
        elseif _zp == 30 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] ^ _jv[_mt]
        elseif _zp == 9 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _yd(_jv[_py]) .. _yd(_jv[_mt])
        elseif _zp == 28 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] < _jv[_mt]
        elseif _zp == 35 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] <= _jv[_mt]
        elseif _zp == 24 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py] == _jv[_mt]
        elseif _zp == 19 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = not _jv[_py]
        elseif _zp == 1 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = -_jv[_py]
        elseif _zp == 8 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = #_jv[_py]
        elseif _zp == 11 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _lp[_qr(_pn, _py + 1, _no)]
        elseif _zp == 18 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            _lp[_qr(_pn, _wz + 1, _no)] = _jv[_py]
        elseif _zp == 13 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _nz = _wu(_bc, _ez, _no); _ez = _ez + 2
            local _gg = _jv[_py]
            local _bj = {}
            for _te = 1, _nz do
                _bj[_te] = _jv[_py + _te]
            end
            local result = _gg(_fx(_bj, 1, _nz))
            if _wz ~= 0xFFFF then
                _jv[_wz] = result
            end
        elseif _zp == 6 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            if _wz == 0xFFFF then
                return nil
            else
                return _jv[_wz]
            end
        elseif _zp == 5 then
            local _ql = _wu(_bc, _ez, _no); _ez = _ez + 2
            _ez = _ez + _ql
        elseif _zp == 16 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _ql = _wu(_bc, _ez, _no); _ez = _ez + 2
            if _jv[_wz] then
                _ez = _ez + _ql
            end
        elseif _zp == 34 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _ql = _wu(_bc, _ez, _no); _ez = _ez + 2
            if not _jv[_wz] then
                _ez = _ez + _ql
            end
        elseif _zp == 25 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _rb = _ip(_bc, _ez, _no); _ez = _ez + 2
            
            _ez = _ez + 2
            local _mg = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _zu = {}
            for _qy = 1, _mg do
                local _tf = _xk(_bc, _ez, _no); _ez = _ez + 1
                local _qf = _ip(_bc, _ez, _no); _ez = _ez + 2
                if _tf == 0 then
                    
                    _zu[_qy] = { _t = _jv, _k = _qf }
                else
                    
                    _zu[_qy] = _yx[_qf + 1]
                end
            end
            local _cr = _zu
            _jv[_wz] = function(...)
                local _rs = { ... }
                local _nz = _ec("#", ...)
                return _bm(_rb, _rs, _nz, _cr)
            end
        elseif _zp == 15 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = {}
        elseif _zp == 20 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _jv[_py][_jv[_mt]]
        elseif _zp == 31 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _mt = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz][_jv[_py]] = _jv[_mt]
        elseif _zp == 2 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _nt = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _bn = _yx[_nt + 1]
            _jv[_wz] = _bn._t[_bn._k]
        elseif _zp == 14 then
            local _nt = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _bn = _yx[_nt + 1]
            _bn._t[_bn._k] = _jv[_py]
        elseif _zp == 27 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _py = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _nz = _wu(_bc, _ez, _no); _ez = _ez + 2
            local _xn = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _qh = _wu(_bc, _ez, _no); _ez = _ez + 2
            local _gg = _jv[_py]
            local _bj = {}
            local _ms = _nz
            for _te = 1, _nz do
                _bj[_te] = _jv[_py + _te]
            end
            if _xn ~= 0xFFFF then
                local _fd = _jv[_xn]
                local _hb = _fd.n or 0
                for _te = 1, _hb do
                    _bj[_ms + 1] = _fd[_te]
                    _ms = _ms + 1
                end
            end
            if _qh == 0 then
                _gg(_fx(_bj, 1, _ms))
            elseif _qh == 1 then
                local _aa = _gg(_fx(_bj, 1, _ms))
                if _wz ~= 0xFFFF then
                    _jv[_wz] = _aa
                end
            else
                local _ve = _pg(_gg(_fx(_bj, 1, _ms)))
                _jv[_wz] = _ve
            end
        elseif _zp == 4 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _xn = _ip(_bc, _ez, _no); _ez = _ez + 2
            _ez = _ez + 2  
            local _qu = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _jl = {}
            local _li = 0
            for _ = 1, _qu do
                local _aa = _ip(_bc, _ez, _no); _ez = _ez + 2
                _li = _li + 1
                _jl[_li] = _jv[_aa]
            end
            if _xn ~= 0xFFFF then
                local _fd = _jv[_xn]
                local _hb = _fd.n or 0
                for _te = 1, _hb do
                    _li = _li + 1
                    _jl[_li] = _fd[_te]
                end
            end
            _jl.n = _li
            _jv[_wz] = _jl
        elseif _zp == 26 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            _jv[_wz] = _hf
        elseif _zp == 17 then
            local _wz = _ip(_bc, _ez, _no); _ez = _ez + 2
            local _jl = _jv[_wz]
            return _fx(_jl, 1, _jl.n)
        else
            _zx("luau-obf: unknown opcode " .. _yd(_zp))
        end
    end
    return nil
end


local _zb = { ... }
local _gv = _ec("#", ...)
return _bm(0, _zb, _gv, {})