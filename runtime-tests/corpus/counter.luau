local function make_counter()
    local n = 0
    local function inc()
        n = n + 1
        return n
    end
    return inc
end

local c1 = make_counter()
local c2 = make_counter()
print(c1())
print(c1())
print(c1())
print(c2())
print(c1())
print(c2())
