-- The bare-minimum metatable test: __index falling through to a parent table.
local parent = { kind = "parent" }
local child = setmetatable({}, { __index = parent })
print(child.kind)  -- "parent" — only works if __index dispatch is reached
print(getmetatable(child).__index == parent)  -- true
