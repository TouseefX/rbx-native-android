local ReplicatedStorage = game:GetService("ReplicatedStorage")
local MarketplaceService = game:GetService("MarketplaceService")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local Items = { Sword = 100, Shield = 250 }
local Products = { small = 1001, large = 1002 }

-- The client says how many coins it gets.
Remotes.AddCoins.OnServerEvent:Connect(function(player, amount)
	player.leaderstats.Coins.Value += amount -- expect: unvalidated-argument
end)

-- Safe: the price comes from a server table the client's choice indexes.
Remotes.BuyItem.OnServerEvent:Connect(function(player, itemName)
	local price = Items[itemName]
	if not price then
		return
	end
	local coins = player.leaderstats.Coins
	if coins.Value < price then
		return
	end
	coins.Value -= price
end)

-- Safe: coerced and clamped before use.
Remotes.Donate.OnServerEvent:Connect(function(player, raw)
	local amount = math.clamp(tonumber(raw) or 0, 0, 100)
	player.leaderstats.Coins.Value -= amount
end)

-- A field of the payload, written straight through a local.
Remotes.SetTitle.OnServerEvent:Connect(function(player, data)
	local title = data.title
	player.leaderstats.Title.Value = title -- expect: unvalidated-argument
end)

-- Safe: type and length checked in the branch that writes.
Remotes.SetBio.OnServerEvent:Connect(function(player, bio)
	if typeof(bio) == "string" and #bio < 200 then
		player.Bio.Value = bio
	end
end)

-- The client picks the product.
Remotes.Prompt.OnServerEvent:Connect(function(player, productId)
	MarketplaceService:PromptProductPurchase(player, productId) -- expect: unvalidated-argument
end)

-- Safe: the product id comes from a whitelist.
Remotes.PromptSafe.OnServerEvent:Connect(function(player, size)
	local id = Products[size]
	if id then
		MarketplaceService:PromptProductPurchase(player, id)
	end
end)

-- Safe: a lookup, returned.
Remotes.GetPrice.OnServerInvoke = function(player, itemName)
	return Items[itemName]
end

-- Made here at run time; clients find it by name.
local runtime = Instance.new("RemoteEvent")
runtime.Name = "Runtime"
runtime.Parent = Remotes
runtime.OnServerEvent:Connect(function() end)

Remotes.Notify:FireAllClients("open")
