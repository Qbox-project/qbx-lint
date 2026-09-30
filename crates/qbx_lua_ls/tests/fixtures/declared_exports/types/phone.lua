---@class PhoneConfig
---@field Debug boolean

---@class PhoneCall
---@field id number

---@class PhoneExports
---@field GetConfig fun(self: PhoneExports): PhoneConfig # shared
---@field IsInCall fun(self: PhoneExports): boolean # client-side
---@field IsInCall fun(self: PhoneExports, source: number): boolean, number?, PhoneCall? # server-side
---@field [string] any

---@type PhoneExports
---@diagnostic disable: missing-fields
exports["phone"] = {}

---@class TabletExports
---@field Ring fun(self: TabletExports, times: number): boolean

---@type TabletExports
exports.tablet = {}
