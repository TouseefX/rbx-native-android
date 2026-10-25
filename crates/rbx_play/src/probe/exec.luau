-- Chunk body: `serialize, fiu, loadstring_ = ...`; returns the per-side runners.
local serialize, fiu, loadstring_ = ...

local function finish(ack, id, packed)
	if packed[1] then
		local ok, text = pcall(serialize, packed[2])
		if ok then
			ack(id, true, text)
		else
			ack(id, false, tostring(text))
		end
	else
		ack(id, false, tostring(packed[2]))
	end
end

-- Studio names code run over MCP `AssistantCommand`, so errors read
-- `AssistantCommand:<line>: <message>`.
local CHUNK = "=AssistantCommand"

-- Fiu reports `Fiu VM Error { ... Line: N ... }: <fiu>:<its line>: message`.
local function cleanFiu(text)
	local line, message = text:match("^Fiu VM Error {.-Line: (%d+).-}: (.*)$")
	if not line then
		return text
	end
	message = message:gsub("^[^:\n]*__rbxnative_fiu:%d+: ", "")
	return "AssistantCommand:" .. line .. ": " .. message
end

local function spawnRun(ack, id, run, clean)
	task.spawn(function()
		local packed = table.pack(xpcall(run, function(err)
			local text = tostring(err)
			return if clean then clean(text) else text
		end))
		finish(ack, id, packed)
	end)
end

local function compile(text)
	local ok, fn = pcall(loadstring_, text, CHUNK)
	if ok and fn then
		return fn
	end
	return nil
end

local function decodeHex(hex)
	return (hex:gsub("%x%x", function(pair)
		return string.char(tonumber(pair, 16))
	end))
end

local function server(cmd, ack)
	local source = cmd.source or ""
	local fn = compile("return " .. source)
	if not fn then
		local ok, loaded, err = pcall(loadstring_, source, CHUNK)
		if not ok or not loaded then
			ack(cmd.id, false, tostring(ok and err or loaded))
			return
		end
		fn = loaded
	end
	spawnRun(ack, cmd.id, fn)
end

local function client(cmd, ack)
	local settings = fiu.luau_newsettings()
	if Vector3 then
		settings.vectorCtor = Vector3.new
		settings.vectorSize = 3
	end
	local env = setmetatable({}, { __index = getfenv and getfenv(0) or _G })
	local ok, fn = pcall(fiu.luau_load, decodeHex(cmd.bytecode or ""), env, settings)
	if not ok then
		ack(cmd.id, false, tostring(fn))
		return
	end
	spawnRun(ack, cmd.id, fn, cleanFiu)
end

return { server = server, client = client }
