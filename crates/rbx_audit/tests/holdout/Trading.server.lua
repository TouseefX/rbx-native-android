-- Trading: player-to-player trades with confirmation prompts
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local trades = {}
local TRADE_TIMEOUT = 60

local function isPlayer(value)
	return typeof(value) == "Instance" and value:IsA("Player")
end

local function openTrade(a, b)
	local folder = Instance.new("Folder")
	folder.Name = a.Name .. "_" .. b.Name
	folder.Parent = workspace
	trades[folder] = { a = a, b = b, opened = os.time() }
	return folder
end

local function expire()
	for folder, trade in pairs(trades) do
		if os.time() - trade.opened > TRADE_TIMEOUT then
			trades[folder] = nil
			folder:Destroy()
		end
	end
end

task.spawn(function()
	while true do
		task.wait(10)
		expire()
	end
end)

Remotes.TradeRequest.OnServerEvent:Connect(function(player, target)
	if not isPlayer(target) or target == player then
		return
	end
	local ok, answer = pcall(function()
		return Remotes.Confirm:InvokeClient(target, player.Name) -- expect: invoke-client
	end)
	if ok and answer == true then
		openTrade(player, target)
	end
end)

Remotes.TradeCancel.OnServerEvent:Connect(function(player, tradeFolder)
	tradeFolder:Destroy() -- expect: unvalidated-argument
end)

Remotes.Confirm.OnServerInvoke = function(player, tradeId)
	if type(tradeId) ~= "string" then
		return false
	end
	return workspace:FindFirstChild(tradeId) ~= nil
end

Remotes.GiftPet.OnServerEvent:Connect(function(player, target, petModel)
	if not isPlayer(target) then
		return
	end
	if typeof(petModel) ~= "Instance" or petModel.Parent ~= player.Backpack then
		return
	end
	petModel.Parent = target.Backpack
end)

Remotes.StealPet.OnServerEvent:Connect(function(player, petModel)
	petModel.Parent = player.Backpack -- expect: unvalidated-argument
end)

Remotes.TradeNotice.OnServerEvent:Connect(function(player, target, message)
	if not isPlayer(target) then
		return
	end
	local answer = Remotes.Confirm:InvokeClient(target, message) -- expect: invoke-client
	if not answer then
		return
	end
	Remotes.Notify:FireClient(target, tostring(message))
end)

Remotes.Splash.OnServerEvent:Connect(function(player, crate)
	crate.Position = Vector3.new(0, 50, 0) -- expect: unvalidated-argument
end)
