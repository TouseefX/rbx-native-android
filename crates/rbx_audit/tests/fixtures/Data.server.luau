local DataStoreService = game:GetService("DataStoreService")
local ServerStorage = game:GetService("ServerStorage")
local store = DataStoreService:GetDataStore("Profiles")
local Remotes = game.ReplicatedStorage.Remotes

-- The client's table goes straight into the store.
Remotes.SaveData.OnServerEvent:Connect(function(player, profile)
	store:SetAsync(player.UserId, profile) -- expect: unvalidated-argument
end)

-- Safe: a number in range, or nothing.
Remotes.SaveSettings.OnServerEvent:Connect(function(player, volume)
	if type(volume) ~= "number" or volume < 0 or volume > 1 then
		return
	end
	store:UpdateAsync(player.UserId, function(old)
		old = old or {}
		old.volume = volume
		return old
	end)
end)

-- Safe: asserted before the write.
Remotes.SaveLevel.OnServerEvent:Connect(function(player, level)
	assert(typeof(level) == "number" and level >= 1 and level <= 50)
	store:SetAsync(player.UserId .. ":level", level)
end)

print(ServerStorage.Lib)
