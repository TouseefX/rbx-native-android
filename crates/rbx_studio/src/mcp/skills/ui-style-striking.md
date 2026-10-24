---
name: ui-style-striking
description: High-energy Roblox UI for event shops, battle passes, limited offers and hype screens: slanted italic headers, sparkles and lightning, diagonal compositions, bold saturated gradients with halftone, strong glows, offer cards fanned at angles, countdown badges. Luau recipes and limits.
---

# Striking / high-energy style

For one screen that has to feel like an event: a seasonal shop, a battle pass, a "STARPASS", a limited bundle, a level-up or reward reveal. The prompt says hype, epic, insane, event, season, limited, or the game already uses loud art. Usually a kid-facing game (pull `ui-style-cartoony` for base pieces). Do not use it for the whole HUD; players tire of it in seconds. Pull `ui-style` first.

## Signature traits

1. **Slanted italic headers**: LuckiestGuy or Bangers with `Enum.FontStyle.Italic` where the family has it, plus `Rotation = -4` to `-8` on the header. White-to-pink or white-to-gold gradient fill on the text, thick dark purple stroke, a second lighter stroke as glow.
2. **Diagonal composition**: a big lightning or slash shape cutting down one side, tabs on a slanted bar, cards and price buttons skewed or rotated a few degrees. Nothing lines up on a boring grid.
3. **Saturated gradients with halftone**: backgrounds go violet `#7B2CFF` to magenta `#D04BFF`; each card has its own gradient (pink, sky blue, orange) with a halftone or diamond pattern and radial light rays behind the item.
4. **Strong glows**: cards and the selected tab have an outer glow (wide `UIStroke` with transparency 0.3 to 0.6 in a light tint), price buttons glow lime.
5. **Sparkles, stars, lightning, clouds**: 4-point sparkle stars scattered around (5 to 10, varied sizes, slight rotation), stylised clouds or swirls in a corner, a big lightning bolt.
6. **Fanned cards**: 3 offer cards at different rotations and heights (for example -4, 0, +3 degrees, the middle one raised), each with a ribbon label on a slanted tab above it ("STARPASS", "SUMMER BUNDLE").
7. **Urgency badges, done honestly**: a rotated diamond "SEASON ENDS" badge with a real countdown ("12D 7H"). Only show a timer that is true.

## Palette

| Role | Value |
| --- | --- |
| Background gradient | `#6A2BFF` to `#B13CFF` with `#8C4DFF` star pattern at 0.8 transparency |
| Header text gradient | `#FFFFFF` to `#FF9BF0` |
| Header stroke | `#3B0A6B` |
| Card A | `#E040FB` to `#FF8AE2` |
| Card B | `#3EA6FF` to `#9FD8FF` |
| Card C | `#FF9A2E` to `#FFD27A` |
| Price button | `#9BE81C` to `#C8FF4A`, stroke `#3E6B00`, glow `#E6FF7A` |
| Sparkles | `#FFFFFF` with a soft white glow |
| Accent clouds | `#F5DEB3` with `#3B0A6B` outline |

## Recipe: slanted gradient header

```lua
local function new(class, props, children)
	local inst = Instance.new(class)
	for k, v in props do inst[k] = v end
	for _, c in children or {} do c.Parent = inst end
	return inst
end

local header = new("TextLabel", {
	Text = "STORE", BackgroundTransparency = 1, TextScaled = true, Rotation = -6,
	FontFace = Font.new("rbxasset://fonts/families/LuckiestGuy.json", Enum.FontWeight.Regular, Enum.FontStyle.Italic),
	TextColor3 = Color3.new(1, 1, 1), Position = UDim2.fromScale(0.03, 0.03), Size = UDim2.fromScale(0.22, 0.13),
}, {
	new("UIGradient", { Rotation = 90, Color = ColorSequence.new(
		Color3.fromHex("FFFFFF"), Color3.fromHex("FF9BF0")) }),
	new("UIStroke", { Thickness = 5, Color = Color3.fromHex("3B0A6B"), ZIndex = 2,
		LineJoinMode = Enum.LineJoinMode.Miter }),
	new("UIStroke", { Thickness = 10, Color = Color3.fromHex("E9B8FF"), Transparency = 0.5, ZIndex = 1 }),
})
```

