---@type VehicleData
local car = { plate = 'ABC' }
Vehicle = GetVehiclePedIsIn(GetPlayerPed(1), false)
print(car, Vehicle)
