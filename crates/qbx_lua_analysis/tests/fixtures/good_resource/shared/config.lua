SharedConfig = {
    models = { `adder`, `zentorno` },
    .enabled,
}

---@param value table
---@param fallback? string
function SharedHelper(value, fallback)
    return value?.nested?.field or fallback
end
