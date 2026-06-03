-- A small bank simulation: closure-captured balance & history,
-- transactional API, compound assignment, string interpolation.

local function make_account(owner, opening_balance)
    local balance = opening_balance
    local history = {}

    local function record(kind, amount, after)
        history[#history + 1] = { kind = kind, amount = amount, after = after }
    end

    local function deposit(amount)
        if amount <= 0 then
            return false, "deposit must be positive"
        end
        balance += amount
        record("deposit", amount, balance)
        return true, balance
    end

    local function withdraw(amount)
        if amount <= 0 then
            return false, "withdrawal must be positive"
        end
        if amount > balance then
            return false, "insufficient funds"
        end
        balance -= amount
        record("withdraw", amount, balance)
        return true, balance
    end

    local function summary()
        return `Account {owner}: ${balance} ({#history} txns)`
    end

    local function show_history(limit)
        local n = #history
        local start = n - limit + 1
        if start < 1 then start = 1 end
        for i = start, n do
            local h = history[i]
            print(`  {i}. {h.kind} ${h.amount} -> ${h.after}`)
        end
    end

    return {
        deposit = deposit,
        withdraw = withdraw,
        summary = summary,
        show_history = show_history,
        owner = owner,
    }
end

local function run()
    local alice = make_account("Alice", 100)
    print(alice.summary())

    alice.deposit(50)
    alice.deposit(25)
    local ok, msg = alice.withdraw(200)
    print(ok, msg)
    alice.withdraw(30)

    print(alice.summary())
    print("Recent history:")
    alice.show_history(3)

    local bob = make_account("Bob", 0)
    bob.deposit(1000)
    bob.withdraw(400)
    print(bob.summary())
end

run()
