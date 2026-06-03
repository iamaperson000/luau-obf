local function pair_of(a, b)
    return a, b
end

local x, y = pair_of(3, 7)
print(x)
print(y)

-- Spread call into multi-bind.
local function triple()
    return 10, 20, 30
end

local a, b, c = triple()
print(a)
print(b)
print(c)

-- `return f()` spreads f's results.
local function forward()
    return triple()
end

local p, q, r = forward()
print(p)
print(q)
print(r)

-- Multi-bind with non-spread RHS pads with nil.
local m, n = 1
print(m)
print(n)

-- Multi-assign through globals.
g1, g2 = pair_of(100, 200)
print(g1)
print(g2)
