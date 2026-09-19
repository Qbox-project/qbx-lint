SharedConfig = {
    models = { `adder`, `zentorno` },
    .enabled,
}

function SharedHelper(value)
    return value?.nested?.field
end
