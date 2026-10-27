---
name: ui-style-cartoony
description: Cartoony Roblox UI for playful, kid-friendly games: thick dark outlines, chunky stroked display fonts (FredokaOne, LuckiestGuy), bevelled 3D buttons with a darker bottom lip, halftone and diagonal-shine fills, colour-coded actions, tilted ribbons and banners, corner ornaments, rich item art. Luau recipes, palettes and limits.
---

# Cartoony style

For pet games, simulators with a mascot, party games, anything the prompt calls cute, fun, bubbly, colourful or "for kids". Pull `ui-style` first for the shared rules, `ui-style-motion` for the bouncy motion this style needs, and `ui-style-icons` for matching icons.

Think of a mobile game sticker sheet: every element is a physical, outlined, slightly 3D object with a top light.

## Signature traits

1. **Thick dark outline on everything**: 3 to 4 px at 1080p on panels and buttons, 2.5 to 4 px on text. Outline colour is a near-black (`#151515`) or a very dark shade of the fill hue (`#3A1050` on purple). Round joins.
2. **Chunky display type**: FredokaOne for most text, LuckiestGuy for big headers and banners ("SHOP", "BUNDLE!"). White or bright yellow fill with a thick dark stroke. Big: headers 40 to 60 px at 1080p.
3. **Bevelled buttons**: a darker bottom lip (6 to 10 percent of height), a top highlight band (white at 0.6 to 0.75 transparency over the top 35 percent), a vertical gradient on the face. Buttons look like candy.
4. **Patterned fills**: halftone dots, small diamonds or diagonal shine stripes at 70 to 90 percent transparency over saturated fills. Never a plain flat colour on a big surface.
5. **Colour-coded actions**: buy green, close red, info blue, premium gold. "CLOSE" can be a big red labelled button instead of an X.
6. **Tilted ribbons and banners**: the panel title sits on a banner that sticks out of the panel's top-left, rotated -2 to -4 degrees, sometimes with folded ends. Price tags and "NEW!" stickers rotated 6 to 12 degrees.
7. **Corner ornaments**: small gold triangular brackets or rivets on panel corners, a sticker or mascot peeking over an edge.
8. **Rich item art**: item tiles show big illustrated items (chests, pets, potions), not small icons. Rarity shown by tile colour and glow.

## Palettes

These hex values are a starting point. Shift them toward the game's own world and characters (`ui-style` section 4) so two games in this style never share a palette.


Bright candy (pet sim, party game):

| Role | Fill | Lip | Stroke |
| --- | --- | --- | --- |
| Buy / confirm | `#63E12E` | `#2F9A12` | `#123D06` |
| Close / cancel | `#FF2E4D` | `#B0102A` | `#3D0610` |
| Info / nav | `#2EB6FF` | `#1270C4` | `#082E52` |
| Premium / bundle | `#FFC21A` | `#D07A00` | `#4A2600` |
| Rare / event | `#C23BFF` | `#7A16B3` | `#2E0846` |
| Panel body | `#FFFFFF` or `#F3F0FA` | | `#151515` |
| Inner well | `#E3DDEB` | | |
| Text | `#FFFFFF` | | `#151515` |

Dark arcade variant (fighting, competitive kid games): panel body `#2A2A35` with a dark halftone, blue title banner `#2E9BFF` to `#4FD0FF`, red round close `#E52530` with white X, gold corner brackets `#F5C542`.

## Recipe: helpers

```lua
local function new(class, props, children)
	local inst = Instance.new(class)
	for k, v in props do inst[k] = v end
	for _, c in children or {} do c.Parent = inst end
	return inst
end
local INK = Color3.fromHex("151515")
local BODY = Font.new("rbxasset://fonts/families/FredokaOne.json", Enum.FontWeight.Bold)
local DISPLAY = Font.new("rbxasset://fonts/families/LuckiestGuy.json")

local function outline(px, color)
	return new("UIStroke", { Thickness = px or 4, Color = color or INK,
		LineJoinMode = Enum.LineJoinMode.Round, ApplyStrokeMode = Enum.ApplyStrokeMode.Border })
end
local function text(str, font, props)
	local t = new("TextLabel", { BackgroundTransparency = 1, Text = str, FontFace = font or BODY,
		TextColor3 = Color3.new(1, 1, 1), TextScaled = true, Size = UDim2.fromScale(1, 1) }, {
		new("UIStroke", { Thickness = 3.5, Color = INK, LineJoinMode = Enum.LineJoinMode.Round }),
		new("UITextSizeConstraint", { MaxTextSize = 64, MinTextSize = 12 }) })
	for k, v in props or {} do t[k] = v end
	return t
end
```

