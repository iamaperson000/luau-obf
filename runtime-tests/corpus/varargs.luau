local function count(...)
    return select("#", ...)
end
print(count())
print(count(1))
print(count(1, 2, 3))

-- Forwarding varargs.
local function fwd(...)
    return count(...)
end
print(fwd(7, 8, 9, 10))

-- Collecting into a table.
local function pack_them(...)
    local t = {...}
    local s = 0
    for i = 1, #t do
        s = s + t[i]
    end
    return s
end
print(pack_them(1, 2, 3, 4, 5))  -- 15

-- `select(i, ...)` returns the i-th vararg and everything after.
-- We collect into a table so we can pick out individual positions —
-- paren-truncation (`(select(i, ...))` taking only the first value) is a
-- Plan 5+ refinement; for now keep all values reachable explicitly.
local function rest(i, ...)
    local vals = {select(i, ...)}
    return vals[1], vals[2]
end
local r1, r2 = rest(2, "x", "y", "z")
print(r1)  -- "y"
print(r2)  -- "z"
