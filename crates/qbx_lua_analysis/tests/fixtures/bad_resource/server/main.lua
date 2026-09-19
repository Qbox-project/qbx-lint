RegisterNetEvent('bad:server', function()
    Wait(100)
    print(source)
    SetTimeout(500, function()
        DropPlayer(source, 'late')
    end)
    local ped = PlayerPedId()
    local a, b = 1, 2, 3
    a, b = 1
    return a, b, ped
end)

RegisterServerEvent('bad:legacy')

AddEventHandler('ok', function()
    local src = source
    Wait(0)
    print(src)
    -- qbx-lint: disable-next-line undefined-global
    print(suppressedGlobal)
    print(alsoSuppressed) ---@diagnostic disable-line: undefined-global
end)

local QBCore = exports['qb-core']:GetCoreObject()
print(QBCore)
