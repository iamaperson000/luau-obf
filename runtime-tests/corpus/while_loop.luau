local i = 0
local total = 0
while i < 5 do
    total = total + i
    i = i + 1
end
print(total)

-- repeat .. until
local j = 0
local product = 1
repeat
    j = j + 1
    product = product * j
until j >= 4
print(product)
