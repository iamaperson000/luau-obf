


local _ni = _ENV or getfenv(1)
local _pm = select
local _ip = error
local _pd = pcall
local _np = tostring
local _bj = type
local _gg = string.byte
local _ez = string.sub
local _rw = string.char
local _pj = table.unpack or unpack
local _pa = function(_gv) local _tc = {} for _pk=1,_gv do _tc[_pk] = false end return _tc end
local _sm = math.floor
local _dw = bit32.bxor
local _ym = table.concat
local _da = table.pack or function(...)
    local _tc = {...}
    _tc.n = _pm("#", ...)
    return _tc
end

local _nv = { 180, 89, 71, 98, 17, 254, 58, 149, 145, 36, 69, 185, 156, 236, 126, 211, 221, 101, 186, 213, 174, 140, 58, 195, 203, 137, 227, 166, 241, 58, 229, 135 }
local _oq = { 71, 17, 196, 115, 31, 86, 107, 53, 204, 55, 114, 176, 2, 227, 1, 225, 135, 179, 127, 55, 11, 107, 198, 175, 32, 161, 154, 164, 103, 105, 153, 107 }

local _wq = 3716411432
local _us = 2478407753

local function _um(_sn, _gj, _pc)
    local _zf = _gg(_sn, _gj)
    local mixed = ((_gj * _us) + _wq + (_pc * 2654435761)) % 4294967296
    return _dw(_zf, mixed % 256)
end

local function _zr(enc, salt)
    local _rr = {}
    local salt_term = (salt * 7919) % 4294967296
    for _pk = 1, #enc do
        local _zf = _gg(enc, _pk)
        local _sr = ((_pk - 1) % 32) + 1
        local _ll = ((_pk - 1) + salt_term) % 256
        _rr[_pk] = _rw(_dw(_dw(_dw(_zf, _nv[_sr]), _ll), _oq[_sr]))
    end
    return _ym(_rr)
end

local function _zd(_mn, bytes) return { _e = bytes, _t = _mn } end

local function _vj(_fl, _sr, _ih)
    local _ts = _fl[_sr]
    if type(_ts) == "table" and _ts._t then
        local _cr = _zr(_ts._e, _ih)
        local _mn = _ts._t
        if _mn == 0 then
            _ts = _cr
        elseif _mn == 1 then
            _ts = string.unpack("<d", _cr)
        elseif _mn == 2 then
            _ts = _gg(_cr, 1) ~= 0
        else
            _ts = nil
        end
        _fl[_sr] = _ts
    end
    return _ts
end


