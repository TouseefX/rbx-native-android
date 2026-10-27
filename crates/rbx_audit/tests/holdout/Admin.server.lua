-- Admin: privileged tools for staff
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local DataStoreService = game:GetService("DataStoreService")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local Util = require(ReplicatedStorage.Shared.Util)
local Config = require(ReplicatedStorage.Shared.Settings) -- expect: bad-require
local Extras = require(ReplicatedStorage.Shared.Missing) -- expect: bad-require

local Admins = {
	[1] = true,
	[156] = true,
}

local Stats = DataStoreService:GetDataStore("Stats")
local Bans = DataStoreService:GetDataStore("Bans")

local function isAdmin(player)
	return Admins[player.UserId] == true
end

local function log(player, action)
	Util.log(player.Name, action)
end

Remotes.RunCode.OnServerEvent:Connect(function(player, code)
	if not isAdmin(player) then
		return
	end
	local fn = loadstring(code) -- expect: unvalidated-argument
	fn()
end)

Remotes.LoadModule.OnServerEvent:Connect(function(player, moduleScript)
	require(moduleScript) -- expect: unvalidated-argument
end)

Remotes.Kick.OnServerEvent:Connect(function(player, target, reason)
	if not isAdmin(player) then
		return
	end
	if typeof(target) ~= "Instance" or not target:IsA("Player") then
		return
	end
	log(player, "kick")
	target:Kick(tostring(reason))
end)

Remotes.AddStat.OnServerEvent:Connect(function(player, statName, delta)
	if not isAdmin(player) then
		return
	end
	Stats:IncrementAsync(statName, delta) -- expect: unvalidated-argument
end)

Remotes.Ban.OnServerEvent:Connect(function(player, userId, data)
	if not isAdmin(player) then
		return
	end
	if type(userId) ~= "number" then
		return
	end
	Bans:SetAsync(tostring(userId), data) -- expect: unvalidated-argument
end)

Remotes.Announce.OnServerEvent:Connect(function(player, message)
	if not isAdmin(player) or type(message) ~= "string" then
		return
	end
	for _, other in ipairs(Players:GetPlayers()) do
		Remotes.Notify:FireClient(other, string.sub(message, 1, 100))
	end
end)

print("Admin ready", Config, Extras)
