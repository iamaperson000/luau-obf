local sum = 0
for i = 1, 100 do
    if i > 10 then break end
    sum = sum + i
end
print(sum)

local found = -1
local i = 1
while true do
    if i * i > 50 then
        found = i
        break
    end
    i = i + 1
end
print(found)

local j = 0
repeat
    j = j + 1
    if j >= 7 then break end
until j >= 100
print(j)
