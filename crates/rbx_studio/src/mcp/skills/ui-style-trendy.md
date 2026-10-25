---
name: ui-style-trendy
description: The default Roblox UI look used by top simulator, tycoon, "Roll a X", obby and hangout games: chunky rounded-square icon buttons with stroked labels, big currency readout, colour-coded top nav, panels with a coloured title bar and studded texture, red square close, "!" badges, SALE tags, progress bars with text inside. Full Luau recipes and palette.
---

# Trendy default style

Use this when the prompt gives no strong vibe, and for simulators, tycoons, obbies and casual hangouts. Pull `ui-style` first for the shared rules. This is what players expect from a popular Roblox game in 2025-2026: bright, chunky, readable at a glance, loud about what is clickable.

## Signature traits

- **Chunky HUD buttons**: rounded squares (radius about 18 percent of size), saturated fill, 2 to 3 px dark outline, a darker strip along the bottom (the "lip") so they look pressable, a big icon filling most of the face, a short stroked label overlapping the bottom edge.
- **Left rail**: 2 columns of those buttons on the left edge, vertically centred (Index, Store, Gifts, Rebirth, Invite, Settings). Big "wide" card on top for the current featured offer.
- **Currency bottom-left**: a big icon (cash stack, coin, gem) with a huge stroked number in the currency's colour (green for cash, yellow for coins, cyan for gems) and a small green "+" button that opens the shop. A secondary line under it (Friend Boost +0%).
- **Colour-coded top nav**: 2 to 4 wide buttons across the top centre, each its own colour (Sell green, Base blue, Shop orange), with a subtle stud/grid texture and a darker bottom band.
- **Panels**: dark slate body (`#2E3A46` to `#34404C`) with a studded or diagonal texture at low opacity, a bright coloured title bar across the top with an icon on the left and a bold stroked title, and a red square close button with a white X at the top-right corner of the title bar.
- **Callouts**: red "!" notification badge on a button corner, rotated "SALE!" text in red with a white stroke, timers under items, "x2" multipliers, before/after comparisons with a big green arrow.
- **Progress bars**: full-width bright green bar, text inside ("$0/$100 Cash"), dark outline.
- **Type**: FredokaOne or BuilderSans ExtraBold for everything, white with a 2 to 3 px near-black stroke. Coloured text (green money, red warning) also gets the dark stroke.

## Palette

These hex values are a starting point. Shift them toward the game's own world and characters (`ui-style` section 4) so two games in this style never share a palette.


| Role | Fill | Lip / dark shade | Use |
| --- | --- | --- | --- |
| Green | `#5BE33B` | `#2E9A1C` | Buy, Sell, Confirm, Rebirth, progress |
| Blue | `#2FA4FF` | `#1667C2` | Navigation, Base, title bars |
| Orange | `#FF9F1A` | `#C46A00` | Shop, offers |
| Red | `#F2343F` | `#A3121C` | Close, warnings, SALE |
| Magenta | `#E93BF2` | `#9A1BA3` | Premium skip (Robux) |
| Yellow | `#FFD23A` | `#C79A00` | Coins, gold tier |
| Panel body | `#34404C` | `#222B34` | Modal background |
| Tile | `#24303A` | `#161D24` | Item slot inside panels |
| Outline | `#14181F` | | All strokes |
| Text | `#FFFFFF` | stroke `#14181F` | All labels |

The 3D world behind is usually a bright sky, so panels are opaque. No translucency.

## Recipe: helper

```lua
local function new(class, props, children)
	local inst = Instance.new(class)
	for k, v in props do inst[k] = v end
	for _, c in children or {} do c.Parent = inst end
	return inst
end

local OUTLINE = Color3.fromHex("14181F")
local FONT = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Bold)

local function stroke(thickness, mode)
	return new("UIStroke", {
		Color = OUTLINE, Thickness = thickness or 3,
		ApplyStrokeMode = mode or Enum.ApplyStrokeMode.Border,
		LineJoinMode = Enum.LineJoinMode.Round,
	})
end

local function label(text, props)
	local l = new("TextLabel", {
		BackgroundTransparency = 1, Text = text, FontFace = FONT,
		TextColor3 = Color3.new(1, 1, 1), TextScaled = true,
		Size = UDim2.fromScale(1, 1),
	}, { stroke(2.5, Enum.ApplyStrokeMode.Contextual),
	     new("UITextSizeConstraint", { MaxTextSize = 40, MinTextSize = 10 }) })
	for k, v in props or {} do l[k] = v end
	return l
end
```

## Recipe: chunky icon button with lip

The lip is a darker frame behind the face, the face sits a few pixels higher so the dark strip shows at the bottom. Pressing moves the face down onto the lip (`ui-style-motion`).

