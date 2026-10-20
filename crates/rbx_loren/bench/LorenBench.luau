--!strict
-- Times Loren's hot paths the way its unit specs drive them: the server
-- through Transport's test hooks, the client through ClientNet's, both wired
-- back to back. Timed loops never yield, so os.clock deltas are CPU time.
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local ServerScriptService = game:GetService("ServerScriptService")

local Runtime = ReplicatedStorage.LorenRuntime
local Loren = require(Runtime) :: any
local C = require(Runtime.Constants) :: any
local Codec = require(Runtime.Codec) :: any
local Schema = require(Runtime.Schema) :: any
local Config = require(Runtime.Config) :: any
local Log = require(Runtime.Log) :: any
local Signal = require(Runtime.Signal) :: any
local ClientNet = require(Runtime.ClientNet) :: any
local Calls = require(Runtime.Calls) :: any
local Proxy = require(Runtime.Proxy) :: any
local Server = ServerScriptService.LorenServer
local Routes = require(Server.Routes) :: any
local Transport = require(Server.Transport) :: any
local Outbox = require(Server.Outbox) :: any
local Dispatch = require(Server.Dispatch) :: any
local ServerSignal = require(Server.ServerSignal) :: any
local Limits = require(Server.Limits) :: any
local Violations = require(Server.Violations) :: any

local T = Schema.T
local K = C.Kind
local OP = C.Op

local RUNS = 7
local WARMUP = 2
local MESSAGES = 1000
local ENTITIES = 100
local OPS = 10000

local function noop(...: any) end

local ENTITY = T.struct({ id = T.u16, pos = T.Vector3, hp = T.f32 })
local UNLIMITED = { RateLimit = { 1e6, 1e6 } }

local now = 1000
local rt: any = nil
local fires: { any } = {}
local c2s: { any } = {}
local nextUid = 0
local seqN = 0
local handled = 0

local function member(kind: number, name: string, spec: any): any
	return { name = name, kind = kind, fn = nil, spec = spec, middleware = nil, colon = false }
end

local function services(): any
	local members = {
		member(K.Signal, "Sig"),
		member(K.Signal, "Ent", Schema.signal(ENTITY)),
		member(K.Event, "Ev", Schema.event(UNLIMITED)),
	}
	local module = { Client = {}, Signals = {}, ClientEvents = {}, Middleware = {} }
	return {
		{
			name = "S",
			record = { name = "S", kind = "Service", module = module, instance = script, path = "S", deps = {} },
			module = module,
			client = module.Client,
			members = members,
			networked = true,
		},
	}
end

local function resetAll()
	ClientNet._reset()
	Calls._reset()
	Proxy._reset()
	Transport._reset()
	Routes._reset()
	Dispatch._reset()
	Outbox._reset()
	ServerSignal._reset()
	Limits._reset()
	Violations._reset()
end

local function setup()
	Loren.Testing.Reset({ Offline = true })
	resetAll()
	Log._test.setSink(noop, noop)
	local c = Config.get()
	Limits.configure(c)
	Violations.configure(c)
	local report: any = { side = "Server", errors = {}, warnings = {} }
	rt = Routes.build(services(), c, report)
	assert(rt ~= nil, "Routes.build failed: " .. table.concat(report.errors, "; "))
	fires = {}
	c2s = {}
	handled = 0
	Outbox._test.capture(function(channel: string, player: any, frame: buffer, refs: { any }?)
		table.insert(fires, { channel = channel, player = player, frame = frame, refs = refs })
	end)
	now = 1000
	Transport._test.setClock(function(): number
		return now
	end)
	ClientNet._test.setClock(function(): number
		return now
	end)
	Transport._test.attach(rt)
	Dispatch._test.setForwardErrors(false)
	rt.events.S.Ev:Connect(function()
		handled += 1
	end)
end

local function hello(p: any)
	local m = buffer.create(2)
	buffer.writeu8(m, 0, OP.HELLO)
	buffer.writeu8(m, 1, C.PROTO)
	seqN = (seqN + 1) % 65536
	local b = buffer.create(8)
	buffer.writeu16(b, 0, seqN)
	buffer.writeu16(b, 2, 1)
	buffer.writeu8(b, 4, 0)
	buffer.writeu8(b, 5, 2)
	buffer.copy(b, 6, m, 0, 2)
	Transport._test.frame(false, p, b, nil)
