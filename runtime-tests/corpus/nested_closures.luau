-- Triple-nested closure threading an upvalue.
local function outer()
    local x = 10
    local function middle()
        local function inner()
            return x
        end
        return inner
    end
    return middle
end

local m = outer()
local i = m()
print(i())

-- Inner closure can mutate the outermost local.
local function bumper()
    local n = 0
    local function step()
        local function actual()
            n = n + 1
            return n
        end
        return actual
    end
    return step
end

local s = bumper()
local a1 = s()
local a2 = s()
print(a1())
print(a2())
print(a1())
