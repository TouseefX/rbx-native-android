---
name: ui-style-sleek
description: Sleek minimal Roblox UI for calm, premium or mature games and chill hangouts: world blur behind translucent panels, thin light type, line icons, generous spacing, very few colours, subtle short motion. Luau recipes, palettes, and how it differs from the generic dark-card look.
---

# Sleek / minimal style

For prompts saying clean, modern, minimal, premium, chill, aesthetic, and for hangouts, story games, showcases and settings screens of older-audience games. Pull `ui-style` first, then `ui-style-motion` (sleek motion section) and `ui-style-icons` (line icons).

Sleek is the hardest style to do without falling into the anti-pattern, because both are "dark and simple". The difference: sleek is **translucent over a blurred world**, uses **thin light type**, **no coloured borders**, **no gradients**, and gets its polish from spacing and alignment, not decoration.

## Signature traits

1. **Frosted backdrop**: when a menu opens, a `BlurEffect` in `Lighting` blurs the 3D world (size 16 to 24) and a full-screen black frame dims it slightly (transparency 0.6 to 0.75). The world stays visible as soft colour.
2. **Translucent rows, not cards**: rows are white at 0.85 to 0.9 transparency (or black at 0.6 to 0.7) with tiny or no corner radius (0 to 4 px). No strokes, or a 1 px white stroke at 0.85 transparency at most.
3. **Thin, light type**: Montserrat or BuilderSans at Regular/Medium for labels, Light for big numbers, SemiBold only for the screen title. White `#F5F7FA` for primary text, `#C9CED6` or white at 0.35 transparency for secondary text. No text stroke.
4. **Line icons**: 1.5 to 2 px strokes on a 24 grid, round caps, single colour, matching text colour. Right-aligned in rows for state ("On", "Medium", an icon).
5. **Space**: 24 to 48 px outer margins, 8 to 12 px between rows, rows 44 to 52 px tall, a narrow centred column (40 to 45 percent of screen width) or a side sheet.
6. **Very few colours**: monochrome plus at most one accent (a soft blue `#7FB2FF`, mint `#7FE0C2` or warm `#FFB37F`) used for the selected state and the primary action only.
7. **Plain close**: a thin X icon top-left or top-right, no button background.

## Palette

These hex values are a starting point. Shift them toward the game's own world and characters (`ui-style` section 4) so two games in this style never share a palette.


| Role | Value |
| --- | --- |
| Dim overlay | `#000000` at transparency 0.7 |
| Row fill (light glass) | `#FFFFFF` at transparency 0.86 |
| Row fill hover | `#FFFFFF` at transparency 0.78 |
| Row fill (dark glass, bright worlds) | `#0E1116` at transparency 0.45 |
| Primary text | `#F5F7FA` |
| Secondary text | `#C9CED6` |
| Accent | one of `#7FB2FF`, `#7FE0C2`, `#FFB37F` |
| Destructive | `#FF7F7F` text only |

Light theme variant (hangouts in daytime): row `#FFFFFF` at 0.15 transparency, text `#1C1F24`, secondary `#6B7280`, soft shadow frame `#000000` at 0.9 transparency offset 4 px down.

## Recipe: frosted open

```lua
local Lighting = game:GetService("Lighting")
local TweenService = game:GetService("TweenService")

local blur = Lighting:FindFirstChild("MenuBlur") or Instance.new("BlurEffect")
blur.Name = "MenuBlur"
blur.Size = 0
blur.Parent = Lighting

local dim = Instance.new("Frame")
dim.BackgroundColor3 = Color3.new(0, 0, 0)
dim.BackgroundTransparency = 1
dim.Size = UDim2.fromScale(1, 1)
dim.Parent = menuGui -- a ScreenGui with ScreenInsets None so it covers the full screen

local info = TweenInfo.new(0.25, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local function open()
	TweenService:Create(blur, info, { Size = 20 }):Play()
	TweenService:Create(dim, info, { BackgroundTransparency = 0.7 }):Play()
end
```

Only the largest `BlurEffect.Size` in `Lighting` applies, so reuse one named instance instead of adding more. Put the dim frame in a separate ScreenGui with `ScreenInsets = None` so it reaches the screen edges, and the content in one with `CoreUISafeInsets`.

## No card: the screen is the container

Good sleek menus have no panel behind the content: the blurred, dimmed world is the background and rows float on it in a centred column (settings, player menu) or a full-height side sheet runs along one edge (shop detail). Do not wrap the content in a rounded dark card in the middle of the screen; with a card it reads as the generic dark-card UI no matter how clean the rows are. If you need grouping, use spacing and a small secondary-colour section label, not a box.

For a static preview (a screenshot of the edit view), also leave a `BlurEffect` with `Size` about 20 in `Lighting` while you show the open menu, or the frosted look cannot be judged; remove it or set `Size` to 0 before shipping so the world is sharp when no menu is open.

## Recipe: row

