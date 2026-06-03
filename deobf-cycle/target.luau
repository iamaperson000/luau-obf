-- Mixed-algorithm program used as the adversarial deobfuscation target.
-- Exercises: tables, recursion, while/for loops, modulo, comparison,
-- math.floor, table.concat, string concatenation, nested control flow.
--
-- Note: rewritten to avoid `a, b = b, expr(a, b)` swap-style multi-assign,
-- which exposes a pre-existing MIR-lowering / VM bug unrelated to the
-- obfuscation passes. Each function uses explicit temporary variables.

local function sieve(n)
    local is_prime = {}
    for i = 2, n do is_prime[i] = true end
    for i = 2, n do
        if is_prime[i] then
            local j = i * i
            while j <= n do
                is_prime[j] = false
                j = j + i
            end
        end
    end
    local primes = {}
    for i = 2, n do
        if is_prime[i] then primes[#primes + 1] = i end
    end
    return primes
end

local function fact(n)
    if n <= 1 then return 1 end
    return n * fact(n - 1)
end

local function gcd(a, b)
    while b ~= 0 do
        local t = b
        b = a % b
        a = t
    end
    return a
end

local function fib(n)
    local a = 0
    local b = 1
    for _ = 1, n do
        local t = a + b
        a = b
        b = t
    end
    return a
end

local function digit_sum(n)
    local s = 0
    while n > 0 do
        s = s + (n % 10)
        n = math.floor(n / 10)
    end
    return s
end

print("primes <= 30: " .. table.concat(sieve(30), ","))
print("fact(10): " .. fact(10))
print("gcd(48, 18): " .. gcd(48, 18))
print("fib(15): " .. fib(15))
print("digit_sum(12345): " .. digit_sum(12345))
