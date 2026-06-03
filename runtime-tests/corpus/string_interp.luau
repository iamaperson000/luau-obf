local name = "world"
print(`hello, {name}`)  -- hello, world

local n = 42
print(`answer: {n}`)  -- answer: 42

-- Multiple expressions in one literal.
local a, b = 1, 2
print(`{a} + {b} = {a + b}`)  -- 1 + 2 = 3

-- Adjacent expressions with no literal between them.
print(`{a}{b}`)  -- 12

-- All-literal backtick — no interpolation.
print(`plain string`)  -- plain string

-- Empty backtick.
print(`<` .. `` .. `>`)  -- <>

-- nil and bool tostring.
print(`x = {nil}, y = {true}, z = {false}`)  -- x = nil, y = true, z = false

-- Nested table access inside an interpolation.
local t = { x = "inner" }
print(`got {t.x}`)  -- got inner

-- Escape sequences in the literal portion.
print(`line1\nline2`)  -- two-line print; matches across runs

-- A function call inside an interpolation.
local function pick() return "picked" end
print(`-> {pick()} <-`)  -- -> picked <-
