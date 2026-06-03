print(bit32.band(0xFF, 0x0F))   -- 15
print(bit32.bor(0xF0, 0x0F))    -- 255
print(bit32.bxor(0xAA, 0xFF))   -- 85
print(bit32.bnot(0))            -- 4294967295
print(bit32.lshift(1, 4))       -- 16
print(bit32.rshift(256, 4))     -- 16
print(bit32.arshift(-16, 1))    -- 2147483640 (signed-shift on 32-bit u32 reinterpretation)
print(bit32.extract(0xF0F0, 4, 8))  -- 15 (extract 8 bits starting at bit 4)
print(bit32.replace(0xF0F0, 0x0F, 0, 4))  -- 0xF0FF

-- Combined: count parity bit pattern.
local function popcount(x)
    local n = 0
    while x ~= 0 do
        n = n + bit32.band(x, 1)
        x = bit32.rshift(x, 1)
    end
    return n
end
print(popcount(0x12345678))  -- bits set in 0x12345678