```lua
local function new(class, props, children)
	local inst = Instance.new(class)
	for k, v in props do inst[k] = v end
	for _, c in children or {} do c.Parent = inst end
	return inst
end
local REG = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Regular)
local MED = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Medium)

local function row(title, value, order)
	local r = new("TextButton", { Text = "", AutoButtonColor = false, LayoutOrder = order,
		BackgroundColor3 = Color3.new(1, 1, 1), BackgroundTransparency = 0.86,
		BorderSizePixel = 0, Size = UDim2.new(1, 0, 0, 48) }, {
		new("UICorner", { CornerRadius = UDim.new(0, 3) }),
		new("UIPadding", { PaddingLeft = UDim.new(0, 16), PaddingRight = UDim.new(0, 16) }) })
	new("TextLabel", { BackgroundTransparency = 1, Text = title, FontFace = REG, TextSize = 17,
		TextColor3 = Color3.fromHex("F5F7FA"), TextXAlignment = Enum.TextXAlignment.Left,
		Size = UDim2.fromScale(0.7, 1) }).Parent = r
	new("TextLabel", { BackgroundTransparency = 1, Text = value, FontFace = REG, TextSize = 16,
		TextColor3 = Color3.fromHex("C9CED6"), TextXAlignment = Enum.TextXAlignment.Right,
		AnchorPoint = Vector2.new(1, 0), Position = UDim2.fromScale(1, 0),
		Size = UDim2.fromScale(0.3, 1) }).Parent = r
	return r
end

local column = new("Frame", { BackgroundTransparency = 1, AnchorPoint = Vector2.new(0.5, 0),
	Position = UDim2.fromScale(0.5, 0.25), Size = UDim2.fromScale(0.42, 0.6) }, {
	new("UIListLayout", { Padding = UDim.new(0, 8), SortOrder = Enum.SortOrder.LayoutOrder }),
	new("UISizeConstraint", { MinSize = Vector2.new(320, 0), MaxSize = Vector2.new(620, 2000) }) })
```

Fixed `TextSize` (16 to 18 body, 24 to 28 title) inside a root `UIScale` reads better than `TextScaled` here: thin text that scales unevenly looks sloppy.

## Recipe: action grid (player menu, profile)

Rows with a line icon at the left (20 px, 16 px from the edge), label 14 px after it. Two half-width rows side by side via a horizontal `UIListLayout` with `HorizontalFlex = SpaceBetween`, or `UIGridLayout` with 2 columns. Avatar at the top in a circle (`UICorner` radius `UDim.new(1, 0)`) with a white ring at 0.7 transparency, display name in SemiBold 22 px, username and id in secondary text 14 px.

## Recipe: primary button

Sleek primary: solid accent or solid white fill (no gradient), dark text, Medium weight. Corners follow the one decision from `ui-style`: either every box small-rounded (rows 4, tiles and buttons 6 to 8, panes 12) or every box sharp. A sharp detail pane next to rounded tiles is the classic sleek mistake. Secondary: the translucent row style. No lip, no bevel, no stroke.

```lua
local primary = new("TextButton", { Text = "Purchase", FontFace = MED, TextSize = 17,
	TextColor3 = Color3.fromHex("0E1116"), BackgroundColor3 = Color3.fromHex("F5F7FA"),
	AutoButtonColor = false, Size = UDim2.new(1, 0, 0, 48) }, {
	new("UICorner", { CornerRadius = UDim.new(0, 4) }) })
```

## Shop in this style

- Full-screen frosted backdrop, title "Store" top-centre in SemiBold 26 px, thin X top-left.
- Category tabs as plain text (secondary colour), selected tab in primary text with a 2 px accent underline that slides between tabs.
- Items as large tiles with the item art on a translucent square (no border), name in Medium 16 px under the tile, price in secondary 14 px with a small line Robux or coin icon.
- Selected item opens a side sheet on the right with a big render (ViewportFrame), description in secondary text and one primary button.

## HUD in this style

Minimal: a small currency readout top-right (line coin icon + number, Medium 18 px, no background or a black 0.6 transparency chip with radius 999), a column of 3 to 4 icon-only buttons on the left edge (44 to 52 px, black at 0.55 transparency, white line icon, tooltip label appears on hover), nothing else. Hide UI that is not needed right now.

## Motion

Short and soft: 0.15 to 0.25 s, Quad or Sine Out. Fades and 8 to 16 px slides, no scaling pops, no bounce. Hover: row transparency 0.86 to 0.78 in 0.12 s. Details in `ui-style-motion`.

## Don'ts (this is where sleek turns into the anti-pattern)

- No coloured borders, especially not purple or blue. No 2 px outline around the panel.
- No opaque navy card floating over an unblurred world.
- No gradients on panels or buttons.
- No bold-everything: if the whole screen is Bold, it is not sleek.
- No centred title plus centred list plus centred pill button inside a bordered box.
- No cartoony fonts, no strokes on text, no lips.
- Do not use sleek for a kids' simulator because it "looks clean". Match the genre first.
