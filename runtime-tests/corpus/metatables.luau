-- __index as a table (inheritance).
local Animal = { kind = "animal" }
function Animal:describe() return self.name .. " is " .. self.kind end
local dog = setmetatable({ name = "Rex" }, { __index = Animal })
print(dog:describe())  -- "Rex is animal"

-- __index as a function (computed).
local computed = setmetatable({}, {
    __index = function(_, k) return "computed:" .. k end,
})
print(computed.anything)   -- "computed:anything"
print(computed.somethingElse)  -- "computed:somethingElse"

-- Nested __index chain.
local base = { from = "base" }
local mid = setmetatable({}, { __index = base })
local top = setmetatable({}, { __index = mid })
print(top.from)  -- "base"

-- __newindex as a function: log writes, defer to rawset.
local writes = {}
local logged = setmetatable({}, {
    __newindex = function(t, k, v)
        writes[#writes + 1] = k .. "=" .. tostring(v)
        rawset(t, k, v)
    end,
})
logged.a = 1
logged.b = 2
print(writes[1])  -- "a=1"
print(writes[2])  -- "b=2"
print(logged.a)   -- 1 (rawset stored it)

-- __newindex as a table: redirects writes.
local backing = {}
local redirected = setmetatable({}, { __newindex = backing })
redirected.x = 42
print(rawget(redirected, "x"))  -- nil (write went to backing, not redirected itself)
print(backing.x)  -- 42

-- __call: a "callable" table.
local callable = setmetatable({}, {
    __call = function(_self, a, b) return a * 10 + b end,
})
print(callable(3, 4))  -- 34

-- Arithmetic metamethods on a record-like wrapper.
local Vec2 = {}
Vec2.__index = Vec2
function Vec2.__add(a, b) return setmetatable({ x = a.x + b.x, y = a.y + b.y }, Vec2) end
function Vec2.__tostring(v) return "(" .. v.x .. "," .. v.y .. ")" end
local function new(x, y) return setmetatable({ x = x, y = y }, Vec2) end
local sum = new(1, 2) + new(3, 4)
print(tostring(sum))  -- "(4,6)"

-- rawget / rawset bypass __index / __newindex.
local guarded = setmetatable({}, {
    __index = function() return "guarded" end,
    __newindex = function() error("blocked") end,
})
rawset(guarded, "stored", "ok")
print(rawget(guarded, "stored"))  -- "ok"
print(rawget(guarded, "missing"))  -- nil (raw read does NOT trigger __index)
print(guarded.missing)             -- "guarded" (real read DOES trigger __index)
