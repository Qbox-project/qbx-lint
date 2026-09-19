---@meta

---@class CitizenLib
Citizen = {}

---Suspends the current scheduler thread for at least `msec` milliseconds; 0 resumes on the next tick. Only valid inside a thread created by the scheduler.
---@param msec integer
function Citizen.Wait(msec) end

---Queues `handler` as a new scheduler thread (coroutine) that starts on the next tick.
---@param handler fun()
function Citizen.CreateThread(handler) end

---Creates a scheduler thread and runs it immediately up to its first yield, instead of waiting for the next tick.
---@param handler fun()
function Citizen.CreateThreadNow(handler) end

---Runs `callback` once in a new thread after `msec` milliseconds and returns a timer id usable with `Citizen.ClearTimeout`.
---@param msec integer
---@param callback fun()
---@return integer timerId
function Citizen.SetTimeout(msec, callback) end

---Cancels a pending timer created by `Citizen.SetTimeout`.
---@param timerId integer
function Citizen.ClearTimeout(timerId) end

---Yields the current thread until the promise settles. Returns the resolved value, or raises the rejection value as an error.
---@param p promise
---@return any ...
function Citizen.Await(p) end

---Writes raw text to the console/log without appending a newline.
---@param text string
function Citizen.Trace(text) end

---Calls a game native by hash. Arguments may include the pointer and result marker values produced by the other `Citizen.*` helpers.
---@param hash integer|string
---@param ... any
---@return any ...
function Citizen.InvokeNative(hash, ...) end

---Returns a directly callable function for the native with the given hash, or nil when it cannot be resolved.
---@param hash integer|string
---@return function? native
---@nodiscard
function Citizen.GetNative(hash) end

---Returns the Lua source of the lazily loaded wrapper for the named native, or nil when the native is unknown.
---@param name string
---@return string? source
---@nodiscard
function Citizen.LoadNative(name) end

---Marker for an integer out-parameter when calling `Citizen.InvokeNative`.
---@return any marker
---@nodiscard
function Citizen.PointerValueInt() end

---Marker for a float out-parameter when calling `Citizen.InvokeNative`.
---@return any marker
---@nodiscard
function Citizen.PointerValueFloat() end

---Marker for a vector3 out-parameter when calling `Citizen.InvokeNative`.
---@return any marker
---@nodiscard
function Citizen.PointerValueVector() end

---Marker for an integer in/out parameter that is initialised with `value` before the native runs.
---@param value integer
---@return any marker
---@nodiscard
function Citizen.PointerValueIntInitialized(value) end

---Marker for a float in/out parameter that is initialised with `value` before the native runs.
---@param value number
---@return any marker
---@nodiscard
function Citizen.PointerValueFloatInitialized(value) end

---Marker telling `Citizen.InvokeNative` to return the native's result even though out-parameters are present.
---@return any marker
---@nodiscard
function Citizen.ReturnResultAnyway() end

---Marker requesting the native result as a 32-bit integer.
---@return any marker
---@nodiscard
function Citizen.ResultAsInteger() end

---Marker requesting the native result as a 64-bit integer.
---@return any marker
---@nodiscard
function Citizen.ResultAsLong() end

---Marker requesting the native result as a float.
---@return any marker
---@nodiscard
function Citizen.ResultAsFloat() end

---Marker requesting the native result as a string.
---@return any marker
---@nodiscard
function Citizen.ResultAsString() end

---Marker requesting the native result as a vector3.
---@return any marker
---@nodiscard
function Citizen.ResultAsVector() end

---Marker requesting the native result as a msgpack-serialised object that is decoded into a Lua value.
---@return any marker
---@nodiscard
function Citizen.ResultAsObject() end

---Runtime internal: marks the start of a scheduler boundary used to stitch stack traces across resources.
---@param boundaryId integer
---@param co? thread
function Citizen.SubmitBoundaryStart(boundaryId, co) end

---Runtime internal: marks the end of the current scheduler boundary.
---@param co? thread
function Citizen.SubmitBoundaryEnd(co) end

---Runtime internal: installs the function the host calls every tick. Used by the scheduler; resources should not call it.
---@param routine fun()
function Citizen.SetTickRoutine(routine) end

---Runtime internal: installs the function the host calls to deliver events. Used by the scheduler; resources should not call it.
---@param routine fun(eventName: string, eventPayload: string, eventSource: string)
function Citizen.SetEventRoutine(routine) end