## Recipe: bevelled candy button

Three layers: the lip (outer, darker, outlined), the face (raised, gradient), the shine (top band).

```lua
local function candyButton(fill, lip, labelText, radius)
	radius = radius or UDim.new(0.22, 0)
	local btn = new("TextButton", { Text = "", AutoButtonColor = false, BackgroundColor3 = lip,
		Size = UDim2.fromScale(0.3, 0.12) }, {
		new("UICorner", { CornerRadius = radius }), outline(4),
		new("UISizeConstraint", { MinSize = Vector2.new(96, 48) }) })
	local face = new("Frame", { Name = "Face", BackgroundColor3 = fill,
		Size = UDim2.new(1, 0, 0.86, 0) }, {
		new("UICorner", { CornerRadius = radius }),
		new("UIGradient", { Rotation = 90, Color = ColorSequence.new({
			ColorSequenceKeypoint.new(0, Color3.new(1, 1, 1)),
			ColorSequenceKeypoint.new(1, Color3.fromRGB(205, 205, 205)) }) }) })
	face.Parent = btn
	new("Frame", { Name = "Shine", BackgroundColor3 = Color3.new(1, 1, 1),
		BackgroundTransparency = 0.65, AnchorPoint = Vector2.new(0.5, 0),
		Position = UDim2.fromScale(0.5, 0.08), Size = UDim2.fromScale(0.9, 0.32) }, {
		new("UICorner", { CornerRadius = UDim.new(1, 0) }) }).Parent = face
	text(labelText, BODY, { Size = UDim2.fromScale(0.85, 0.7), AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5), ZIndex = 3 }).Parent = face
	return btn
end

local buy = candyButton(Color3.fromHex("63E12E"), Color3.fromHex("2F9A12"), "BUY")
```

For a price button, put a Robux or coin icon left of the number inside the face with a horizontal `UIListLayout` centred.

## Recipe: patterned fill

Best: author a seamless halftone or diamond tile with `ui-style-icons` (64x64 or 128x128, white shapes on transparent), upload with `store_image`, and tile it:

```lua
new("ImageLabel", { Name = "Pattern", BackgroundTransparency = 1, Image = HALFTONE_TILE,
	ImageColor3 = Color3.new(1, 1, 1), ImageTransparency = 0.8,
	ScaleType = Enum.ScaleType.Tile, TileSize = UDim2.fromOffset(64, 64),
	Size = UDim2.fromScale(1, 1), ZIndex = 1 }).Parent = card
```

No image yet: diagonal shine stripes. Wrap the card content in a `CanvasGroup` (it always clips, including rotated children), then add 2 or 3 tall white frames rotated 20 degrees at 0.8 to 0.85 transparency, one wide and one thin, placed right of centre:

```lua
local clip = new("CanvasGroup", { BackgroundColor3 = fill, Size = UDim2.fromScale(1, 1) },
	{ new("UICorner", { CornerRadius = UDim.new(0, 10) }) })
for i, spec in { { 0.62, 0.16 }, { 0.8, 0.05 } } do
	new("Frame", { BackgroundColor3 = Color3.new(1, 1, 1), BackgroundTransparency = 0.82,
		BorderSizePixel = 0, Rotation = 20, AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(spec[1], 0.5), Size = UDim2.fromScale(spec[2], 1.6) }).Parent = clip
end
```

A second, vertical `UIGradient` on the card (lighter top, deeper bottom) plus a 10 percent darker band along the bottom edge completes the look of the offer tiles in top simulator shops.

