function factorial(n)
    if n <= 1 then return 1 end
    return n * factorial(n - 1)
end
print(factorial(6))

function fib(n)
    if n < 2 then return n end
    return fib(n - 1) + fib(n - 2)
end
print(fib(10))
