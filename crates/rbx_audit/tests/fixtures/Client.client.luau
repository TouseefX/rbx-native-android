local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Remotes = ReplicatedStorage:WaitForChild("Remotes")
local Shared = require(ReplicatedStorage:WaitForChild("Shared"))
local player = Players.LocalPlayer

Remotes.BuyItem:FireServer("Sword")
Remotes.AddCoins:FireServer(player, 10) -- expect: player-argument
Remotes:WaitForChild("Purchase"):FireServer("Sword") -- expect: missing-remote
ReplicatedStorage.Remote.Events.Buy:FireServer() -- expect: missing-remote
Remotes:WaitForChild("Runtime"):FireServer()
local price = Remotes.GetPrice:InvokeServer("Sword")
Remotes.Feedback:FireServer("hi")
Remotes.ClientOnly:InvokeServer()
Remotes.Notify.OnClientEvent:Connect(print)

local DataStoreService = game:GetService("DataStoreService") -- expect: server-api-on-client
local secrets = game.ServerStorage:FindFirstChild("Secrets") -- expect: server-api-on-client

print(Shared, price, DataStoreService, secrets)