local _fm = {
    {  },
    { _zd(0, "o\208\029\148\1624\205%\222\132\188\138"), _zd(0, "o\205\031\142\156.\221"), _zd(0, "s\207\028\134\143"), _zd(0, "k\209\006\139\153.\197!"), _zd(0, "x\221\002\140\1425\208") },
    { _zd(0, "l\251\n\147\137"), _zd(1, "-\151c\240\236K\140\005"), _zd(0, "^\255\012\135\179#\2206\207\155\173\153"), _zd(1, "-\151c\240\236K\204\005"), _zd(1, "-\151c\240\236K\181E"), _zd(0, "]\229\n\158\152"), _zd(1, "-\151c\240\236K\252\005"), _zd(1, "-\151c\240\236KEz"), _zd(1, "-\151c\240\236\011:\005"), _zd(0, "Z\254\023\152\1369\2122"), _zd(1, "-\151c\240\236K\181\005"), _zd(0, "o\248\001"), _zd(1, "-\151c\240\236K\139\005"), _zd(1, "-\151c\240\236K\220\005"), _zd(0, "I\242\019\159\159\"\193"), _zd(1, "-\151c\240\236K\189\005"), _zd(0, "^\226\014\157\1419\204"), _zd(0, "\127\242\000\149\130?\149-\210\135\171\143\006\157\169"), _zd(1, "-\151c\240\236K\236\005") },
    { _zd(1, ">\134L\193\223zrK"), _zd(0, "_\2248\164\173"), _zd(0, "_\235#\180\177\014"), _zd(0, "U\239\"\165") },
    { _zd(1, "O\245=\174\206i\147c"), _zd(2, "N"), _zd(0, "+\144M\193\189\000\231"), _zd(0, "+\144M\193\189\000\231C\244\163\130\186v\164\208\217\230txD\001_X\218"), _zd(2, "O") },
    { _zd(0, "1\138]\202\199~\137q\135\194\236\203\t\209\179\230\133\025"), _zd(2, "X"), _zd(0, "/\141Z\215\197j\129e"), _zd(2, "Y"), _zd(0, "/\141Z\215\197j\129e\143\203\162\210\\\196\178\168\131\015X,uTT\218A\154\217"), _zd(1, "X\228.\191\161\024\224\018") },
    { _zd(0, "S\243;"), _zd(0, "(\176|\227\229Y\133!"), _zd(0, "I\167g\226\227\030"), _zd(0, "I\251"), _zd(0, "\029\188l\248\226^\159f") },
    { _zd(0, "Z\230"), _zd(0, "T\226"), _zd(0, "\017\171f\249"), _zd(0, "\027\175g\232\237R"), _zd(0, "Z\2396\189\167"), _zd(0, "\014\173{\233\241O\176W"), _zd(0, "\027\164|\248\241"), _zd(0, "\n\176a\243\247"), _zd(1, "z\194\008\157\131&.\015"), _zd(0, "Z\226") },
}