end

local function takeFires(): { any }
	local list = fires
	fires = {}
	return list
end

local function toClient(list: { any })
	for _, f in list do
		ClientNet._test.receive(f.frame, f.refs, f.channel == "Unreliable")
	end
end

-- A server with one player whose client has installed the manifest.
local function connect(): any
	setup()
	nextUid += 1
	local p: any = { UserId = nextUid, Name = "Player" .. nextUid, Kick = noop }
	Transport._test.addPlayer(p)
	ClientNet._test.setFire(function(unreliable: boolean, frame: buffer, refs: { any }?)
		table.insert(c2s, { unreliable = unreliable, frame = frame, refs = refs })
	end)
	ClientNet._test.begin()
	hello(p)
	Transport._test.flush()
	toClient(takeFires())
	assert(#ClientNet.routes() == rt.count, "the client did not install the server's MANIFEST")
	return p
end

local function clientRoute(name: string): any
	for _, r in rt.routes do
		if r.member == name then
			return ClientNet.routes()[r.id]
		end
	end
	error("no route " .. name)
end

-- Flushes until a flush produces nothing more, so a frame's work is all counted.
local function flushServer()
	Transport._test.flush()
	for _ = 1, 100 do
		local before = #fires
		now += 1
		Transport._test.flush()
		if #fires == before then
			return
		end
	end
end

local function flushClient()
	for _ = 1, 100 do
		local before = #c2s
		now += 1
		ClientNet._test.flushNow()
		if #c2s == before then
			return
		end
	end
end

type Case = { name: string, prepare: (() -> any)?, run: (any) -> (), check: ((any) -> ())? }

local function median(xs: { number }): number
	local s = table.clone(xs)
	table.sort(s)
	return s[(#s + 1) // 2]
end

local function measure(case: Case): { number }
	local times = {}
	for i = 1, WARMUP + RUNS do
		local state = if case.prepare then case.prepare() else nil
		local t0 = os.clock()
		case.run(state)
		local dt = os.clock() - t0
		if case.check then
			case.check(state)
		end
		if i > WARMUP then
			table.insert(times, dt * 1e6)
		end
	end
	return times
end

local function entity(i: number): any
	return { id = i, pos = Vector3.new(i, i * 0.5, -i), hp = 100 - i * 0.25 }
end

local cases: { Case } = {
	{
		name = "signal_send",
		prepare = function()
			connect()
			return rt.signals.S.Sig
		end,
		run = function(sig: any)
			for i = 1, MESSAGES do
				sig:FireAll(i)
			end
			flushServer()
		end,
		check = function()
			assert(#fires > 0, "signal_send produced no frames")
		end,
	},
	{
		name = "signal_receive",
		prepare = function()
			connect()
			local got = { n = 0 }
			clientRoute("Sig").signal:Connect(function()
				got.n += 1
			end)
			for i = 1, MESSAGES do
				rt.signals.S.Sig:FireAll(i)
			end
			flushServer()
			got.frames = takeFires()
			return got
		end,
		run = function(got: any)
			toClient(got.frames)
		end,
		check = function(got: any)
			assert(got.n == MESSAGES, "signal_receive ran " .. got.n .. " listeners")
		end,
	},
	{
		name = "entity_send",
		prepare = function()
			connect()
			local list = {}
			for i = 1, ENTITIES do
				list[i] = entity(i)
			end
			return { sig = rt.signals.S.Ent, list = list }
		end,
		run = function(s: any)
			for _, e in s.list do
				s.sig:FireAll(e)
			end
			flushServer()
		end,
		check = function()
			assert(#fires > 0, "entity_send produced no frames")
		end,
	},
	{
		name = "entity_receive",
		prepare = function()
			connect()
			local got = { n = 0 }
			clientRoute("Ent").signal:Connect(function()
				got.n += 1
			end)
			for i = 1, ENTITIES do
				rt.signals.S.Ent:FireAll(entity(i))
			end
			flushServer()
			got.frames = takeFires()
			return got
		end,
		run = function(got: any)
			toClient(got.frames)
		end,
		check = function(got: any)
			assert(got.n == ENTITIES, "entity_receive ran " .. got.n .. " listeners")
		end,
	},
	{
		name = "event_send",
		prepare = function()
			connect()
			return clientRoute("Ev")
		end,
		run = function(r: any)
			for i = 1, MESSAGES do
				ClientNet.fire(r, 1, i)
			end
			flushClient()
		end,
		check = function()
			assert(#c2s > 0, "event_send produced no frames")
		end,
	},
	{
		name = "event_receive",
		prepare = function()
			local p = connect()
			local r = clientRoute("Ev")
			for i = 1, MESSAGES do
				ClientNet.fire(r, 1, i)
			end
			flushClient()
			local list = c2s
			c2s = {}
			return { p = p, frames = list }
		end,
		run = function(s: any)
			for _, f in s.frames do
				-- One client frame per server second keeps the player's admission buckets full.
				now += 1
				Transport._test.frame(f.unreliable, s.p, f.frame, f.refs)
			end
		end,
		check = function()
			assert(handled == MESSAGES, "event_receive ran " .. handled .. " handlers")
		end,
	},
	{
		name = "codec_untyped",
		run = function()
			local part = { tag = "hit", n = 3 }
			for _ = 1, OPS do
				local w = Codec.newWriter(64, false)
				Codec.writeArgs(w, 255, 6, 42, -1.5, "hello", true, Vector3.new(1, 2, 3), part)
				local b = buffer.create(w.n)
				buffer.copy(b, 0, w.buf, 0, w.n)
				local ctx = Codec.newReadCtx(false, false)
				Codec.beginFrame(ctx, nil, 0)
				Codec.beginMessage(ctx, false)
				Codec.readArgs(b, 0, buffer.len(b), ctx, {}, 255)
			end
		end,
	},
	{
		name = "schema_typed",
		prepare = function()
			return Schema.compile({ ENTITY } :: any, false, C.Limit.MAX_ARGS_C2S, false)
		end,
		run = function(pc: any)
			local e = entity(7)
			for _ = 1, OPS do
				local w = Codec.newWriter(16, false)
				pc.encode(w, 1, e)
				local b = buffer.create(w.n)
				buffer.copy(b, 0, w.buf, 0, w.n)
				local ctx = Codec.newReadCtx(false, false)
				Codec.beginFrame(ctx, nil, 0)
				Codec.beginMessage(ctx, pc.allowNonFinite)
				pc.decode(b, 0, buffer.len(b), ctx, {})
			end
		end,
	},
	{
		name = "localsignal_fire",
		prepare = function()
			local sig = Signal.LocalSignal.new()
			local got = { n = 0, sig = sig }
			for _ = 1, 5 do
				sig:Connect(function()
					got.n += 1
				end)
			end
			return got
		end,
		run = function(got: any)
			for i = 1, OPS do
				got.sig:Fire(i)
			end
		end,
		check = function(got: any)
			assert(got.n == OPS * 5, "localsignal_fire ran " .. got.n .. " listeners")
		end,
	},
}

-- Per-frame cases report one frame's cost; per-op cases scale to 1000 ops.
local PER_OPS = { codec_untyped = true, schema_typed = true, localsignal_fire = true }

return function(opts: any?)
	local filter = if opts then opts.filter else nil
	print("BENCH|runtime|" .. tostring(Loren.Version))
	for _, case in cases do
		if filter == nil or string.find(case.name, filter, 1, true) then
			local times = measure(case)
			if PER_OPS[case.name] then
				for i, v in times do
					times[i] = v * 1000 / OPS
				end
			end
			local lo, hi = math.huge, 0
			for _, v in times do
				lo = math.min(lo, v)
				hi = math.max(hi, v)
			end
			print(string.format("BENCH|%s|%.1f|%.1f|%.1f", case.name, median(times), lo, hi))
		end
	end
	resetAll()
	Loren.Testing.Reset()
end
