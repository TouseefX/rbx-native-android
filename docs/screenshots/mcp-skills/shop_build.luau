--!strict
local Lighting = game:GetService("Lighting")
local StarterGui = game:GetService("StarterGui")

local Theme = {
	Surface = Color3.fromHex("14242A"),
	SurfaceRaised = Color3.fromHex("1F3B40"),
	Line = Color3.fromHex("3A5C63"),
	Text = Color3.fromHex("EDE6D6"),
	TextMuted = Color3.fromHex("A9B7BA"),
	Accent = Color3.fromHex("C89B3C"),
	AccentText = Color3.fromHex("14242A"),
	Danger = Color3.fromHex("A8432F"),
	Pad = 14,
	FontTitle = Font.new("rbxasset://fonts/families/Balthazar.json", Enum.FontWeight.Regular),
	FontBody = Font.new("rbxasset://fonts/families/BuilderSans.json", Enum.FontWeight.Medium),
	FontBold = Font.new("rbxasset://fonts/families/BuilderSans.json", Enum.FontWeight.Bold),
}

local SHELLS = 240
local ITEMS = {
	{ cat = "Rods" },
	{ name = "Cork Rod", effect = "Short cast. Your starter rod.", price = 0, owned = true },
	{ name = "Reed Rod", effect = "Casts 30% farther off the pier.", price = 120 },
	{ name = "Brass Reel Rod", effect = "Fights back less on heavy fish.", price = 450 },
	{ cat = "Bait" },
	{ name = "Worm Bait", effect = "Common fish bite sooner.", price = 15 },
	{ name = "Glow Minnow", effect = "Draws night fish after dusk.", price = 80 },
	{ cat = "Lantern" },
	{ name = "Deep Wick", effect = "Lantern light reaches 20 studs.", price = 300 },
}

local function new(class: string, props: { [string]: any }, parent: Instance?): any
	local inst = (Instance.new :: any)(class)
	for k, v in props do
		inst[k] = v
	end
	inst.Parent = parent
	return inst
end

local function padding(parent: Instance, t: number, b: number, l: number, r: number)
	new("UIPadding", {
		PaddingTop = UDim.new(0, t),
		PaddingBottom = UDim.new(0, b),
		PaddingLeft = UDim.new(0, l),
		PaddingRight = UDim.new(0, r),
	}, parent)
end

local function text(parent: Instance, s: string, size: number, color: Color3, font: Font): TextLabel
	return new("TextLabel", {
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 0),
		AutomaticSize = Enum.AutomaticSize.Y,
		Text = s,
		TextSize = size,
		TextColor3 = color,
		FontFace = font,
		TextWrapped = true,
		TextXAlignment = Enum.TextXAlignment.Left,
		TextYAlignment = Enum.TextYAlignment.Top,
	}, parent)
end

-- Backdrop world -----------------------------------------------------------
local old = workspace:FindFirstChild("ShopBackdrop")
if old then
	old:Destroy()
end
local world = new("Folder", { Name = "ShopBackdrop" }, workspace)

local function part(name: string, size: Vector3, cf: CFrame, color: Color3, material: Enum.Material): Part
	return new("Part", {
		Name = name,
		Size = size,
		CFrame = cf,
		Color = color,
		Material = material,
		Anchored = true,
		TopSurface = Enum.SurfaceType.Smooth,
		BottomSurface = Enum.SurfaceType.Smooth,
	}, world)
end

part("Water", Vector3.new(400, 1, 400), CFrame.new(0, -2.5, 0), Color3.fromHex("1F3B40"), Enum.Material.SmoothPlastic).Transparency = 0.1
for i = 0, 5 do
	local shade = if i % 2 == 0 then "6B5440" else "5E4A38"
	part("Plank" .. i, Vector3.new(3.6, 0.5, 30), CFrame.new(-9 + i * 3.7, 0, 0), Color3.fromHex(shade), Enum.Material.WoodPlanks)
end
for _, z in { -12, 12 } do
	part("Post", Vector3.new(1, 5, 1), CFrame.new(-10.5, -1.5, z), Color3.fromHex("4A3A2C"), Enum.Material.Wood)
end
for _, z in { -11, 10 } do
	local lamp = part("Lantern", Vector3.new(0.9, 1.3, 0.9), CFrame.new(-10.5, 3.6, z), Color3.fromHex("C89B3C"), Enum.Material.Metal)
	new("PointLight", { Color = Color3.fromHex("FFC66B"), Range = 22, Brightness = 2.5 }, lamp)
end

Lighting.ClockTime = 18.4
Lighting.Ambient = Color3.fromHex("3A4650")
Lighting.OutdoorAmbient = Color3.fromHex("5A6470")
local atmosphere = Lighting:FindFirstChildOfClass("Atmosphere") or new("Atmosphere", {}, Lighting)
atmosphere.Color = Color3.fromHex("8C9AA6")
atmosphere.Decay = Color3.fromHex("8C9AA6")
atmosphere.Density = 0.4
atmosphere.Haze = 1.5

local camera = workspace.CurrentCamera
if camera then
	camera.CFrame = CFrame.lookAt(Vector3.new(3, 8, -30), Vector3.new(-2, 1, 8))
end

-- Shop ---------------------------------------------------------------------
local existing = StarterGui:FindFirstChild("TackleShop")
if existing then
	existing:Destroy()
end

local gui = new("ScreenGui", {
	Name = "TackleShop",
	ResetOnSpawn = false,
	ScreenInsets = Enum.ScreenInsets.CoreUISafeInsets,
	DisplayOrder = 10,
	ZIndexBehavior = Enum.ZIndexBehavior.Sibling,
	Enabled = true,
})

