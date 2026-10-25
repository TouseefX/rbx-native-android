---
name: ui-design
description: Visual design of in-game Roblox UI: avoiding the generic AI look, deriving palette, type and shape from the specific game, genre layouts, scale vs offset, safe areas, and a worked shop example. Pull it before building or restyling any ScreenGui.
---

# In-game UI visual design

Pull `ux-patterns` for flows and feedback, `accessibility` for contrast, text size and reduced motion, and `animation` for tweens. This file is about how the UI looks and how it is laid out.

## 1. How generic AI UI looks, and why it reads wrong

You have seen the default output: a centred rounded card, a blue-violet gradient, a pill button, a header like "Welcome to the Shop!", emoji for icons, a UIStroke and a shadow on everything, 12px corners on every element. Players read this as a template, not as part of the game, for three reasons:

1. **It ignores the world.** A horror game, a candy tycoon and a military shooter get the same colours and shapes. Players expect the UI to belong to the game's art (Roblox's own UI guidance calls the deliberate set of shape, colour and style choices a "visual language", source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
2. **It has no hierarchy.** Everything is centred, the same size, the same weight. Nothing says "this is the thing to press".
3. **It uses no assets.** Shipped Roblox games use illustrated icons and custom frames. Emoji and flat rectangles look unfinished next to them.

Typical tells to remove:

| Tell | Replace with |
|---|---|
| Purple, indigo or blue-violet gradient background | Flat or very subtle tonal colour taken from the game's world |
| One centred rounded card for everything | Layout that follows the genre (section 6), panels attached to edges or to the thing they describe |
| Emoji as icons or bullets | ImageLabel with a real asset id, or a plain text label, or a simple shape |
| Same `UICorner` radius everywhere | One decision per surface type (section 7), often 0 to 6px, sometimes none |
| UIStroke + shadow + gradient on every element | One depth cue at most per element, often none |
| Glassmorphism (blurred translucent panels) | Opaque or near-opaque surfaces |
| Every button a full-width pill | Primary action visually heavier; secondary actions lighter; close buttons square (section 8) |
| Filler copy ("Discover amazing items!") | Short, specific, in the game's voice (section 9) |
| Identical layout for every game | Start from the place's existing UI and genre |

### Checklist: run it before you finish

- [ ] I looked at the place first (existing ScreenGuis, Lighting, terrain and part colours, fonts already used) and matched them. If the place is empty, I asked or chose a palette from the genre and said so.
- [ ] Palette is 2 to 3 hues plus neutrals, with exactly one accent used for the primary action and key numbers.
- [ ] No gradient unless the game's art uses them; no purple or blue-violet unless the world is purple or blue-violet.
- [ ] At most two fonts, with a clear size ramp (title, label, body, caption).
- [ ] Corner radius, stroke and shadow decisions are deliberate and not identical on every object.
- [ ] No emoji anywhere in text or icons. Icons are ImageLabels with real asset ids, or absent.
- [ ] Layout follows the genre's HUD conventions, and nothing sits in the mobile thumb zones.
- [ ] All sizes are scale-based with constraints; checked at a small phone (about 640x360), a tablet and a 1920x1080 desktop.
- [ ] Interactive ScreenGuis use `ScreenInsets = CoreUISafeInsets` (default) and do not overlap the topbar.
- [ ] Copy is short, concrete, and contains no "Welcome to", "Discover", "Amazing", "Unlock the power".
- [ ] Text and non-text contrast pass (see `accessibility`); state is never colour alone.
- [ ] Hover, press and disabled states exist for every button.

## 2. Derive the look from the game

Do this in order. Do not skip to a default.

