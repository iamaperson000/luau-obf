local t = { a = 1, b = 2, c = 3 }
local sum = 0
for k, v in pairs(t) do
    sum = sum + v
end
print(sum)  -- 6 regardless of iteration order

local arr = { 10, 20, 30, 40 }
local total = 0
for i, v in ipairs(arr) do
    total = total + i * v
end
print(total)  -- 1*10 + 2*20 + 3*30 + 4*40 = 10 + 40 + 90 + 160 = 300

-- Custom iterator with the (iter, state, ctrl) protocol.
local function range_iter(state, ctrl)
    local nxt = ctrl + 1
    if nxt > state then return nil end
    return nxt, nxt * nxt
end

local function range(stop)
    return range_iter, stop, 0
end

local squares_sum = 0
for i, sq in range(5) do
    squares_sum = squares_sum + sq
end
print(squares_sum)  -- 1+4+9+16+25 = 55