If a family has no italic face, Roblox may fake the slant or fall back to upright; the `Rotation` still gives the diagonal feel. Check it in Studio.

## Recipe: glowing offer card

```lua
local function offerCard(colA, colB, ribbonText, rotation, raise)
	local card = new("Frame", { BackgroundColor3 = Color3.new(1, 1, 1), Rotation = rotation,
		AnchorPoint = Vector2.new(0.5, 0.5), Size = UDim2.fromScale(0.25, 0.52),
		Position = UDim2.fromScale(0.5, 0.48 - raise) }, {
		new("UIGradient", { Rotation = 135, Color = ColorSequence.new(colA, colB) }),
		new("UIStroke", { Thickness = 4, Color = Color3.fromHex("2A0A4A"), ZIndex = 2,
			LineJoinMode = Enum.LineJoinMode.Miter }),
		new("UIStroke", { Thickness = 14, Color = colB, Transparency = 0.55, ZIndex = 1 }),
		new("UIAspectRatioConstraint", { AspectRatio = 0.82 }) })
	-- radial light rays + halftone: ImageLabels from ui-style-icons, ImageTransparency 0.6 to 0.85
	local ribbon = new("Frame", { BackgroundColor3 = Color3.new(1, 1, 1), Rotation = -3,
		AnchorPoint = Vector2.new(0.5, 1), Position = UDim2.fromScale(0.6, 0.02),
		Size = UDim2.fromScale(0.7, 0.11) }, {
		new("UIStroke", { Thickness = 3, Color = Color3.fromHex("2A0A4A") }) })
	ribbon.Parent = card
	return card
end
```

Fan them: `offerCard(pinkA, pinkB, "STARPASS", -4, 0.03)`, `offerCard(blueA, blueB, "SUMMER BUNDLE", 0, 0)`, `offerCard(orangeA, orangeB, "ROOKIE BUNDLE", 3, 0)`, placed at X 0.3, 0.6, 0.85. The first card is the hero: slightly bigger and raised.

Rotated frames ignore `ClipsDescendants` unless `StarterGui.ClipsDescendantsSupportsRotation` is on, and even then a rotated parent cannot clip. If card art must be clipped, put it in a `CanvasGroup` and rotate the CanvasGroup.

## Recipe: sparkles

Draw one 4-point star SVG (`ui-style-icons`), upload once, scatter 6 to 10 copies with sizes 16 to 64 px, rotations 0 to 30, transparency 0 to 0.3. Animate 2 or 3 of them with a slow scale and transparency pulse (1.2 to 2 s, Sine InOut, reverses, infinite repeat). Never animate all of them at once.

## Layout

- Header top-left, slanted. Tabs top-centre in a row on the background, white Bold italic at 0.4 transparency for unselected, solid white with glow for selected.
- Three fanned cards centre, price buttons under each card (not on them), wide and glowing.
- Currencies bottom-left in a row: icon + number with a dark italic stroke.
- Back button bottom-right: a curved arrow icon with "BACK" in slanted type.
- Lightning bolt or slash down the left edge behind the header, clouds in two opposite corners.

## How much is too much

- Striking is for one screen at a time, opened on purpose. The HUD stays trendy or cartoony.
- At most 3 gradient hues for cards plus the background. Price buttons share one colour.
- Max 10 sparkles, max 2 animated at a time.
- Glow on cards and the main price button only. Text stays legible: the item name and price must read at phone size.
- Countdown and "limited" badges only when true. Fake urgency is a dark pattern (see `growth-monetization`).

## Don'ts

- No upright, centred, unstroked headers. No straight grid of identical cards.
- No dark navy background with a purple border (the anti-pattern in disguise: striking uses bright saturated fills, not dark cards).
- No striking style on a military, horror or sleek game.