1. **Inspect the place.** Read existing ScreenGuis, StarterGui, fonts and colours in use, `Lighting` (Ambient, ColorShift, Atmosphere colour), `Terrain` water/grass colours, and the dominant `Part.Color` / material of the main environment. New UI must look like it came with these.
2. **Name the genre and mood** in one line ("post-apocalypse scavenger tycoon, dusty, practical"). Every later choice should be defensible against that line.
3. **Pick the palette.**
   - 2 or 3 hues sampled from the world or brand (a rust orange from the terrain, a steel blue-grey from the buildings).
   - Neutrals: one dark surface, one lighter surface, one text colour, one muted text colour. Tint neutrals slightly toward the main hue instead of pure grey or pure black.
   - One accent, used only for the primary action, the focused item and the key number. If everything is accented, nothing is.
   - Semantic colours (positive, warning, danger) stay consistent across the game. Roblox's own guidance cites keeping "Health" green everywhere and unaffordable prices red (source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
4. **Write the tokens first** (a `Theme` ModuleScript with `Color3`, `Font`, spacing, radius). All UI reads from it so a restyle is one edit.
5. **Check contrast** of text on each surface before building (see `accessibility`).

```lua
--!strict
-- Theme.lua: one place for every visual decision
local Theme = {
	Surface = Color3.fromHex("1B1612"),
	SurfaceRaised = Color3.fromHex("2A221B"),
	Line = Color3.fromHex("4A3C2E"),
	Text = Color3.fromHex("EFE6D6"),
	TextMuted = Color3.fromHex("B3A58F"),
	Accent = Color3.fromHex("D9822B"), -- primary action and key numbers only
	Danger = Color3.fromHex("C4452F"),
	Radius = UDim.new(0, 4),
	Pad = 12,
	FontTitle = Font.new("rbxasset://fonts/families/Balthazar.json", Enum.FontWeight.Bold),
	FontBody = Font.new("rbxasset://fonts/families/BuilderSans.json", Enum.FontWeight.Medium),
}
return Theme
```

(The hex values are illustrative for a dusty scavenger game, not a default to reuse.)

## 3. Typography

