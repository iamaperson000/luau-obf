function classify(n)
    if n < 0 then
        return "neg"
    elseif n == 0 then
        return "zero"
    else
        return "pos"
    end
end
print(classify(-1))
print(classify(0))
print(classify(1))
