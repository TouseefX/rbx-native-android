-- Roblox behaviour Loren's specs rely on that the editor's sandbox lacks.
-- The sandbox's instances have no `WaitForChild` and its RunService answers
-- as Studio's edit mode, while unit specs expect a running server. Instances
-- and signals are therefore seen through proxies that add what is missing,
-- and modules are loaded by a `require` that gives them the same view.

local rawTypeof = typeof
local rawGame = game
local rawInstance = Instance
local rawRequire = require
local baseEnv = getfenv(0)

local REAL = {}
local proxies: { [any]: any } = {}
local proxyMeta = {}
local wrap, unwrap

local WRAPPED = { Instance = true, RBXScriptSignal = true, RBXScriptConnection = true }

local function wrapAll(...: any): ...any
	local n = select("#", ...)
	local out = table.pack(...)
	for i = 1, n do
		out[i] = wrap(out[i])
	end
	return table.unpack(out, 1, n)
end

local function unwrapAll(...: any): ...any
	local n = select("#", ...)
	local out = table.pack(...)
	for i = 1, n do
		out[i] = unwrap(out[i])
	end
	return table.unpack(out, 1, n)
end

local callbacks = setmetatable({}, { __mode = "k" })

function unwrap(value: any): any
	if type(value) == "table" then
		local real = rawget(value, REAL)
		if real ~= nil then
			return real
		end
		return value
	elseif type(value) == "function" then
		local found = callbacks[value]
		if not found then
			found = function(...)
				return unwrapAll(value(wrapAll(...)))
			end
			callbacks[value] = found
		end
		return found
	end
	return value
end

local function wrapTable(value: { [any]: any }): { [any]: any }
	if getmetatable(value) ~= nil then
		return value
	end
	local copy = {}
	for k, v in value do
		copy[wrap(k)] = wrap(v)
	end
	return copy
end

function wrap(value: any): any
	local kind = rawTypeof(value)
	if WRAPPED[kind] then
		local found = proxies[value]
		if not found then
			found = setmetatable({ [REAL] = value }, proxyMeta)
			proxies[value] = found
		end
		return found
	elseif kind == "table" then
		return wrapTable(value)
	end
	return value
end

-- The host runs `time()` on a virtual clock that jumps over waits, while
-- os.clock is CPU time: deadlines measured with it would cost real seconds
-- and let virtual waits outrun them. It follows the virtual clock instead,
-- plus the CPU time spent, so busy loops that yield on os.clock still see
-- it move.
local rawClock = os.clock
local cpuStart = rawClock()
local function loren_clock(): number
	return time() + (rawClock() - cpuStart)
end
local osShim = setmetatable({ clock = loren_clock }, { __index = os })

local function waitForChild(self: any, name: string, timeout: number?): any
	local real = unwrap(self)
	local started = loren_clock()
	local warned = false
	while true do
		local found = real:FindFirstChild(name)
		if found then
			return wrap(found)
		end
		local waited = loren_clock() - started
		if timeout and waited >= timeout then
			return nil
		end
		if not timeout and not warned and waited >= 5 then
			warned = true
			warn(`Infinite yield possible on '{real:GetFullName()}:WaitForChild("{name}")'`)
		end
		task.wait()
	end
end

-- The sandbox has no frame loop, so RunService's frame events are signals
-- pumped by a thread of their own while anything listens to them. The
-- driver stops the pump once the specs are done, or connections a spec
-- never disconnects would keep the run alive until the runtime gives up.
local fakeKinds: { [any]: string } = setmetatable({}, { __mode = "k" }) :: any
local frameSignals = {}
local pumping = false
local framesStopped = false

local FakeConnection = {}
FakeConnection.__index = FakeConnection

function FakeConnection:Disconnect()
	if not self.Connected then
		return
	end
	self.Connected = false
	local list = self._signal._connections
	local at = table.find(list, self)
	if at then
		table.remove(list, at)
	end
end

local FakeSignal = {}
FakeSignal.__index = FakeSignal

local startPump

function FakeSignal:Connect(fn: (...any) -> ...any)
	local connection = setmetatable({ Connected = true, _signal = self, _fn = fn }, FakeConnection)
	fakeKinds[connection] = "RBXScriptConnection"
	table.insert(self._connections, connection)
	startPump()
	return connection
end

function FakeSignal:Once(fn: (...any) -> ...any)
	local connection
	connection = self:Connect(function(...)
		connection:Disconnect()
		fn(...)
	end)
	return connection
end

function FakeSignal:Wait(): ...any
	local result
	self:Once(function(...)
		result = table.pack(...)
	end)
	while not result do
		task.wait()
	end
	return table.unpack(result, 1, result.n)
end

local function frameSignal(): any
	local signal = setmetatable({ _connections = {} }, FakeSignal)
	fakeKinds[signal] = "RBXScriptSignal"
	table.insert(frameSignals, signal)
	return signal
