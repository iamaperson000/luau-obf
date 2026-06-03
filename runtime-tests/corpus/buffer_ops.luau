local b = buffer.create(8)
print(buffer.len(b))  -- 8

buffer.writeu8(b, 0, 0xAB)
buffer.writeu8(b, 1, 0xCD)
print(buffer.readu8(b, 0))  -- 171
print(buffer.readu8(b, 1))  -- 205

-- Wider writes/reads.
buffer.writeu16(b, 2, 0xBEEF)
print(buffer.readu16(b, 2))  -- 48879
buffer.writeu32(b, 4, 0xDEADBEEF)
print(buffer.readu32(b, 4))  -- 3735928559

-- fromstring / tostring round-trip.
local s = "hello!"
local b2 = buffer.fromstring(s)
print(buffer.len(b2))      -- 6
print(buffer.tostring(b2)) -- "hello!"

-- buffer.copy: move bytes between buffers.
local src = buffer.fromstring("WXYZ")
local dst = buffer.create(8)
buffer.copy(dst, 2, src, 0, 4)
print(buffer.readu8(dst, 2))  -- W (0x57 = 87)
print(buffer.readu8(dst, 5))  -- Z (0x5A = 90)
