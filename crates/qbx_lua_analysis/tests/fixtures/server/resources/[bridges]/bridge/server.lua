local framework = GetResourceKvpString('framework')

print(exports.core:Ping('started earlier by server.cfg'))

if framework == 'ghost' then
    exports.ghost_framework:Init()
end

exports.ghost_inventory:AddItem(1, 'water')

print(exports.vault:Open(1), exports.vault:HiddenInEncryptedCode())
print(exports.core:Missing())

local ok, object = pcall(function() return exports.maybe_installed:GetObject() end)
print(ok, object)

if object then
    exports.behind_a_flag:Notify('x')
end
