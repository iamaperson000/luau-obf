local point = {x = 1, y = 2}
function point:magnitude()
    return self.x * self.x + self.y * self.y
end
function point:scale(k)
    self.x = self.x * k
    self.y = self.y * k
end
print(point:magnitude())
point:scale(3)
print(point.x)
print(point.y)
print(point:magnitude())

local counter = {n = 0}
function counter.inc(self)
    self.n = self.n + 1
end
counter.inc(counter)
counter.inc(counter)
counter.inc(counter)
print(counter.n)
