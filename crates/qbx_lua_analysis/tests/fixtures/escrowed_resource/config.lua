Config = {}
Config.Label = locale('title')
Config.OnUse = function(item)
    return EncryptedHelper(item)
end
