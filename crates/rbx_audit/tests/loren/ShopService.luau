local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Loren = require(ReplicatedStorage.Shared.Loren)

local Prices = { Sword = 100, Shield = 250 }

local ShopService = {
	Dependencies = {},
	Client = {},
	ClientEvents = { "Tip", "Emote" },
	Spec = {
		SetVolume = Loren.Method({ Loren.T.u8 }),
	},
	Middleware = {
		Emote = function(player, emote)
			return typeof(emote) == "string"
		end,
	},
}

local function coins(player)
	return player.leaderstats.Coins
end

local function give(player, amount)
	coins(player).Value += amount -- expect: unvalidated-argument
end

-- The client says how many coins it gets, through a helper.
function ShopService.Client:Grant(player, amount)
	give(player, amount)
end

-- Safe: the price is the server's.
function ShopService.Client:Buy(player, item)
	local price = Prices[item]
	if not price or coins(player).Value < price then
		return false
	end
	coins(player).Value -= price
	return true
end

-- One bound only: a negative amount pays the player.
function ShopService.Client:Withdraw(player, amount)
	if amount > 0 then
		coins(player).Value -= amount -- expect: unvalidated-argument
	end
end

-- Safe: a number, bounded on both sides.
function ShopService.Client:Donate(player, amount)
	if typeof(amount) ~= "number" or amount < 1 or amount > coins(player).Value then
		return
	end
	coins(player).Value -= amount
end

-- Typed by its Spec before it gets here, so one severity lower.
function ShopService.Client:SetVolume(player, volume)
	player.Settings.Volume.Value = volume -- expect: unvalidated-argument
end

function ShopService.Client:Rename(newName) -- expect: player-argument
	return newName
end

function ShopService:LorenIgnite()
	self.ClientEvents.Tip:Connect(function(player, amount)
		local n = tonumber(amount)
		if not n then
			return
		end
		coins(player).Value -= n -- expect: unvalidated-argument
	end)
	self.ClientEvents.Emote:Connect(function(player, emote)
		player:SetAttribute("Emote", emote)
	end)
end

return ShopService
