exports.phone = { GetConfig = function() return 'mocked' end, Fake = function() end }
local config = exports.phone:GetConfig()
print(config)
