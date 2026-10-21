-- Combat: damage, healing and movement abilities
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local MAX_DAMAGE = 25
local COOLDOWN = 0.5
local lastHit = {}

local Dashes = {
	Short = 10,
	Long = 25,
}

local function humanoidOf(player)
	local character = player.Character
	return character and character:FindFirstChildOfClass("Humanoid")
end

local function onCooldown(player)
	local now = os.clock()
	if lastHit[player] and now - lastHit[player] < COOLDOWN then
		return true
	end
	lastHit[player] = now
	return false
end

Players.PlayerRemoving:Connect(function(player)
	lastHit[player] = nil
end)

Remotes.Damage.OnServerEvent:Connect(function(player, target, amount)
	if onCooldown(player) then
		return
	end
	if typeof(target) ~= "Instance" or not target:IsA("Humanoid") then
		return
	end
	amount = math.clamp(tonumber(amount) or 0, 0, MAX_DAMAGE)
	target.Health -= amount
end)

Remotes.Attack.OnServerEvent:Connect(function(damage, target) -- expect: player-argument
	print("attack", damage, target)
end)

Remotes.Heal.OnServerEvent:Connect(function(player, healAmount)
	local humanoid = humanoidOf(player)
	if humanoid then
		humanoid.Health = healAmount -- expect: unvalidated-argument
	end
end)

Remotes.Haste.OnServerEvent:Connect(function(player, speed)
	local humanoid = humanoidOf(player)
	if humanoid then
		humanoid.WalkSpeed = speed -- expect: unvalidated-argument
	end
end)

Remotes.Dash.OnServerEvent:Connect(function(player, kind)
	local distance = Dashes[kind]
	local character = player.Character
	if not distance or not character then
		return
	end
	local root = character.PrimaryPart
	root.CFrame = root.CFrame * CFrame.new(0, 0, -distance)
end)

Remotes.Blink.OnServerEvent:Connect(function(player, destination)
	local character = player.Character
	if character and character.PrimaryPart then
		character.PrimaryPart.Position = destination -- expect: unvalidated-argument
	end
end)

Remotes.Sprint.OnServerEvent:Connect(function(player, enabled)
	if typeof(enabled) ~= "boolean" then
		return
	end
	local humanoid = humanoidOf(player)
	if humanoid then
		humanoid.WalkSpeed = enabled and 24 or 16
	end
end)

Remotes.Ping.OnServerInvoke = function(player, stamp)
	if type(stamp) ~= "number" then
		return 0
	end
	return os.clock() - stamp
end