```lua
local function hudButton(name, fill, lip, iconId, text)
	local btn = new("TextButton", {
		Name = name, Text = "", AutoButtonColor = false,
		BackgroundColor3 = lip, Size = UDim2.fromScale(1, 1),
	}, {
		new("UICorner", { CornerRadius = UDim.new(0.18, 0) }),
		stroke(3),
		new("UIAspectRatioConstraint", { AspectRatio = 1 }),
	})
	local face = new("Frame", {
		Name = "Face", BackgroundColor3 = fill,
		Size = UDim2.new(1, 0, 0.9, 0), -- bottom 10% shows the lip
	}, {
		new("UICorner", { CornerRadius = UDim.new(0.18, 0) }),
		new("UIGradient", { -- top highlight, bottom slightly darker
			Rotation = 90,
			Color = ColorSequence.new(Color3.new(1, 1, 1), Color3.fromRGB(215, 215, 215)),
		}),
	})
	face.Parent = btn
	new("ImageLabel", {
		Name = "Icon", BackgroundTransparency = 1, Image = iconId,
		AnchorPoint = Vector2.new(0.5, 0), Position = UDim2.fromScale(0.5, 0.06),
		Size = UDim2.fromScale(0.72, 0.72), ScaleType = Enum.ScaleType.Fit,
	}).Parent = face
	label(text, {
		AnchorPoint = Vector2.new(0.5, 1), Position = UDim2.fromScale(0.5, 1.02),
		Size = UDim2.fromScale(1.05, 0.32), ZIndex = 3,
	}).Parent = btn
	return btn
end
```

A white-to-light-grey `UIGradient` tints the fill (white keeps the fill colour, grey darkens it; this is how it behaves in Studio, the docs do not spell out the blend). Keep the fill colour itself saturated and do not use the gradient to shift hue.

## Recipe: left rail

```lua
local rail = new("Frame", {
	Name = "Rail", BackgroundTransparency = 1,
	AnchorPoint = Vector2.new(0, 0.5), Position = UDim2.new(0, 12, 0.5, 0),
	Size = UDim2.fromScale(0.13, 0.5),
}, {
	new("UIGridLayout", {
		CellSize = UDim2.fromScale(0.47, 0.3), CellPadding = UDim2.fromScale(0.06, 0.03),
		SortOrder = Enum.SortOrder.LayoutOrder,
	}),
	new("UIAspectRatioConstraint", { AspectRatio = 0.66 }),
	new("UISizeConstraint", { MinSize = Vector2.new(110, 166), MaxSize = Vector2.new(230, 350) }),
})
```

## Recipe: currency readout

```lua
local cash = new("Frame", {
	Name = "Cash", BackgroundTransparency = 1,
	AnchorPoint = Vector2.new(0, 1), Position = UDim2.new(0, 16, 0.78, 0),
	Size = UDim2.fromScale(0.26, 0.08),
}, {
	new("UIListLayout", { FillDirection = Enum.FillDirection.Horizontal,
		VerticalAlignment = Enum.VerticalAlignment.Center, Padding = UDim.new(0, 6) }),
	new("UISizeConstraint", { MaxSize = Vector2.new(420, 70) }),
})
new("ImageLabel", { Image = CASH_ICON, BackgroundTransparency = 1, LayoutOrder = 1,
	Size = UDim2.fromScale(0.2, 1), ScaleType = Enum.ScaleType.Fit }).Parent = cash
label("$167,000", { LayoutOrder = 2, Size = UDim2.fromScale(0.62, 1),
	TextColor3 = Color3.fromHex("5BE33B"), TextXAlignment = Enum.TextXAlignment.Left }).Parent = cash
```

Put the "+" as a small `hudButton` (green, `+` glyph) after the number. Use `UIListLayout` so the "+" follows the number's width; set the number label `AutomaticSize = X` with `TextScaled = false` and a fixed `TextSize` when the number length changes a lot.

## Recipe: panel with title bar

```lua
local function panel(title, iconId, barColor)
	local root = new("Frame", {
		Name = title .. "Panel", AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.52), Size = UDim2.fromScale(0.46, 0.62),
		BackgroundColor3 = Color3.fromHex("34404C"), Visible = false,
	}, {
		stroke(3),
		new("UICorner", { CornerRadius = UDim.new(0, 6) }),
		new("UIAspectRatioConstraint", { AspectRatio = 1.45 }),
		new("UISizeConstraint", { MaxSize = Vector2.new(900, 620) }),
	})
	-- studded texture: a tiled stud image at low opacity
	new("ImageLabel", { Name = "Texture", BackgroundTransparency = 1, Image = STUD_TILE,
		ImageTransparency = 0.85, ScaleType = Enum.ScaleType.Tile,
		TileSize = UDim2.fromOffset(48, 48), Size = UDim2.fromScale(1, 1) }).Parent = root
	local bar = new("Frame", { Name = "TitleBar", BackgroundColor3 = barColor,
		Size = UDim2.fromScale(1, 0.14), Position = UDim2.fromScale(0, -0.02) }, {
		stroke(3),
		new("UIGradient", { Rotation = 90,
			Color = ColorSequence.new(Color3.new(1, 1, 1), Color3.fromRGB(190, 190, 190)) }),
	})
	bar.Parent = root
	new("ImageLabel", { BackgroundTransparency = 1, Image = iconId,
		Position = UDim2.fromScale(0.02, -0.15), Size = UDim2.fromScale(0.1, 1.3),
		ScaleType = Enum.ScaleType.Fit }).Parent = bar
	label(title, { Position = UDim2.fromScale(0.14, 0.1), Size = UDim2.fromScale(0.6, 0.8),
		TextXAlignment = Enum.TextXAlignment.Left }).Parent = bar
	return root, bar
end
```

