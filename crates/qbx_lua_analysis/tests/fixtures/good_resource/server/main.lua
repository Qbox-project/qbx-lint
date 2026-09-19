local function getLicense(src)
    return GetPlayerIdentifierByType(src, 'license2') or GetPlayerIdentifierByType(src, 'license')
end

RegisterNetEvent('good:server', function(coords)
    local src = source
    local license = getLicense(src)
    local rows = MySQL.query.await('SELECT 1 FROM players WHERE license = ?', { license })
    Wait(0)
    TriggerClientEvent('good:client', src, SharedHelper(rows), coords)
end)

lib.callback.register('good:cb', function(source)
    return exports.qbx_core:GetPlayer(source)?.PlayerData
end)

AddEventHandler('playerDropped', function(reason)
    print(('%s left: %s'):format(GetPlayerName(source), reason))
end)
