local v1 = vector.create(1, 2, 3)
local v2 = vector.create(4, 5, 6)

-- Component access.
print(v1.x)  -- 1
print(v1.y)  -- 2
print(v1.z)  -- 3

-- Component arithmetic.
local sum = v1 + v2
print(sum.x)  -- 5
print(sum.y)  -- 7
print(sum.z)  -- 9

local diff = v2 - v1
print(diff.x)  -- 3
print(diff.y)  -- 3
print(diff.z)  -- 3

-- Scalar multiplication if the host overloads it.
local scaled = v1 * 2
print(scaled.x)  -- 2
print(scaled.y)  -- 4
print(scaled.z)  -- 6

-- Identity round-trip through a function.
local function passthrough(v) return v end
local p = passthrough(v1)
print(p.x)  -- 1
print(p.z)  -- 3