end

local function fireFrame(signal: any, ...: any)
	for _, connection in table.clone(signal._connections) do
		if connection.Connected then
			task.spawn(connection._fn, ...)
		end
	end
end

local Stepped = frameSignal()
local Heartbeat = frameSignal()
local RenderStepped = frameSignal()

local function listening(): boolean
	for _, signal in frameSignals do
		if #signal._connections > 0 then
			return true
		end
	end
	return false
end

function startPump()
	if pumping or framesStopped then
		return
	end
	pumping = true
	task.spawn(function()
		local started = loren_clock()
		local last = started
		while not framesStopped and listening() do
			task.wait()
			local now = loren_clock()
			local dt = now - last
			last = now
			fireFrame(Stepped, now - started, dt)
			fireFrame(RenderStepped, dt)
			fireFrame(Heartbeat, dt)
		end
		pumping = false
	end)
end

local function stopFrames()
	framesStopped = true
end

local runService = {
	Heartbeat = Heartbeat,
	Stepped = Stepped,
	RenderStepped = RenderStepped,
	PreSimulation = Stepped,
	PostSimulation = Heartbeat,
	IsServer = function()
		return true
	end,
	IsClient = function()
		return false
	end,
	IsRunning = function()
		return true
	end,
	IsRunMode = function()
		return true
	end,
	IsStudio = function()
		return true
	end,
	IsEdit = function()
		return false
	end,
}

-- Nothing in the sandbox reports script errors to ScriptContext, but code
-- may still listen for them.
local scriptContextError = setmetatable({ _connections = {} }, FakeSignal)
fakeKinds[scriptContextError] = "RBXScriptSignal"

local function isRunService(real: any): boolean
	return rawTypeof(real) == "Instance" and real.ClassName == "RunService"
end

proxyMeta.__index = function(self, key)
	local real = rawget(self, REAL)
	if key == "WaitForChild" and rawTypeof(real) == "Instance" then
		return waitForChild
	end
	if type(key) == "string" and runService[key] and isRunService(real) then
		return runService[key]
	end
	if key == "Error" and rawTypeof(real) == "Instance" and real.ClassName == "ScriptContext" then
		return scriptContextError
	end
	local value = real[unwrap(key)]
	if type(value) == "function" then
		return function(this, ...)
			if rawequal(this, self) then
				return wrapAll(value(real, unwrapAll(...)))
			end
			return wrapAll(value(unwrap(this), unwrapAll(...)))
		end
	end
	return wrap(value)
end
proxyMeta.__newindex = function(self, key, value)
	rawget(self, REAL)[key] = unwrap(value)
end
proxyMeta.__tostring = function(self)
	return tostring(rawget(self, REAL))
end
proxyMeta.__eq = function(a, b)
	return rawget(a, REAL) == rawget(b, REAL)
end
-- Roblox locks an Instance's metatable; code that treats "has a metatable"
-- as "not plain data" must see the same answer through a proxy.
proxyMeta.__metatable = "The metatable is locked"

-- Proxies and fake signals stand in for userdata, and serializers branch on `type`.
local function loren_type(value: any): string
	if type(value) == "table" and (rawget(value, REAL) ~= nil or fakeKinds[value] ~= nil) then
		return "userdata"
	end
	return type(value)
end

-- Roblox runs threads deferred before a wait ends ahead of the waiting
-- thread; the sandbox wakes due timers first. Deferring the woken thread
-- once more lets that earlier deferred work finish before the wait returns.
local rawTask = task
local function loren_wait(seconds: number?): ...any
	local results = table.pack(rawTask.wait(seconds))
	rawTask.defer(coroutine.running())
	coroutine.yield()
	return table.unpack(results, 1, results.n)
end
-- Roblox's task.cancel closes the thread, so its status reads "dead".
local function loren_cancel(thread: thread)
	rawTask.cancel(thread)
	if coroutine.status(thread) == "suspended" then
		pcall(coroutine.close, thread)
	end
end
-- Roblox fires ScriptContext.Error for an uncaught error in a thread the
-- engine resumes, never for one coroutine.resume hands back to its caller.
-- Bodies run under xpcall in their own thread, so coroutine.running() is
-- unchanged, and the error is raised again so the thread dies the same way.
local rawCoroutine = coroutine
local resuming: { [thread]: number } = setmetatable({}, { __mode = "k" }) :: any

local function reportError(message: string, trace: string)
	for _, connection in table.clone(scriptContextError._connections) do
		if connection.Connected then
			rawTask.spawn(connection._fn, message, trace, nil)
		end
	end
end

