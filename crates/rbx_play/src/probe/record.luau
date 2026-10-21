-- Playtest recording: samples every player's character, reports gameplay
-- events and carries `shared.rbxnative.mark`. Nothing here may error into
-- game code, so every engine call that can fail is in a pcall.
local HttpService = game:GetService("HttpService")
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

local Record = {}

local MAX_DEPTH = 6

local queue = nil
local t0 = 0
local remote = nil
local recording = false
local clientRemote = nil
local cams = {}

local function rate()
	local ok, value = pcall(function()
		return script:GetAttribute("RATE")
	end)
	if ok and type(value) == "number" and value > 0 then
		return value
	end
	return nil
end

local function rootOf(player)
	local ok, root = pcall(function()
		local character = player.Character
		return character and character:FindFirstChild("HumanoidRootPart")
	end)
	if ok and root and root:IsA("BasePart") then
		return root
	end
	return nil
end

local function posOf(root)
	local ok, p = pcall(function()
		local v = root.Position
		return { v.X, v.Y, v.Z }
	end)
	return ok and p or nil
end

local function vec(v)
	return { v.X, v.Y, v.Z }
end

local function plain(value, depth)
	local kind = typeof(value)
	if kind == "Vector3" or kind == "Instance" or kind == "CFrame" or kind == "Color3" then
		return tostring(value)
	elseif kind == "table" then
		if depth >= MAX_DEPTH then
			return tostring(value)
		end
		local out = {}
		for k, v in value do
			out[if type(k) == "string" or type(k) == "number" then k else tostring(k)] = plain(v, depth + 1)
		end
		return out
	elseif kind == "string" or kind == "number" or kind == "boolean" then
		return value
	end
	return tostring(value)
end

local function encode(data)
	if data == nil then
		return ""
	end
	local ok, json = pcall(function()
		return HttpService:JSONEncode(plain(data, 0))
	end)
	if ok and type(json) == "string" then
		return json
	end
	return tostring(data)
end

local function event(kind, user, pos, amount, label, data)
	queue({
		type = "rec_event",
		t = os.clock() - t0,
		user = user,
		kind = kind,
		pos = pos,
		amount = amount,
		label = label or "",
		data = data or "",
	})
end

local function number(v)
	return type(v) == "number" and v == v and math.abs(v) < 1e9
end

local function validVec(t)
	return type(t) == "table" and number(t[1]) and number(t[2]) and number(t[3])
end

-- Health lost from `last` to `health`, or nil when it is not a damage
-- event. A drop to 0 is the death itself: a reset sets Health to 0 and
-- HealthChanged fires before Died, which logged a 100-point hit at the
-- moment of every reset.
function Record.damageTaken(last, health, dead)
	if dead or health <= 0 or health >= last then
		return nil
	end
	return last - health
end

local function hookCharacter(player, character, first)
	if not first then
		local root = rootOf(player)
		event("respawn", player.UserId, root and posOf(root), nil, "", "")
	end
	task.spawn(function()
		local ok = pcall(function()
			local humanoid = character:WaitForChild("Humanoid", 10)
			if not humanoid then
				return
			end
			local last = humanoid.Health
			local dead = false
			humanoid.Died:Connect(function()
				dead = true
				local root = rootOf(player)
				event("death", player.UserId, root and posOf(root), nil, "", "")
			end)
			humanoid.HealthChanged:Connect(function(health)
				local amount = Record.damageTaken(last, health, dead)
				if amount then
					local root = rootOf(player)
					event("damage", player.UserId, root and posOf(root), amount, "", "")
				end
				last = health
			end)
		end)
		if not ok then
			return
		end
	end)
end

local function hookPlayer(player)
	pcall(function()
		event("join", player.UserId, nil, nil, player.Name, "")
		player.Chatted:Connect(function(message)
			local root = rootOf(player)
			event("chat", player.UserId, root and posOf(root), nil, tostring(message), "")
		end)
		local seen = false
		player.CharacterAdded:Connect(function(character)
			local first = not seen
			seen = true
			pcall(hookCharacter, player, character, first)
		end)
		if player.Character then
			seen = true
			pcall(hookCharacter, player, player.Character, true)
		end
	end)