local _dx = {
    "q\191\003L\149\204'p\184\002K\148\221&c\185\001J\147\206'n\181\000H\146\218$m\183\255D\146\218!l\185\250G\147\2178\148K\249F\143\2160\149L",
    " ~\180\253F\131\218!k\179\233F\142\219$i\177\251V\136\214\028h\177\250B\140\213\026g\188\255B\142\212\015a\175\252A\139\211\030e\174\245@\137\212\027h\165\2468\136\195\019c\169\245<\135\210\025b\169\244=\128\207\020k\170\250<\151\197\023f\169\241;\135\205\022_\168\2418\131\204\017^\171\2529\137\203\006P\166\2328\133\202\018\\\165\2347\140\199\018V\164\2489\127\206\001Z\167\236;q\199\000Y\170\2352l\198\012X\175\2293m\197\004W\166\2512z\196\003Y\159\2501v\195\nF\158\2310w\205\011G\157\232/~\213\nQ\156\235!w\212\tR\155\245\"v\174\247\174",
    "\211%e\175\247`\137\210\027d\171\247?\136\209\028a\172\231>\139\230\025b\171\248\n\134\206\024m\146\243>\133\212\020`\159\2429\132\193\018_\171\241.\134\204\016^\161\2469\146\2036Z\166\2358\135\202\031j\165\2337\153\193\018m\164\2376\1287\019Z\175\2185{\199\t\166]\2214}\198\007X\161\2345u\197\000W\130\2272\127\196\004V\153\2271|\195\000c\158\2370u\245\011_\157\252\208\135\247\nR\156\227\"w\206\tp\150\228)v\179\008W\148\227-u\1781P\148\226'C\189\008O\130\030\213E\188\004N\145\239){\187&]\150\219(~\186\005]\149\211'|\143\002[\148\209\017o\169\001S\129\220\019n\182\000\182m\217$z\165\255Z\145\218#l\181\248S\144\222\"I\161\253U\143\204!f\165\252P\142\2097i\184\251f\149\214\012h\166\250O\149\213\006g\164\227B\142\212\017P\175\226A\134\228\028s\174\251x\137\203\027~R\t\t\136\211\026e\183\2457\135\242\005b\175\244&\134\201\005a\166\2430\179\206\011`\165\197;\153\205\012\160W\199:\130\204\001@\167\2459\132\212\020M\166\205\024\129\206\019C\165\226\001\128\233\018B\133\237\000\127\200\017\165\\\2385r\241\016G\162\242\203\130\240\015X\161\2033|\197\026u\160\2362}\231\rG\159\228\007z\225\012Y\169\231\019y\216\244\171\171\230.x\199.S\158\229\012R\192\rR\191\228+P\191\007Q\150\213,P\190\011g\153\196+nB\249y\152\224*S\155\005N\151\230\001r\176\004K\191\223,q\1825L\178\222+G\185*K\152\229&F\184\027`\147\234%l\183\012b\146\241$k\154\255F\145\248\014l\158\254k\144\223\012k\188\253J\185\216\012j\191\203E\160\215:\150M\205D\140\214\025G\177\243C\174\229\030L\176\214B\141\229\029e\175\244w\138\227\028i\153\247q\137\200\228\155\155\246>\136\197(c\169\2458\180\208\tb\137\192=\173\207+a\166\197<\177\206\014U\169\196;\132\205\233\160\170\2416\181\204'^\190\015\198\180\203\020]\147\2398\129\219\236\163",
    "\135\193\022^\168\2099\131\204\021~\163\2409\130\219\017]\162\239>\135\202\019\\\174\2337\133\201\020[\177\2296y\193\017Y\163\226=~\206\016Y\162\237>}\196\015V\169\2349|\196\014Q\171\2333{\202\005V\148\2323z\205\015U\153\2318y\211\244\171",
    "4l\199\018Y\164\2344}\198\017Z\161\2343}\197\015U\160\2432}\199\rR\159\2385z\192\012B\155\231\207\134\194\011V\157\229/|\193\025V\156\197(w\192\tY\156\228+v\191\008V\154\227+u\158\015P\152\226-}\189\004O\184\235*s\188\t@\151\232)~\180\004D\150\2118q\186\003@\132\222-p\163\253\180\154\221%o\190\nJ\146\220\005b\183\000I\133\214$\146I\254H\147\218(l\185\254T\157\217!\195K\236\185p",
    "\2318x\195\nU\157\229+w\222\011R\155\228,v\190\nQ\128\227*v\190\006P\159\230+p\189\017J\152\030\213s\188\007N\148\224-r\168\001M\182\217(q\186\028K\149\216'p\185\003L\148\253&l\187\001Iz##f\183\001I\148\210$m\182\232B\145%\220m\181\252G\152\217+k\167\247F\175\211!j\179\231I\142\220 i\178\252D\141\218\031H\188\250B\140\211\016g\178\249b\132\212\029f\163\235A\135\211\016q\174\249@\133\199\027d\173\250)\136\222\026yS\n-\135\211\025d\187\244>\134\239\ta\170\243+\151\206\232\159\171\2429\132\221\022N\168\226(\131\207\189\161\182\015\198",
    "\151\246)q\187\002M\150\222(e\187\003H\149\254%p\185\002G\128\221'o\180\020J\145\220?m\183\020I\147\2192i\182\255H\146\218%i\181\254G\134\223\"o\180\248F\155\223!n\179\220M\142\214 e\166\251C\141\218\nh\185\250Y\133\213\ng\177\249T\129\212\027f\166\248G\129\211\031e\184\251@\131\210\016d\185\251?\140\209:m\172\247>\151\223\025l\171\248)\134\194\024m\191\2433\133\212\007`\189\242:\132\219\007_\164\241*\131\202\007^\165\240/\145\203\005]\180\239)\146\202\002\163Z",
    "I\161\218\"l\149\255G\144\2172i\180\252F\131\219!h\179\231A\142\212 i\178\253A\141\222\031c\183\250G\140\208\030k\183\249D\139\210\021f\167\248^\131\211\027e\166\247A\128\210\022d\171\252?\128\209\022d\172\255>\132\208\025n\160\244:\134\201\020a\162\2430\136\206\020`\165\252;\136\205\008P\168\250:\142\204\020Q\167\0079\162\219\020]\166\205)\129\218\019W\165\226%\128\216\018O\183\2371\127\206\005Z\170\236!k\199\021Y\174\1974h\198\003w\161\2253f\211\014y\160\2322m\211\rB\159\2541|\219\012T\158\241)y\213\011L\157\2425x\196\nU\135\229,w\226\021R\137\2286v\179&Q\128\227 Z\190\027P\131\255+Z\189\007O\142\255*j\188\024N\145\255)r\187\018m\150\193(n\186\023m\149\219'v\155\002H\148\255\005o\170\001h\147\208\011n\150\000E\189\219\007m\172\219H\191\218\"l\163\219G\176\217\006k\178\219F\139\2167M\179\217E\168\2154A\178\254D\139\255\031n\177\216i\140\199\030N\176\245l\139\252\029j\128\248k\138\2017e\128\247A\137\1967d\138\246\020\136\2214c\191\2452\168\2085b\177\011\194\168\207\025a\161\222<\142\206\025`\165\249;\169\205\021\162V\224\197|",
}


