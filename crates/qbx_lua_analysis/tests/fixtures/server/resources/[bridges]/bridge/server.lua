local framework = GetResourceKvpString('framework')

print(exports.core:Ping('started earlier by server.cfg'))

if framework == 'ghost' then
    exports.ghost_framework:Init()
end

exports.ghost_inventory:AddItem(1, 'water')
