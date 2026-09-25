local libState = GetResourceState('lib')
local hasLib = libState == 'started' or libState == 'starting'
local resourceToLoadFrom = hasLib and 'lib' or GetCurrentResourceName()
local fileNamePrefix = hasLib and '' or 'lib/'

---@param files string[]
local function LoadFiles(files)
    for i = 1, #files do
        local fileName = fileNamePrefix .. files[i]

        Citizen.CreateThreadNow(function()
            local fileContent = LoadResourceFile(resourceToLoadFrom, fileName)

            if not fileContent then
                return
            end

            local loadFunction, errorMessage = load(fileContent, '@@lib/' .. fileName)

            if loadFunction then
                local success, result = pcall(loadFunction)

                if not success then
                    print("^1[ERROR]^7: Failed to load file '" .. fileName .. "': " .. result)
                end
            else
                print("^1[ERROR]^7: Failed to load file '" .. fileName .. "': " .. errorMessage)
            end
        end)
    end
end

if not IsDuplicityVersion() then
    LoadFiles({ 'client/client.lua' })
end
