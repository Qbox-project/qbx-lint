-- Outside any resource and not a `---@meta` file, so no resource sees its globals.
Loose = {}

---@return number
function Loose.value()
    return 1
end
