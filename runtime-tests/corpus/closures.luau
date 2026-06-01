-- Basic capture: function reads outer local.
local x = 42
local read_x = function() return x end
print(read_x())

-- Mutation through upvalue.
local n = 0
local bump = function() n = n + 1 end
bump()
bump()
bump()
print(n)

-- Two closures sharing the same upvalue.
local shared = 100
local add = function(k) shared = shared + k end
local get = function() return shared end
add(5)
add(7)
print(get())