local function guarded(f: (...any) -> ...any): (...any) -> ...any
	return function(...)
		local thread = rawCoroutine.running()
		local results = table.pack(xpcall(f, function(err)
			if (resuming[thread] or 0) == 0 then
				reportError(tostring(err), debug.traceback())
			end
			return err
		end, ...))
		if not results[1] then
			error(results[2], 0)
		end
		return table.unpack(results, 2, results.n)
	end
end

local function resumeTracked(co: thread, ...: any): ...any
	resuming[co] = (resuming[co] or 0) + 1
	local results = table.pack(rawCoroutine.resume(co, ...))
	resuming[co] -= 1
	return table.unpack(results, 1, results.n)
end

local coroutineShim = setmetatable({
	create = function(f)
		return rawCoroutine.create(guarded(f))
	end,
	resume = resumeTracked,
	wrap = function(f)
		local co = rawCoroutine.create(guarded(f))
		return function(...)
			local results = table.pack(resumeTracked(co, ...))
			if not results[1] then
				error(results[2], 0)
			end
			return table.unpack(results, 2, results.n)
		end
	end,
}, { __index = rawCoroutine })

local function scheduled(raw: (...any) -> any, first: number): (...any) -> any
	return function(...)
		local args = table.pack(...)
		if type(args[first]) == "function" then
			args[first] = guarded(args[first])
		end
		return raw(table.unpack(args, 1, args.n))
	end
end

local taskShim = setmetatable({
	wait = loren_wait,
	cancel = loren_cancel,
	spawn = scheduled(rawTask.spawn, 1),
	defer = scheduled(rawTask.defer, 1),
	delay = scheduled(rawTask.delay, 2),
}, { __index = rawTask })

-- The sandbox's Enum has no GetEnums; the host lists every engine Enum.
local rawEnum = Enum
local enumList = nil
local EnumShim = setmetatable({
	GetEnums = function()
		if not enumList then
			enumList = {}
			for _, name in __lorenEnumNames do
				local ok, found = pcall(function()
					return rawEnum[name]
				end)
				if ok and found ~= nil then
					table.insert(enumList, found)
				end
			end
		end
		return table.clone(enumList)
	end,
}, {
	__index = function(_, key)
		return rawEnum[key]
	end,
})

-- An Enum prints as its bare name in Roblox ("Material"), unlike its items.
local function loren_tostring(value: any): string
	if rawTypeof(value) == "Enum" then
		return (string.gsub(tostring(value), "^Enum%.", ""))
	end
	return tostring(value)
end

local function loren_typeof(value: any): string
	if type(value) == "table" then
		local real = rawget(value, REAL)
		if real ~= nil then
			return rawTypeof(real)
		end
		local fake = fakeKinds[value]
		if fake then
			return fake
		end
	end
	return rawTypeof(value)
end

local InstanceShim = setmetatable({
	new = function(class: string, parent: any?)
		return wrap(rawInstance.new(class, unwrap(parent)))
	end,
}, { __index = rawInstance })

local loaded: { [any]: { value: any } } = {}
local loading: { [any]: boolean } = {}
local makeEnv

local function loren_require(target: any): any
	local real = unwrap(target)
	if rawTypeof(real) ~= "Instance" then
		return rawRequire(real)
	end
	local done = loaded[real]
	if done then
		return done.value
	end
	if real.ClassName ~= "ModuleScript" then
		error("Attempted to call require with invalid argument(s).", 2)
	end
	if loading[real] then
		error("Requested module was required recursively: " .. real:GetFullName(), 2)
	end
	local chunk, err = loadstring(real.Source, "=" .. real:GetFullName())
	if not chunk then
		error(err, 0)
	end
	setfenv(chunk, makeEnv(wrap(real)))
	loading[real] = true
	local ok, result = pcall(chunk)
	loading[real] = nil
	if not ok then
		error(result, 0)
	end
	loaded[real] = { value = result }
	return result
end

-- Memory categories and profiler labels only matter to Studio's tooling.
local function noop() end
local debugShim = setmetatable({
	setmemorycategory = debug.setmemorycategory or noop,
	resetmemorycategory = debug.resetmemorycategory or noop,
	profilebegin = debug.profilebegin or noop,
	profileend = debug.profileend or noop,
}, { __index = debug })

local gameProxy = wrap(rawGame)

function makeEnv(scriptProxy: any): { [string]: any }
	return setmetatable({
		game = gameProxy,
		workspace = gameProxy.Workspace,
		Workspace = gameProxy.Workspace,
		script = scriptProxy,
		require = loren_require,
		typeof = loren_typeof,
		type = loren_type,
		tostring = loren_tostring,
		task = taskShim,
		coroutine = coroutineShim,
		Enum = EnumShim,
		wait = loren_wait,
		Instance = InstanceShim,
		debug = debugShim,
		os = osShim,
	}, { __index = baseEnv })
end

local __lorenEnv = makeEnv(nil)