local drawer = new("Frame", {
	Name = "Drawer",
	AnchorPoint = Vector2.new(1, 0),
	Position = UDim2.fromScale(1, 0),
	Size = UDim2.fromScale(0.38, 1),
	BackgroundColor3 = Theme.Surface,
	BorderSizePixel = 0,
	Active = true,
}, gui)
new("UISizeConstraint", { MinSize = Vector2.new(340, 0), MaxSize = Vector2.new(560, math.huge) }, drawer)
new("Frame", {
	Name = "EdgeLine",
	Size = UDim2.new(0, 2, 1, 0),
	BackgroundColor3 = Theme.Accent,
	BorderSizePixel = 0,
}, drawer)

local body = new("Frame", {
	Name = "Body",
	Size = UDim2.fromScale(1, 1),
	BackgroundTransparency = 1,
}, drawer)
padding(body, Theme.Pad, Theme.Pad, Theme.Pad + 4, Theme.Pad)

local header = new("Frame", {
	Name = "Header",
	Size = UDim2.new(1, 0, 0, 48),
	BackgroundTransparency = 1,
}, body)

local title = text(header, "TACKLE", 32, Theme.Text, Theme.FontTitle)
title.Size = UDim2.new(0, 130, 1, 0)
title.AutomaticSize = Enum.AutomaticSize.None
title.TextYAlignment = Enum.TextYAlignment.Center

local balance = text(header, `{SHELLS} Shells`, 20, Theme.Accent, Theme.FontBold)
balance.Name = "Balance"
balance.AnchorPoint = Vector2.new(1, 0.5)
balance.Position = UDim2.new(1, -56, 0.5, 0)
balance.Size = UDim2.new(1, -196, 1, 0)
balance.AutomaticSize = Enum.AutomaticSize.None
balance.TextXAlignment = Enum.TextXAlignment.Right
balance.TextYAlignment = Enum.TextYAlignment.Center

local close = new("TextButton", {
	Name = "Close",
	AnchorPoint = Vector2.new(1, 0.5),
	Position = UDim2.fromScale(1, 0.5),
	Size = UDim2.fromOffset(44, 44),
	BackgroundColor3 = Theme.Danger,
	BorderSizePixel = 0,
	AutoButtonColor = false,
	Text = "X",
	TextSize = 20,
	TextColor3 = Theme.Text,
	FontFace = Theme.FontBold,
}, header)
new("UICorner", { CornerRadius = UDim.new(0, 2) }, close)

local list = new("ScrollingFrame", {
	Name = "List",
	Position = UDim2.fromOffset(0, 60),
	Size = UDim2.new(1, 0, 1, -60),
	BackgroundTransparency = 1,
	BorderSizePixel = 0,
	ScrollBarThickness = 6,
	ScrollBarImageColor3 = Theme.Line,
	AutomaticCanvasSize = Enum.AutomaticSize.Y,
	CanvasSize = UDim2.new(),
}, body)
new("UIListLayout", { Padding = UDim.new(0, 0), SortOrder = Enum.SortOrder.LayoutOrder }, list)
padding(list, 0, 0, 0, 10)

for i, item in ITEMS do
	local order = i * 2
	if item.cat then
		local cap = text(list, string.upper(item.cat), 14, Theme.TextMuted, Theme.FontBold)
		cap.LayoutOrder = order
		padding(cap, 14, 6, 0, 0)
	else
		local affordable = SHELLS >= item.price
		local row = new("Frame", {
			Name = item.name,
			LayoutOrder = order,
			Size = UDim2.new(1, 0, 0, 72),
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundColor3 = Theme.SurfaceRaised,
			BorderSizePixel = 0,
		}, list)
		padding(row, 10, 10, 12, 10)

		local col = new("Frame", {
			Name = "Info",
			Size = UDim2.new(1, -124, 0, 0),
			AutomaticSize = Enum.AutomaticSize.Y,
			BackgroundTransparency = 1,
		}, row)
		new("UIListLayout", { Padding = UDim.new(0, 2), SortOrder = Enum.SortOrder.LayoutOrder }, col)
		text(col, item.name, 18, Theme.Text, Theme.FontBold).LayoutOrder = 1
		text(col, item.effect, 15, Theme.TextMuted, Theme.FontBody).LayoutOrder = 2

		local label, fill, ink
		if item.owned then
			label, fill, ink = "Owned", Theme.SurfaceRaised, Theme.TextMuted
		elseif affordable then
			label, fill, ink = `{item.price} Shells`, Theme.Accent, Theme.AccentText
		else
			label, fill, ink = `Need {item.price - SHELLS}`, Theme.Surface, Theme.Text
		end
		local buy = new("TextButton", {
			Name = "Buy",
			AnchorPoint = Vector2.new(1, 0.5),
			Position = UDim2.fromScale(1, 0.5),
			Size = UDim2.fromOffset(112, 44),
			BackgroundColor3 = fill,
			BorderSizePixel = 0,
			AutoButtonColor = false,
			Text = label,
			TextSize = 17,
			TextColor3 = ink,
			FontFace = Theme.FontBold,
		}, row)
		new("UICorner", { CornerRadius = UDim.new(0, 2) }, buy)
		if not affordable or item.owned then
			new("UIStroke", { Color = Theme.Line, Thickness = 1, ApplyStrokeMode = Enum.ApplyStrokeMode.Border }, buy)
		end
	end
	local rule = new("Frame", {
		Name = "Rule",
		LayoutOrder = order + 1,
		Size = UDim2.new(1, 0, 0, 1),
		BackgroundColor3 = Theme.Line,
		BorderSizePixel = 0,
		BackgroundTransparency = if item.cat then 1 else 0,
	}, list)
end

gui.Parent = StarterGui
