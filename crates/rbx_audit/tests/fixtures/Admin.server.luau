local Remotes = game:GetService("ReplicatedStorage").Remotes
local Commands = require(script.Parent.Commands)
local Config = require(script.Parent.Config) -- expect: bad-require
local Lib = require(game.ServerStorage.Lib.Missing) -- expect: bad-require
local Published = require(1234567)

Remotes.AdminCommand.OnServerEvent:Connect(function(player, ...)
	local args = { ... }
	local fn = loadstring(args[1]) -- expect: unvalidated-argument
	fn()
end)

Remotes.LoadModule.OnServerEvent:Connect(function(player, module)
	require(module) -- expect: unvalidated-argument
end)

Remotes.Spawn.OnServerEvent:Connect(function(player, className)
	local thing = Instance.new(className) -- expect: unvalidated-argument
	thing.Parent = workspace
end)

-- Safe: only whitelisted commands, only for the owner.
Remotes.RunCommand.OnServerEvent:Connect(function(player, name, ...)
	local command = Commands[name]
	if command and player.UserId == game.CreatorId then
		command(player, ...)
	end
end)

-- Safe: the class is checked against a whitelist first.
local Spawnable = { Part = true, WedgePart = true }
Remotes.SpawnSafe.OnServerEvent:Connect(function(player, className)
	if not Spawnable[className] then
		return
	end
	Instance.new(className).Parent = workspace
end)

print(Config, Lib, Published)
