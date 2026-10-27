-- UiController: shop buttons, notifications and trade prompts
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local UserInputService = game:GetService("UserInputService")
local DataStoreService = game:GetService("DataStoreService") -- expect: server-api-on-client
local ServerStorage = game:GetService("ServerStorage") -- expect: server-api-on-client

local LocalPlayer = Players.LocalPlayer
local Remotes = ReplicatedStorage:WaitForChild("Remotes")
local gui = LocalPlayer:WaitForChild("PlayerGui"):WaitForChild("Shop")

local selected = nil

local function setStatus(text)
	gui.Status.Text = text
end

local function select(itemId)
	selected = itemId
	setStatus("Selected " .. tostring(itemId))
end

gui.Buy.Activated:Connect(function()
	if selected then
		Remotes.BuyItem:FireServer(selected)
	end
end)

gui.Donate.Activated:Connect(function()
	local amount = tonumber(gui.Amount.Text)
	if amount then
		Remotes.Donate:FireServer(amount)
	end
end)

gui.Tip.Activated:Connect(function()
	Remotes.Tip:FireServer(game.Players.LocalPlayer, 5) -- expect: player-argument
end)

gui.Emote.Activated:Connect(function()
	Remotes:WaitForChild("Emote"):FireServer("wave") -- expect: missing-remote
end)

gui.Wave.Activated:Connect(function()
	Remotes.Wave:FireServer() -- expect: missing-remote
end)

Remotes.Notify.OnClientEvent:Connect(function(text)
	setStatus(text)
end)

Remotes.Confirm.OnClientInvoke = function(requester)
	setStatus(tostring(requester) .. " wants to trade")
	return true
end

UserInputService.InputBegan:Connect(function(input, processed)
	if processed then
		return
	end
	if input.KeyCode == Enum.KeyCode.One then
		select("Sword")
	elseif input.KeyCode == Enum.KeyCode.Two then
		select("Shield")
	elseif input.KeyCode == Enum.KeyCode.Three then
		select("Potion")
	end
end)

local price = Remotes.GetPrice:InvokeServer("Sword")
setStatus("Sword costs " .. tostring(price))
