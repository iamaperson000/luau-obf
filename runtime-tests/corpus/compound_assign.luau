-- Scalar locals.
local a = 1
a += 5
print(a)  -- 6
a -= 2
print(a)  -- 4
a *= 3
print(a)  -- 12
a /= 4
print(a)  -- 3
a %= 2
print(a)  -- 1
a = 2
a ^= 5
print(a)  -- 32

-- Concat compound on a string.
local s = "hello"
s ..= " world"
print(s)  -- hello world

-- Compound assign through a table field (dot form).
local t = { count = 10 }
t.count += 5
print(t.count)  -- 15

-- Bracket form.
t["count"] *= 2
print(t.count)  -- 30

-- Indexed compound assign that mutates a deeply-nested field.
local obj = { inner = { x = 100 } }
obj.inner.x -= 25
print(obj.inner.x)  -- 75

-- Compound assign through a global.
g_counter = 0
g_counter += 7
g_counter += 8
print(g_counter)  -- 15

-- Compound assign on an upvalue inside a closure.
local outer = 100
local function bump() outer += 1 end
bump(); bump(); bump()
print(outer)  -- 103
