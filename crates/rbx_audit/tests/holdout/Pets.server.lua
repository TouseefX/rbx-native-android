-- Pets: equipping, saving, spawning and dropping items
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local DataStoreService = game:GetService("DataStoreService")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")

local PetStore = DataStoreService:GetDataStore("Pets")

local PetCatalog = {
	Dog = { rarity = "Common" },
	Cat = { rarity = "Common" },
	Dragon = { rarity = "Legendary" },
}

local Spawnable = {
	Crate = true,
	Barrel = true,
}

local equipped = {}

local function key(player)
	return "Player_" .. player.UserId
end

local function save(player)
	local ok, err = pcall(function()
		PetStore:SetAsync(key(player), equipped[player])
	end)
	if not ok then
		warn("save failed", err)
	end
end

Players.PlayerRemoving:Connect(function(player)
	save(player)
	equipped[player] = nil
end)

Remotes.EquipPet.OnServerEvent:Connect(function(player, petName, saveSlot)
	if not PetCatalog[petName] then
		return
	end
	equipped[player] = petName
	PetStore:SetAsync(key(player), saveSlot) -- expect: unvalidated-argument
end)

Remotes.DeletePet.OnServerEvent:Connect(function(player, petModel)
	petModel:Destroy() -- expect: unvalidated-argument
end)

Remotes.ReleasePet.OnServerEvent:Connect(function(petName) -- expect: player-argument
	print("released", petName)
end)

Remotes.ReparentPet.OnServerEvent:Connect(function(player, newParent)
	local pet = workspace:FindFirstChild(player.Name .. "Pet")
	if pet then
		pet.Parent = newParent -- expect: unvalidated-argument
	end
end)

Remotes.SpawnItem.OnServerEvent:Connect(function(player, className)
	local item = Instance.new(className) -- expect: unvalidated-argument
	item.Parent = workspace
end)

Remotes.DropItem.OnServerEvent:Connect(function(player, itemName)
	if type(itemName) ~= "string" or not Spawnable[itemName] then
		return
	end
	local template = ReplicatedStorage.Items:FindFirstChild(itemName)
	if not template then
		return
	end
	local clone = template:Clone()
	clone.Parent = workspace
end)

Remotes.FeedPet.OnServerEvent:Connect(function(player, pet)
	if typeof(pet) ~= "Instance" or not pet:IsDescendantOf(workspace) then
		return
	end
	pet:Destroy()
end)
