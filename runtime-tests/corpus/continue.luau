-- continue inside while
local i = 0
local sum = 0
while i < 10 do
    i = i + 1
    if i % 2 == 0 then continue end
    sum = sum + i
end
print(sum)  -- 1+3+5+7+9 = 25

-- continue inside numeric for
local total = 0
for j = 1, 10 do
    if j > 5 then continue end
    total = total + j
end
print(total)  -- 1+2+3+4+5 = 15

-- continue inside repeat-until: skip the body when k == 3.
local k = 0
local rsum = 0
repeat
    k = k + 1
    if k == 3 then continue end
    rsum = rsum + k
until k >= 5
print(rsum)  -- 1+2+4+5 = 12

-- continue inside generic for (ipairs)
local arr = {10, 20, 30, 40, 50}
local gsum = 0
for _, v in ipairs(arr) do
    if v > 30 then continue end
    gsum = gsum + v
end
print(gsum)  -- 10+20+30 = 60

-- Nested loops: continue applies to the INNERMOST loop only.
local outer_count = 0
local inner_count = 0
for a = 1, 3 do
    outer_count = outer_count + 1
    for b = 1, 3 do
        if b == 2 then continue end
        inner_count = inner_count + 1
    end
end
print(outer_count)  -- 3
print(inner_count)  -- 3 * 2 = 6
