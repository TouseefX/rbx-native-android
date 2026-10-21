local Remotes = game.ReplicatedStorage.Remotes

local function onDamage(player, target, amount)
	local humanoid = target.Parent:FindFirstChildOfClass("Humanoid")
	humanoid:TakeDamage(amount) -- expect: unvalidated-argument
end
Remotes.Damage.OnServerEvent:Connect(onDamage)

-- Safe: the target is checked, the damage is the server's.
Remotes.Hit.OnServerEvent:Connect(function(player, target)
	if typeof(target) ~= "Instance" or not target:IsDescendantOf(workspace) then
		return
	end
	local humanoid = target:FindFirstChildOfClass("Humanoid")
	if humanoid then
		humanoid:TakeDamage(10)
	end
end)

-- The client moves its character anywhere.
Remotes.Teleport.OnServerEvent:Connect(function(player, cframe)
	local root = player.Character.HumanoidRootPart
	root.CFrame = cframe -- expect: unvalidated-argument
end)

-- Safe: the destination is one of the server's spawn points.
Remotes.TeleportSafe.OnServerEvent:Connect(function(player, spawnName)
	local spawn = workspace.Spawns:FindFirstChild(spawnName)
	if not spawn then
		return
	end
	player.Character:PivotTo(spawn.CFrame)
end)

-- Deletes whatever the client names.
Remotes.Drop.OnServerEvent:Connect(function(player, tool)
	tool:Destroy() -- expect: unvalidated-argument
end)

-- Safe: only a tool the player holds.
Remotes.DropSafe.OnServerEvent:Connect(function(player, tool)
	if typeof(tool) == "Instance" and tool:IsA("Tool") and tool.Parent == player.Backpack then
		tool.Parent = workspace
	end
end)

-- Reparents an instance the client chose.
Remotes.Equip.OnServerEvent:Connect(function(player, tool)
	tool.Parent = player.Character -- expect: unvalidated-argument
end)

-- The server waits on a client's answer.
local GetSettings = Remotes.GetSettings
game.Players.PlayerAdded:Connect(function(player)
	local settings = GetSettings:InvokeClient(player) -- expect: invoke-client
	print(settings)
end)