---Alias of `Citizen.Wait`.
---@param msec integer
function Wait(msec) end

---Alias of `Citizen.CreateThread`.
---@param handler fun()
function CreateThread(handler) end

---Alias of `Citizen.SetTimeout`.
---@param msec integer
---@param callback fun()
---@return integer timerId
function SetTimeout(msec, callback) end

---Alias of `Citizen.ClearTimeout`.
---@param timerId integer
function ClearTimeout(timerId) end

---Handle returned by `AddEventHandler`; pass it to `RemoveEventHandler` to unregister the handler.
---@class EventHandlerData
---@field key string
---@field name integer
local EventHandlerData = {}

---While a network event handler runs: the server id of the player that triggered it (server side), or 65535 for server-sent events (client side).
---@type integer
source = 0

---Registers `handler` for the named event and returns a handle for `RemoveEventHandler`. Network events additionally need `RegisterNetEvent`.
---@param eventName string
---@param handler fun(...: any)
---@return EventHandlerData eventData
function AddEventHandler(eventName, handler) end

---Unregisters a handler previously added with `AddEventHandler` or `RegisterNetEvent`.
---@param eventData EventHandlerData
function RemoveEventHandler(eventData) end

---Marks the event as safe to receive over the network. When `handler` is given it is also registered and its handle returned.
---@param eventName string
---@param handler? fun(...: any)
---@return EventHandlerData? eventData
---@overload fun(eventName: string)
function RegisterNetEvent(eventName, handler) end

---Old name of `RegisterNetEvent`.
---@deprecated
---@param eventName string
---@param handler? fun(...: any)
---@return EventHandlerData? eventData
function RegisterServerEvent(eventName, handler) end

---Triggers a local event on this side (client or server), invoking every handler in every resource with the given arguments.
---@param eventName string
---@param ... any
function TriggerEvent(eventName, ...) end

---Cross-resource export registry. Call it as `exports('name', fn)` to publish a function, or index it by resource name to call one: `exports.resource:fn(...)` / `exports['resource']:fn(...)`.
---@class CitizenExports
---@overload fun(name: string, fn: function)
---@field [string] table<string, function>
exports = {}

---@class jsonlib
json = {}

---Sentinel value that represents JSON `null` inside Lua tables.
---@type any
json.null = {}

---Serialises a Lua value to a JSON string. The optional state table tweaks output (for example `indent = true`).
---@param value any
---@param state? table
---@return string encoded
---@nodiscard
function json.encode(value, state) end

---Parses a JSON string starting at `pos`. Returns the value, or nil with the failing position and an error message. `nullval` replaces JSON null.
---@param str string
---@param pos? integer
---@param nullval? any
---@return any value
---@return integer? nextPos
---@return string? err
---@nodiscard
function json.decode(str, pos, nullval) end

---@class msgpacklib
msgpack = {}

---Serialises the given values into one MessagePack byte string.
---@param ... any
---@return string packed
---@nodiscard
function msgpack.pack(...) end

---Serialises the arguments as a single MessagePack array, the wire format used for event and export arguments.
---@param ... any
---@return string packed
---@nodiscard
function msgpack.pack_args(...) end

---Decodes a MessagePack byte string and returns every value it contains.
---@param data string
---@return any ...
---@nodiscard
function msgpack.unpack(data) end

---Deferred/promise object. `state` is 0 pending, 1 resolving, 2 rejecting, 3 resolved, 4 rejected; `value` holds the settled value.
---@class promise
---@field state integer
---@field value any
promise = {}

---Creates a new pending promise.
---@return promise p
---@nodiscard
function promise.new() end

---Returns a promise that resolves with a list of all results once every given promise has resolved, or rejects as soon as one rejects.
---@param promises promise[]
---@return promise p
---@nodiscard
function promise.all(promises) end

---Returns a promise that settles the same way as the first of the given promises to settle.
---@param promises promise[]
---@return promise p
---@nodiscard
function promise.first(promises) end

---Runs `fn` over each item sequentially, where `fn` returns a promise, and resolves with the list of results.
---@param items any[]
---@param fn fun(item: any): promise
---@return promise p
---@nodiscard
function promise.map(items, fn) end

---Fulfils the promise with `value` and schedules its continuations.
---@param value? any
---@return promise self
function promise:resolve(value) end

---Rejects the promise with `err` and schedules its rejection handlers.
---@param err? any
---@return promise self
function promise:reject(err) end