- **Hierarchy beats variety.** Roblox's guidance: titles larger and bolder than body, highlight key items or stats with colour or weight, and prioritise legibility when choosing a font (source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
- **Fonts.** `Enum.Font.BuilderSans`, `BuilderSansMedium`, `BuilderSansBold` and `BuilderSansExtraBold` exist (source: https://create.roblox.com/docs/reference/engine/enums/Font). `Gotham*` is removed and maps to Montserrat. `FredokaOne` is still available for chunky, friendly games. For more choice use `Font.new("rbxasset://fonts/families/<Name>.json", weight, style)` with families from the font list (for example Balthazar, Amatic SC, Arimo, Accanthis ADF Std; source: https://create.roblox.com/docs/reference/engine/datatypes/Font). Set `TextLabel.FontFace`, not the legacy `Font`, when you need a weight.
- **Match the game.** Rounded geometric fonts for cartoon simulators, a serif or condensed face for fantasy and survival, a monospace face for sci-fi terminals. Builder Sans is a neutral choice when the game has no strong identity.
- **One display font at most.** Use the display face for titles and big numbers, a plain face for everything else.
- **Rough ramp (px at 1080p, scaled by UIScale):** title 28 to 36, section label 20 to 24, body 16 to 18, caption 14. Do not go below 14 for anything players must read on a phone (see `accessibility`).
- **Do not use `TextScaled` for body text.** It ignores `PreferredTextSize` (source: https://create.roblox.com/docs/production/publishing/accessibility) and makes text sizes drift between labels. Use fixed `TextSize`, `TextWrapped`, `AutomaticSize` and a `UIScale` for viewport scaling. If you must use `TextScaled` (a short number in a fixed badge), add a `UITextSizeConstraint` with Min and Max (source: https://create.roblox.com/docs/ui/size-modifiers).
- **Rich text** (`RichText = true`) for inline emphasis: `<b>`, `<font color="#D9822B">`. Never build a sentence out of five separate labels (source: https://create.roblox.com/docs/ui/rich-text).
- **Numbers.** Abbreviate big values (1.2K, 3.4M) and keep the unit or icon in a fixed slot so digits changing do not shift the layout.

## 4. Scale vs offset, constraints, layout objects

Rules (source: https://create.roblox.com/docs/ui/position-and-size and https://create.roblox.com/docs/ui/size-modifiers):

- **Position and big panel sizes: scale.** A panel at `UDim2.fromScale(0.5, 0.5)` with `AnchorPoint = Vector2.new(0.5, 0.5)` stays centred on any screen.
- **Small fixed things: offset.** Icon sizes, paddings, stroke thickness, 1px dividers. Padding in scale looks wrong on wide screens.
- **Mix them on purpose:** `UDim2.new(0.3, 0, 0, 48)` is "30% wide, 48px tall".
- **Always set `AnchorPoint`** when you anchor to an edge or centre. Top-right: `AnchorPoint (1, 0)`, `Position (1, -16, 0, 16)`.
- **`UIAspectRatioConstraint`** on anything that must keep its shape (icons, item cards, a 16:9 panel). Do not combine it with a `UIListLayout` controlling the same object: the constraint wins and breaks the layout.
- **`UISizeConstraint`** with `MinSize` and `MaxSize` so a panel is neither tiny on phones nor a billboard on a 4K monitor.
- **`UIScale`** for global scaling by viewport (compute from `workspace.CurrentCamera.ViewportSize`, clamp it). It scales children, including strokes and text.
- **Layout objects over manual positions:** `UIListLayout` (rows and columns, `Padding`, `SortOrder = LayoutOrder`), `UIGridLayout` (fixed `CellSize`), `UIPadding`, `UIFlexItem` (`FlexMode`, `GrowRatio`, `ShrinkRatio`) for rows that share space, and `AutomaticSize` so panels fit their content (source: https://create.roblox.com/docs/ui/list-flex-layouts). With `AutomaticSize` the `Size` becomes the minimum size.
- **`ScrollingFrame`:** `AutomaticCanvasSize = Y`, `CanvasSize = UDim2.new()`, plus a list or grid layout.
- **`ZIndexBehavior = Sibling`** (default on new ScreenGuis) so children render above their parents and `ZIndex` only orders siblings.
- **9-slice** (`ImageLabel.ScaleType = Slice` with `SliceCenter`) for custom borders and frames that stretch to unknown text length or localised strings (source: https://create.roblox.com/docs/ui/9-slice).

Responsive frame, in code:

```lua
--!strict
local function makePanel(parent: Instance): Frame
	local panel = Instance.new("Frame")
	panel.Name = "Panel"
	panel.AnchorPoint = Vector2.new(0.5, 0.5)
	panel.Position = UDim2.fromScale(0.5, 0.5)
	panel.Size = UDim2.fromScale(0.6, 0.7) -- scale: follows the screen
	panel.BackgroundColor3 = Color3.fromHex("1B1612")
	panel.BorderSizePixel = 0

	local ratio = Instance.new("UIAspectRatioConstraint")
	ratio.AspectRatio = 1.6 -- width / height
	ratio.Parent = panel

	local limits = Instance.new("UISizeConstraint")
	limits.MinSize = Vector2.new(420, 260) -- readable on a small phone
	limits.MaxSize = Vector2.new(1100, 690) -- not a billboard on 4K
	limits.Parent = panel

	panel.Parent = parent
	return panel
end
```

## 5. Safe areas and the topbar

- `ScreenGui.ScreenInsets` defaults to `CoreUISafeInsets`: content stays clear of device cutouts and the Roblox topbar buttons. Use it for anything interactive. `DeviceSafeInsets` ignores the topbar. `None` is only for non-interactive backgrounds. `TopbarSafeInsets` gives the strip beside the topbar controls (source: https://create.roblox.com/docs/ui/on-screen-containers).
- Do not hard-code a 36px top offset. Prefer the ScreenGui inset modes. `GuiService:GetGuiInset()` and `GuiService.TopbarInset` (a `Rect`) exist when you need numbers (source: https://create.roblox.com/docs/reference/engine/classes/GuiService).
- Use `ScreenGui.DisplayOrder` to layer HUD under menus. Set `ResetOnSpawn = false` for persistent UI.
- On phones the default controls occupy parts of the bottom-left and bottom-right corners; keep important info and buttons out of them (source: https://create.roblox.com/docs/ui/position-and-size). Details in `ux-patterns`.

## 6. Layout per genre

Roblox's docs show prioritisation, contextual UI and consistency in games such as Spellbound RPG (minimal, important on mobile), Super Striker League (buttons swap with context), Jailbreak, Dragon Adventures and Tower Defense Simulator (size and a colour stripe to rank shop items) (source: https://create.roblox.com/docs/production/game-design/ui-ux-design). The placements below are common practice I have observed, not documented rules. Verify against the place's existing UI first.

| Genre | HUD conventions (observed in) | Look |
|---|---|---|
| Simulators (Pet Simulator 99, Bee Swarm Simulator) | Currencies stacked at the top or left edge, row of menu buttons down the left side, progress or capacity bar near top or bottom centre. Dense but icon-led. | Bright saturated palette, thick chunky type, illustrated icons, bold outlines that match the art. Buttons large and glossy. |
| Tycoons | Cash top of screen, upgrade or build panel on a side, prompts in world (ProximityPrompt, BillboardGui). | Friendly, flat or lightly shaded; strong colour per currency. |
| Horror (DOORS) | Almost no HUD; inventory hotbar small near bottom, interaction prompts in the world; panels appear only when needed. | Desaturated, low-contrast chrome with a single warm accent, serif or period type, no bright rounded bubbles. |
| RPG / anime fighters (Blox Fruits) | Health and energy bars top-left, hotbar of skills along the bottom, quest tracker along an edge, menu button row at one side. | Stylised frames, gold or elemental accent, ability icons with cooldown overlays. |
| Obbies | Stage counter and timer at the top, minimal elsewhere; checkpoint toast. | Simple, bold numerals, high contrast over varied backgrounds. |
| Social / roleplay (Brookhaven) | Menu buttons at an edge, emote and chat access, big panels for houses and vehicles that fully replace the HUD while open. | Clean, neutral, bright, lots of illustrated cards. |
| Shooters (Arsenal, Rivals) | Health and ammo bottom corners or bottom centre, kill feed top right, crosshair centre, minimal clutter. | Sharp edges, small caps type, monochrome plus team colours. |

Keep the HUD to what the player needs this second (Roblox's "hierarchy of information"); move everything else into menus.

## 7. Corners, strokes, shadows, gradients

- **Corner radius is part of the art style.** Sharp or 2 to 4px for military, sci-fi, horror. Larger radii for toy-like or cartoon games. Vary by surface: panels, buttons and chips do not all share one radius. A circle (`UDim.new(0.5, 0)`) only for things that are circles (avatar thumbnail, round icon button).
- **Often omit `UICorner`.** If the art uses hard edges or custom 9-slice frames, use those and no `UICorner`.
- **`UIStroke`:** use for a real reason: outlining text over busy 3D backgrounds (`ApplyStrokeMode = Contextual`, thickness 1 to 2), or to separate a selected state. Not on every frame. One stroke colour from the palette.
- **Shadows:** Roblox has no box shadow. Fake ones (dark offset frames) are only worth it if the game's art has them. If you add one, do it consistently or not at all.
- **`UIGradient`:** only for a reason the art supports (health bar shine, sky tint, metallic frame). Keep it subtle (a few percent lightness), not a hue shift.
- **Buttons need to look pressable** (container, highlight or shadow suggesting depth) but states carry the weight (see `ux-patterns`) (source: https://create.roblox.com/docs/production/game-design/ui-ux-design).

## 8. Icons and imagery

- Use `ImageLabel` / `ImageButton` with real asset ids (`rbxassetid://...`) from the game's art. Keep icon sets visually consistent (same line weight, same palette). Related stats can share a palette so players learn them (source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
- If you do not have asset ids, do not invent them and do not use emoji. Use plain text labels, or a simple shape drawn with Frames (a square, a bar, a notch), or leave the icon out.
- Never reference an id you have not verified exists and is accessible to the place.
- Close buttons: a square with an X, top right of the dialog, in the consistent danger colour (a convention seen in Winds of Fortune, Dragon Adventures, BotClash Simulator and DOORS, source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
- Set `ImageLabel.ResampleMode = Pixelated` for pixel art; keep `ScaleType` explicit (Fit, Crop or Slice).

## 9. Copy

- Labels are nouns or verbs, 1 to 3 words: "Upgrade", "Equip", "Sell all", "Backpack".
- State specifics: "Needs 500 Scrap" not "Not enough currency".
- Match the game voice (dry, cheerful, ominous), not a marketing voice. No "Welcome to", "Discover", "Amazing", "Unlock the power of".
- No sentence-long descriptions in tiles. Details on selection or hover.
- No emoji, no stacked exclamation marks.
- Leave room for translation (translated strings are often noticeably longer than English; see `accessibility`).

## 10. Worked example: a shop

Game: a scavenger tycoon in a rusted junkyard. Currency is Scrap.

**Generic version (do not build):** centred rounded card, indigo gradient, title "Welcome to the Shop! Discover amazing items", 3x3 grid of identical pill-cornered tiles with emoji icons, a purple "Buy" pill on every tile, a drop shadow on the card, X in the corner with a circle.

**Game-specific version:**

- Palette from the junkyard: soot background, oxidised orange accent, bone-white text.
- Panel attaches to the right edge like a workbench drawer, 38% of the width, full height minus the topbar inset, so the world stays visible and playable on the left.
- Header: "WORKBENCH" in the display font, left aligned, with the Scrap balance on the same row in the accent colour.
- Rows instead of a card grid: each item is a wide row (icon placeholder, name, one-line effect, price). Reads faster, localises better, and differs from the usual grid.
- One accent button per row ("Buy"), tinted danger when unaffordable with the missing amount shown.
- Square 4px corners, 1px line between rows, no gradient, no shadow.

```lua
--!strict
local Players = game:GetService("Players")
local Theme = require(script.Parent.Theme) -- tokens from section 2

type Item = { id: string, name: string, effect: string, price: number }

local function label(text: string, parent: Instance, size: number, color: Color3, bold: boolean?): TextLabel
	local l = Instance.new("TextLabel")
	l.BackgroundTransparency = 1
	l.Text = text
	l.TextSize = size
	l.TextColor3 = color
	l.FontFace = if bold then Theme.FontTitle else Theme.FontBody
	l.TextXAlignment = Enum.TextXAlignment.Left
	l.TextTruncate = Enum.TextTruncate.AtEnd
	l.Parent = parent
	return l
end

local function buildShop(items: { Item }, scrap: number): ScreenGui
	local gui = Instance.new("ScreenGui")
	gui.Name = "Workbench"
	gui.ResetOnSpawn = false
	gui.ScreenInsets = Enum.ScreenInsets.CoreUISafeInsets
	gui.DisplayOrder = 10

	local drawer = Instance.new("Frame")
	drawer.Name = "Drawer"
	drawer.AnchorPoint = Vector2.new(1, 0)
	drawer.Position = UDim2.fromScale(1, 0)
	drawer.Size = UDim2.fromScale(0.38, 1)
	drawer.BackgroundColor3 = Theme.Surface
	drawer.BorderSizePixel = 0
	drawer.Parent = gui

	local limits = Instance.new("UISizeConstraint")
	limits.MinSize = Vector2.new(300, 0)
	limits.MaxSize = Vector2.new(560, math.huge)
	limits.Parent = drawer

	local pad = Instance.new("UIPadding")
	for _, side in { "PaddingTop", "PaddingBottom", "PaddingLeft", "PaddingRight" } do
		(pad :: any)[side] = UDim.new(0, Theme.Pad)
	end
	pad.Parent = drawer

	local header = Instance.new("Frame")
	header.Size = UDim2.new(1, 0, 0, 44)
	header.BackgroundTransparency = 1
	header.Parent = drawer
	local title = label("WORKBENCH", header, 28, Theme.Text, true)
	title.Size = UDim2.fromScale(0.6, 1)
	local balance = label(`{scrap} Scrap`, header, 20, Theme.Accent, true)
	balance.AnchorPoint = Vector2.new(1, 0)
	balance.Position = UDim2.fromScale(1, 0)
	balance.Size = UDim2.fromScale(0.4, 1)
	balance.TextXAlignment = Enum.TextXAlignment.Right

	local list = Instance.new("ScrollingFrame")
	list.Position = UDim2.new(0, 0, 0, 56)
	list.Size = UDim2.new(1, 0, 1, -56)
	list.BackgroundTransparency = 1
	list.BorderSizePixel = 0
	list.ScrollBarThickness = 4
	list.AutomaticCanvasSize = Enum.AutomaticSize.Y
	list.CanvasSize = UDim2.new()
	list.Parent = drawer
	local layout = Instance.new("UIListLayout")
	layout.Padding = UDim.new(0, 1) -- the 1px gap shows the Line colour behind rows
	layout.SortOrder = Enum.SortOrder.LayoutOrder
	layout.Parent = list

	for i, item in items do
		local row = Instance.new("Frame")
		row.LayoutOrder = i
		row.Size = UDim2.new(1, 0, 0, 64)
		row.BackgroundColor3 = Theme.SurfaceRaised
		row.BorderSizePixel = 0
		row.Parent = list
		local name = label(item.name, row, 18, Theme.Text, true)
		name.Position = UDim2.fromOffset(12, 6)
		name.Size = UDim2.new(0.6, -12, 0, 24)
		local effect = label(item.effect, row, 14, Theme.TextMuted)
		effect.Position = UDim2.fromOffset(12, 32)
		effect.Size = UDim2.new(0.6, -12, 0, 22)

		local canAfford = scrap >= item.price
		local buy = Instance.new("TextButton")
		buy.Name = "Buy"
		buy.AnchorPoint = Vector2.new(1, 0.5)
		buy.Position = UDim2.new(1, -10, 0.5, 0)
		buy.Size = UDim2.new(0.3, 0, 0, 40)
		buy.AutoButtonColor = false -- states are handled in code, see ux-patterns
		buy.BorderSizePixel = 0
		buy.BackgroundColor3 = if canAfford then Theme.Accent else Theme.Line
		buy.FontFace = Theme.FontTitle
		buy.TextSize = 18
		buy.TextColor3 = if canAfford then Theme.Surface else Theme.Text
		buy.Text = if canAfford then `{item.price}` else `Need {item.price - scrap}`
		buy.Parent = row
	end

	gui.Parent = Players.LocalPlayer:WaitForChild("PlayerGui")
	return gui
end
```

Notes: no emoji, no gradient, one accent, text-only price instead of invented icons, panel sits on an edge. Connect `buy.Activated`, hover, press and purchase confirmation as described in `ux-patterns`. Text sizes are fixed, so check them against `PreferredTextSize` in `accessibility`.

## Sources used

- https://create.roblox.com/docs/production/game-design/ui-ux-design
- https://create.roblox.com/docs/ui/position-and-size
- https://create.roblox.com/docs/ui/size-modifiers
- https://create.roblox.com/docs/ui/on-screen-containers
- https://create.roblox.com/docs/ui/list-flex-layouts
- https://create.roblox.com/docs/ui/9-slice
- https://create.roblox.com/docs/ui/rich-text
- https://create.roblox.com/docs/production/publishing/accessibility
- https://create.roblox.com/docs/reference/engine/enums/Font
- https://create.roblox.com/docs/reference/engine/datatypes/Font
- https://create.roblox.com/docs/reference/engine/classes/GuiService
