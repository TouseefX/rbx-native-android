local Remotes = game.ReplicatedStorage.Remotes

Remotes.Gift.OnServerEvent:Connect(function(amount) -- expect: player-argument
	print(amount)
end)

Remotes.Vote.OnServerEvent:Connect(function(plr, choice)
	if plr == "yes" then -- expect: player-argument
		print(choice)
	end
end)

Remotes.Score.OnServerEvent:Connect(function(who)
	local n = tonumber(who) -- expect: player-argument
	print(n)
end)

-- Safe: the first parameter is used as the Player.
Remotes.Ping.OnServerEvent:Connect(function(player)
	print(player.Name .. " pinged")
end)