---Attaches continuations and returns a new promise chained on their result.
---@param onFulfilled? fun(value: any): any
---@param onRejected? fun(err: any): any
---@return promise chained
function promise:next(onFulfilled, onRejected) end

---Two-component float vector value type.
---@class vector2
---@field x number
---@field y number
---@operator add(vector2): vector2
---@operator sub(vector2): vector2
---@operator mul(number): vector2
---@operator mul(vector2): vector2
---@operator div(number): vector2
---@operator div(vector2): vector2
---@operator unm: vector2
---@operator len: number

---Three-component float vector value type. `#v` gives its length (magnitude).
---@class vector3
---@field x number
---@field y number
---@field z number
---@field xy vector2
---@operator add(vector3): vector3
---@operator sub(vector3): vector3
---@operator mul(number): vector3
---@operator mul(vector3): vector3
---@operator div(number): vector3
---@operator div(vector3): vector3
---@operator unm: vector3
---@operator len: number

---Four-component float vector value type.
---@class vector4
---@field x number
---@field y number
---@field z number
---@field w number
---@field xy vector2
---@field xyz vector3
---@operator add(vector4): vector4
---@operator sub(vector4): vector4
---@operator mul(number): vector4
---@operator mul(vector4): vector4
---@operator div(number): vector4
---@operator div(vector4): vector4
---@operator unm: vector4
---@operator len: number

---Quaternion value type. Multiplying by a vector3 rotates that vector.
---@class quat
---@field x number
---@field y number
---@field z number
---@field w number
---@operator mul(quat): quat
---@operator mul(vector3): vector3
---@operator mul(number): quat
---@operator unm: quat
---@operator len: number

---Creates a vector2.
---@param x number
---@param y number
---@return vector2 v
---@nodiscard
function vector2(x, y) end

---Creates a vector3.
---@param x number
---@param y number
---@param z number
---@return vector3 v
---@nodiscard
function vector3(x, y, z) end

---Creates a vector4.
---@param x number
---@param y number
---@param z number
---@param w number
---@return vector4 v
---@nodiscard
function vector4(x, y, z, w) end

---Creates a quaternion. Note that the scalar part `w` comes first.
---@param w number
---@param x number
---@param y number
---@param z number
---@return quat q
---@nodiscard
function quat(w, x, y, z) end

---Short alias of `vector2`.
---@param x number
---@param y number
---@return vector2 v
---@nodiscard
function vec2(x, y) end

---Short alias of `vector3`.
---@param x number
---@param y number
---@param z number
---@return vector3 v
---@nodiscard
function vec3(x, y, z) end

---Short alias of `vector4`.
---@param x number
---@param y number
---@param z number
---@param w number
---@return vector4 v
---@nodiscard
function vec4(x, y, z, w) end

---Creates a vector whose dimension matches the number of components given (2, 3 or 4).
---@param ... number
---@return vector2|vector3|vector4 v
---@overload fun(x: number, y: number): vector2
---@overload fun(x: number, y: number, z: number): vector3
---@overload fun(x: number, y: number, z: number, w: number): vector4
---@nodiscard
function vec(...) end

---Returns the normalised (unit length) copy of a vector or quaternion.
---@generic T: vector2|vector3|vector4|quat
---@param v T
---@return T normalized
---@nodiscard
function norm(v) end

---Computes the Jenkins one-at-a-time hash of `str`, the same value game natives use for model and asset names.
---@param str string
---@return integer hash
---@nodiscard
function joaat(str) end

---Key/value store synchronised between server and clients. Read and write keys as plain fields; use `set` to control replication explicitly.
---@class StateBag
---@field [string] any
local StateBag = {}

---Writes `key`. When `replicated` is true the change is sent across the network (a client sends to the server, the server sends to clients).
---@param key string
---@param value any
---@param replicated? boolean
function StateBag:set(key, value, replicated) end

---State bag shared by the whole server. Writable on the server, read-only on clients.
---@type StateBag
GlobalState = {}

---@class EntityInterface
---@field state StateBag
local EntityInterface = {}

---@class PlayerInterface
---@field state StateBag
local PlayerInterface = {}

---Wraps an entity handle in an object whose `state` field is that entity's state bag.
---@param entity integer
---@return EntityInterface wrapper
---@nodiscard
function Entity(entity) end

---Wraps a player server id in an object whose `state` field is that player's state bag.
---@param playerSrc integer|string
---@return PlayerInterface wrapper
---@nodiscard
function Player(playerSrc) end
