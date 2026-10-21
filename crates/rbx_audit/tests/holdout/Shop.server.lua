-- Shop: purchases, sales, tips and price lookups
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local MarketplaceService = game:GetService("MarketplaceService")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local Items = {
	Sword = { price = 100 },
	Shield = { price = 150 },
	Potion = { price = 25 },
}

local Products = {
	[1001] = "Boost",
	[1002] = "Crate",
}

local function coins(player)
	return player.leaderstats.Coins
end

local function notify(player, text)
	Remotes.Notify:FireClient(player, text)
end

Players.PlayerAdded:Connect(function(player)
	local stats = Instance.new("Folder")
	stats.Name = "leaderstats"
	local c = Instance.new("IntValue")
	c.Name = "Coins"
	c.Value = 50
	c.Parent = stats
	stats.Parent = player
	notify(player, "Welcome to the shop")
end)

Remotes.BuyItem.OnServerEvent:Connect(function(player, itemId)
	local item = Items[itemId]
	if not item then
		return
	end
	local c = coins(player)
	if c.Value < item.price then
		notify(player, "Not enough coins")
		return
	end
	c.Value -= item.price
end)

Remotes.SellItem.OnServerEvent:Connect(function(player, itemName, payout)
	local c = coins(player)
	c.Value = c.Value + payout -- expect: unvalidated-argument
end)

Remotes.Donate.OnServerEvent:Connect(function(player, amount)
	amount = math.clamp(tonumber(amount) or 0, 0, 1000)
	if amount <= 0 then
		return
	end
	coins(player).Value -= amount
end)

Remotes.Tip.OnServerEvent:Connect(function(player, target, amount)
	if typeof(target) ~= "Instance" or not target:IsA("Player") then
		return
	end
	if type(amount) ~= "number" or amount < 1 or amount > 500 then
		return
	end
	coins(target).Value += amount
end)

Remotes.BuyProduct.OnServerEvent:Connect(function(player, productId)
	MarketplaceService:PromptProductPurchase(player, productId) -- expect: unvalidated-argument
end)

Remotes.GetPrice.OnServerInvoke = function(player, itemId)
	local item = Items[itemId]
	if not item then
		return nil
	end
	return item.price
end

Remotes.Redeem.OnServerEvent:Connect(function(player, productId)
	local name = Products[productId]
	assert(name, "unknown product")
	notify(player, name)
end)

Remotes.Bonus.OnServerEvent:Connect(function(player, bonus)
	coins(player).Value += bonus -- expect: unvalidated-argument
end)

Remotes.Refund.OnServerEvent:Connect(function(player, refund)
	local n = tonumber(refund)
	if not n then
		return
	end
	coins(player).Value += math.clamp(n, 0, 10)
end)

Remotes.Gift.OnServerEvent:Connect(function(player, gift)
	coins(player).Value = gift -- expect: unvalidated-argument
end)