## Recipe: tilted title banner

```lua
local banner = new("Frame", { Name = "Banner", BackgroundColor3 = Color3.fromHex("2EB6FF"),
	AnchorPoint = Vector2.new(0, 0.5), Position = UDim2.new(-0.03, 0, 0, 0),
	Size = UDim2.fromScale(0.42, 0.16), Rotation = -2, ZIndex = 4 }, {
	outline(4),
	new("UIGradient", { Rotation = 90, Color = ColorSequence.new(
		Color3.fromHex("FFFFFF"), Color3.fromHex("B5D9F5")) }) })
text("Store", DISPLAY, { Size = UDim2.fromScale(0.6, 0.78), Position = UDim2.fromScale(0.08, 0.11),
	TextXAlignment = Enum.TextXAlignment.Left, ZIndex = 5 }).Parent = banner
banner.Parent = panel
```

The banner follows the UI's corner decision (rounded in a rounded UI, a small radius is fine). Add gold corner brackets (small rotated triangles drawn as icons) at the banner's ends.

## Recipe: panel

- Outer: white or dark body, `outline(4)`, `UICorner` 14 to 20 px.
- Inner well: inset 3 to 4 percent with `UIPadding`, slightly darker tone, its own 2 px stroke in a lighter grey, radius 4 px smaller than the outer.
- Optional "stacked paper" depth: a second frame behind the panel, same size, rotated 2 degrees, offset 6 px down-right, same outline.
- Close: a round red button (`CornerRadius = UDim.new(1,0)`) overlapping the top-right corner, white X icon with dark stroke, white 3 px ring then dark 4 px outline (two UIStrokes, ZIndex 1 and 2), or a big labelled red "CLOSE" banner mirroring the title banner.

## Recipe: rarity tile (shop and inventory)

```lua
local RARITY = {
	Common = { "B9C2CC", "6E7884" }, Uncommon = { "63E12E", "2F9A12" },
	Rare = { "2EB6FF", "1270C4" }, Epic = { "C23BFF", "7A16B3" },
	Legendary = { "FFC21A", "D07A00" }, Mythic = { "FF2E8A", "A3105A" },
}
```

Tile: lip colour as background with `outline(3)`, gradient face in the rarity fill, pattern on top, item art filling 70 percent, name at top in FredokaOne with stroke, price button at the bottom. Legendary and above get a second `UIStroke` outside (thickness 8, rarity colour, transparency 0.5) as a glow, and a slow rotating shine.

## Shop layout in this style

- Panel slightly off-centre, banner "SHOP" plus a basket or chest icon top-left, red "CLOSE" or round X top-right.
- Left column: one big "BUNDLE!" or "Starter Pack!" card (2x height) in gold with LuckiestGuy header and a big green price button with the Robux icon.
- Right: 2x3 grid of colourful offer tiles (pink, blue, green, cyan), each with a small green price chip with a gold coin icon.
- Section header "- GAMEPASSES -" in white with stroke between groups.
- Game pass rows: big round emblem art overlapping the left edge (x2 Strength, x2 Luck), title in a light tint of the row colour, short description, price bottom-right.

## HUD layout in this style

Same placement as `ui-style-trendy` (left rail, currency bottom-left, nav top), but buttons are round or squircle candy buttons with bigger outlines, labels in FredokaOne with heavy stroke, and the currency number in a bubble pill with its own outline.

## How much is too much

- Max 2 fonts (FredokaOne + LuckiestGuy). Bangers only for comic-style action games.
- Patterns on big surfaces only (panels, tiles, banners), not on small buttons.
- Tilt only banners, stickers and price tags. A tilted content grid is unreadable.
- Glows only on the one thing you most want clicked (the bundle, legendary items).
- Outline thickness consistent: pick 4 px for containers and 3 px for small parts and keep it everywhere.

## Don'ts

- No thin 1 px strokes. No unstroked white text on bright fills.
- No sleek glass, no blur, no thin fonts.
- No flat single-colour buttons without a lip or gradient.
- No pure black panels with a neon border (that is the anti-pattern).