local function _kr(_sn, _ll, _pc)
    local lo = _um(_sn, _ll, _pc)
    local hi = _um(_sn, _ll + 1, _pc)
    return lo + hi * 256
end


local function _wb(_sn, _ll, _pc)
    local v = _kr(_sn, _ll, _pc)
    if v >= 32768 then v = v - 65536 end
    return v
end

local _gy
_gy = function(_ih, _uj, _tx, _mv)
    local _sn = _dx[_ih + 1]
    local _fl = _fm[_ih + 1]
    local _hg = _um(_sn, 1, _ih)
    local _fx = _um(_sn, 2, _ih) + _um(_sn, 3, _ih) * 256
    local _oh = _um(_sn, 5, _ih) == 1
    _mv = _mv or {}
    local _mi = _pa(_fx + 1)
    for _pk = 1, _hg do
        _mi[_pk - 1] = _uj[_pk]
    end
    local _wa
    if _oh then
        local _gp = _tx - _hg
        if _gp < 0 then _gp = 0 end
        _wa = {}
        for _pk = 1, _gp do
            _wa[_pk] = _uj[_hg + _pk]
        end
        _wa.n = _gp
    end
    local _gj = 6
    local _xt = #_sn
    while _gj <= _xt do
        local _ii = _um(_sn, _gj, _ih)
        _gj = _gj + 1
        if _ii == 9 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = nil
        elseif _ii == 5 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = true
        elseif _ii == 15 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = false
        elseif _ii == 6 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _vj(_fl, _zf + 1, _ih)
        elseif _ii == 12 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf]
        elseif _ii == 11 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] + _mi[_ts]
        elseif _ii == 27 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] - _mi[_ts]
        elseif _ii == 28 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] * _mi[_ts]
        elseif _ii == 24 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] / _mi[_ts]
        elseif _ii == 4 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] % _mi[_ts]
        elseif _ii == 35 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] ^ _mi[_ts]
        elseif _ii == 22 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _np(_mi[_zf]) .. _np(_mi[_ts])
        elseif _ii == 31 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] < _mi[_ts]
        elseif _ii == 30 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] <= _mi[_ts]
        elseif _ii == 29 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf] == _mi[_ts]
        elseif _ii == 13 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = not _mi[_zf]
        elseif _ii == 33 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = -_mi[_zf]
        elseif _ii == 16 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = #_mi[_zf]
        elseif _ii == 20 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _ni[_vj(_fl, _zf + 1, _ih)]
        elseif _ii == 2 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _ni[_vj(_fl, _qy + 1, _ih)] = _mi[_zf]
        elseif _ii == 26 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _gv = _wb(_sn, _gj, _ih); _gj = _gj + 2
            local _sb = _mi[_zf]
            local _gn = {}
            for _pk = 1, _gv do
                _gn[_pk] = _mi[_zf + _pk]
            end
            local result = _sb(_pj(_gn, 1, _gv))
            if _qy ~= 0xFFFF then
                _mi[_qy] = result
            end
        elseif _ii == 17 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            if _qy == 0xFFFF then
                return nil
            else
                return _mi[_qy]
            end
        elseif _ii == 3 then
            local _qe = _wb(_sn, _gj, _ih); _gj = _gj + 2
            _gj = _gj + _qe
        elseif _ii == 10 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _qe = _wb(_sn, _gj, _ih); _gj = _gj + 2
            if _mi[_qy] then
                _gj = _gj + _qe
            end
        elseif _ii == 1 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _qe = _wb(_sn, _gj, _ih); _gj = _gj + 2
            if not _mi[_qy] then
                _gj = _gj + _qe
            end
        elseif _ii == 18 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _pc = _kr(_sn, _gj, _ih); _gj = _gj + 2
            
            _gj = _gj + 2
            local _tz = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _cn = {}
            for _py = 1, _tz do
                local _nn = _um(_sn, _gj, _ih); _gj = _gj + 1
                local _nw = _kr(_sn, _gj, _ih); _gj = _gj + 2
                if _nn == 0 then
                    
                    _cn[_py] = { _t = _mi, _k = _nw }
                else
                    
                    _cn[_py] = _mv[_nw + 1]
                end
            end
            local _er = _cn
            _mi[_qy] = function(...)
                local _yl = { ... }
                local _gv = _pm("#", ...)
                return _gy(_pc, _yl, _gv, _er)
            end
        elseif _ii == 21 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = {}
        elseif _ii == 34 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _mi[_zf][_mi[_ts]]
        elseif _ii == 14 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _ts = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy][_mi[_zf]] = _mi[_ts]
        elseif _ii == 32 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _sr = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _jq = _mv[_sr + 1]
            _mi[_qy] = _jq._t[_jq._k]
        elseif _ii == 7 then
            local _sr = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _jq = _mv[_sr + 1]
            _jq._t[_jq._k] = _mi[_zf]
        elseif _ii == 25 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _zf = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _gv = _wb(_sn, _gj, _ih); _gj = _gj + 2
            local _rg = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _vm = _wb(_sn, _gj, _ih); _gj = _gj + 2
            local _sb = _mi[_zf]
            local _gn = {}
            local _yh = _gv
            for _pk = 1, _gv do
                _gn[_pk] = _mi[_zf + _pk]
            end
            if _rg ~= 0xFFFF then
                local _ec = _mi[_rg]
                local _lt = _ec.n or 0
                for _pk = 1, _lt do
                    _gn[_yh + 1] = _ec[_pk]
                    _yh = _yh + 1
                end
            end
            if _vm == 0 then
                _sb(_pj(_gn, 1, _yh))
            elseif _vm == 1 then
                local _ae = _sb(_pj(_gn, 1, _yh))
                if _qy ~= 0xFFFF then
                    _mi[_qy] = _ae
                end
            else
                local _cj = _da(_sb(_pj(_gn, 1, _yh)))
                _mi[_qy] = _cj
            end
        elseif _ii == 23 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _rg = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _gj = _gj + 2  
            local _hy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _za = {}
            local _cy = 0
            for _ = 1, _hy do
                local _ae = _kr(_sn, _gj, _ih); _gj = _gj + 2
                _cy = _cy + 1
                _za[_cy] = _mi[_ae]
            end
            if _rg ~= 0xFFFF then
                local _ec = _mi[_rg]
                local _lt = _ec.n or 0
                for _pk = 1, _lt do
                    _cy = _cy + 1
                    _za[_cy] = _ec[_pk]
                end
            end
            _za.n = _cy
            _mi[_qy] = _za
        elseif _ii == 8 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            _mi[_qy] = _wa
        elseif _ii == 19 then
            local _qy = _kr(_sn, _gj, _ih); _gj = _gj + 2
            local _za = _mi[_qy]
            return _pj(_za, 1, _za.n)
        else
            _ip("luau-obf: unknown opcode " .. _np(_ii))
        end
    end
    return nil
end


local _kd = { ... }
local _vp = _pm("#", ...)
return _gy(0, _kd, _vp, {})