end

local function sampleAll()
	local samples = {}
	for _, player in Players:GetPlayers() do
		local root = rootOf(player)
		if root then
			pcall(function()
				local p = root.Position
				local look = root.CFrame.LookVector
				local sample = {
					t = os.clock() - t0,
					user = player.UserId,
					pos = { p.X, p.Y, p.Z },
					yaw = math.atan2(-look.X, -look.Z),
				}
				local cam = cams[player]
				if cam then
					sample.cam = cam
				end
				table.insert(samples, sample)
			end)
		end
	end
	if #samples > 0 then
		queue({ type = "rec_samples", samples = samples })
	end
end

function Record.fromClient(player, payload)
	pcall(function()
		if not recording or typeof(player) ~= "Instance" then
			return
		end
		if payload.kind == "rec_cam" then
			local c = payload.cam
			if type(c) == "table" then
				for i = 1, 6 do
					if not number(c[i]) then
						return
					end
				end
				cams[player] = { c[1], c[2], c[3], c[4], c[5], c[6] }
			end
		elseif payload.kind == "rec_mark" and type(payload.name) == "string" then
			local pos = if validVec(payload.pos) then { payload.pos[1], payload.pos[2], payload.pos[3] } else nil
			if not pos then
				local root = rootOf(player)
				pos = root and posOf(root)
			end
			local data = if type(payload.data) == "string" then payload.data else ""
			event("mark", player.UserId, pos, nil, payload.name, string.sub(data, 1, 4000))
		end
	end)
end

local function serverMark(name, data)
	local pos, user = nil, 0
	if type(data) == "table" then
		if typeof(data.position) == "Vector3" then
			pos = vec(data.position)
		end
		if typeof(data.player) == "Instance" and data.player:IsA("Player") then
			user = data.player.UserId
			if not pos then
				local root = rootOf(data.player)
				pos = root and posOf(root)
			end
		end
	end
	event("mark", user, pos, nil, name, encode(data))
end

local function clientMark(name, data)
	local pos = nil
	local root = rootOf(Players.LocalPlayer)
	if root then
		pos = posOf(root)
	end
	if type(data) == "table" and typeof(data.position) == "Vector3" then
		pos = vec(data.position)
	end
	clientRemote:FireServer({ kind = "rec_mark", name = name, data = encode(data), pos = pos })
end

Record.api = {
	mark = function(name, data)
		if type(name) ~= "string" then
			return
		end
		pcall(function()
			if RunService:IsServer() then
				if recording then
					serverMark(name, data)
				end
			elseif clientRemote then
				clientMark(name, data)
			end
		end)
	end,
}

function Record.startServer(q, start, r)
	shared.rbxnative = Record.api
	local hz = rate()
	if not hz then
		return
	end
	queue = q
	t0 = start
	remote = r
	recording = true
	for _, player in Players:GetPlayers() do
		hookPlayer(player)
	end
	Players.PlayerAdded:Connect(hookPlayer)
	Players.PlayerRemoving:Connect(function(player)
		pcall(function()
			event("leave", player.UserId, nil, nil, player.Name, "")
			cams[player] = nil
		end)
	end)
	task.spawn(function()
		while true do
			task.wait(1 / hz)
			pcall(sampleAll)
		end
	end)
end

function Record.startClient(r)
	shared.rbxnative = Record.api
	clientRemote = r
	task.spawn(function()
		while true do
			local hz = rate()
			task.wait(if hz then 1 / hz else 1)
			if hz then
				pcall(function()
					local camera = workspace.CurrentCamera
					if camera then
						local cf = camera.CFrame
						local p, l = cf.Position, cf.LookVector
						clientRemote:FireServer({
							kind = "rec_cam",
							cam = { p.X, p.Y, p.Z, l.X, l.Y, l.Z },
						})
					end
				end)
			end
		end
	end)
end

return Record
