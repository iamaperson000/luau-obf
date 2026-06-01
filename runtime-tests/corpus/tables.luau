local t = {1, 2, 3}
print(t[1] + t[2] + t[3])
print(#t)
t[4] = 10
print(#t)
print(t[4])

local h = {x = 5, y = 7}
print(h.x + h.y)
h.z = h.x * h.y
print(h.z)

local mixed = {10, x = 20, 30}
print(mixed[1])
print(mixed[2])
print(mixed.x)

local k = "key"
local map = {[k] = 42}
print(map[k])
print(map["key"])
