-- Inventory: grid of items and pets, notes and drag-and-drop
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local ServerScriptService = game:GetService("ServerScriptService") -- expect: server-api-on-client
local RunService = game:GetService("RunService")

local LocalPlayer = Players.LocalPlayer
local Remotes = ReplicatedStorage:WaitForChild("Remotes")
local gui = LocalPlayer:WaitForChild("PlayerGui"):WaitForChild("Inventory")

local slots = {}
local dragging = nil

local function refresh()
	for index, slot in ipairs(gui.Grid:GetChildren()) do
		slots[index] = slot
		slot.Visible = true
	end
end

local function beginDrag(slot)
	dragging = slot
	slot.ZIndex = 10
end

local function endDrag(target)
	if dragging and target then
		Remotes.MoveItem:FireServer(dragging.Name, target.Name)
	end
	dragging = nil
end

gui.Save.Activated:Connect(function()
	Remotes.SaveNote:FireServer(gui.Note.Text)
end)

gui.Stash.Activated:Connect(function()
	Remotes:WaitForChild("Stash"):FireServer(dragging and dragging.Name) -- expect: missing-remote
end)

gui.Drop.Activated:Connect(function()
	if dragging then
		Remotes.DropItem:FireServer(dragging.Name)
	end
end)

gui.Spawn.Activated:Connect(function()
	Remotes.SpawnItem:FireServer("Crate")
end)

gui.Equip.Activated:Connect(function()
	Remotes.EquipPet:FireServer("Dog", 1)
end)

gui.Delete.Activated:Connect(function()
	if dragging then
		Remotes.DeletePet:FireServer(dragging.Pet.Value)
	end
end)

gui.Flair.Activated:Connect(function()
	Remotes:FindFirstChild("Flair"):FireServer("sparkle") -- expect: missing-remote
end)

RunService.RenderStepped:Connect(function()
	if dragging then
		dragging.Position = UDim2.fromOffset(
			LocalPlayer:GetMouse().X,
			LocalPlayer:GetMouse().Y
		)
	end
end)

refresh()
print(ServerScriptService, endDrag, beginDrag)