Without a stud texture asset yet, draw one (`ui-style-icons`: a 48x48 tile with a rounded square stud, top-left highlight and bottom-right shadow) or use a diagonal two-tone pattern. Never leave the body a plain flat colour on this style.

## Recipe: red square close

Square, not round, in this style. Same lip construction as `hudButton`, red fill, radius 4 to 6 px, white "X" drawn as an icon or a FredokaOne "X" with a dark stroke. Place it at the top-right of the title bar, overlapping the edge by about a quarter of its size:

```lua
close.AnchorPoint = Vector2.new(0.5, 0.5)
close.Position = UDim2.new(1, -24, 0, 6)
close.Size = UDim2.fromScale(0.09, 0.09) -- with UIAspectRatioConstraint 1, MinSize 44x44
```

## Recipe: badges, SALE, timers

```lua
-- "!" badge on a button's top-right corner
local badge = new("Frame", { BackgroundColor3 = Color3.fromHex("F2343F"),
	AnchorPoint = Vector2.new(0.5, 0.5), Position = UDim2.fromScale(0.92, 0.08),
	Size = UDim2.fromScale(0.3, 0.3), ZIndex = 5 }, {
	new("UICorner", { CornerRadius = UDim.new(0.2, 0) }), stroke(2),
	new("UIAspectRatioConstraint", { AspectRatio = 1 }) })
label("!", {}).Parent = badge
badge.Parent = storeButton

-- tilted SALE tag
label("SALE!", { TextColor3 = Color3.fromHex("FF3B3B"), Rotation = 8, ZIndex = 5,
	AnchorPoint = Vector2.new(1, 0), Position = UDim2.fromScale(1.15, -0.12),
	Size = UDim2.fromScale(0.75, 0.24) }).Parent = storeButton
```

Badges only when something is actually new or claimable. A badge on every button is noise.

## Recipe: progress bar with text inside

```lua
local barBack = new("Frame", { BackgroundColor3 = Color3.fromHex("1C232B"),
	Size = UDim2.fromScale(0.9, 0.08) }, { stroke(3), new("UICorner", { CornerRadius = UDim.new(0, 4) }) })
local fill = new("Frame", { BackgroundColor3 = Color3.fromHex("5BE33B"),
	Size = UDim2.fromScale(0.35, 1) }, { new("UICorner", { CornerRadius = UDim.new(0, 4) }),
	new("UIGradient", { Rotation = 90,
		Color = ColorSequence.new(Color3.new(1, 1, 1), Color3.fromRGB(200, 200, 200)) }) })
fill.Parent = barBack
label("$35/$100 Cash", { ZIndex = 3 }).Parent = barBack
```

Tween `fill.Size` when progress changes; never jump.

## Recipe: before/after comparison (rebirth, upgrade)

Two dark tiles side by side ("Before" / "After"), each with a header above ("Rebirth 0" / "Rebirth 1"), item art in the middle, and the value in green at the bottom ("1x Money" / "1.5x Money"). A big green arrow icon between them. Warning line in red with a dark stroke under them ("Rebirth resets your Speed level!"), then the progress bar, then two big buttons: green primary ("REBIRTH") and magenta paid skip ("SKIP REBIRTH" with a smaller "R$199" line under it).

## Shop layout in this style

- Panel with orange title bar "Shop" plus a basket icon, red close.
- Optional tab row under the title bar (Featured, Passes, Currency) as small coloured buttons, selected one brighter and 4 px taller.
- Featured offer card full width at the top: bright gradient background, item art left, title in yellow with stroke, contents line in white, green price button with the Robux icon right.
- Grid of item tiles below (`UIGridLayout`, 3 or 4 columns): dark tile, rarity-coloured 3 px stroke, item art large, name on top, price button at the bottom (green with coin/Robux icon).
- Owned items show a grey "OWNED" button instead of the price.

## HUD layout in this style

- Top centre: colour-coded nav (2 to 3 buttons).
- Left edge: rail of icon buttons, featured offer card above it.
- Bottom-left: currency.
- Bottom-right above the jump button: active boosts as small icons with "x2" stroked labels and timers.
- Top-right (below the Roblox top bar): spin wheel or daily reward with a countdown.

## Don'ts

- No thin borders, no dark navy card, no pill buttons. Corners are chunky rounded squares.
- No unstroked text. Every label has a dark outline.
- No more than 4 saturated hues on one screen besides rarity colours.
- Do not shrink the icons to make room for text. The icon is the button; the label is secondary.
