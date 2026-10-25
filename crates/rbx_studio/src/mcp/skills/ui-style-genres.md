---
name: ui-style-genres
description: Genre UI styles for Roblox games beyond the four base styles: military / realistic shooter (tactical, monospace, desaturated), horror (diegetic, sparse, grungy), anime fighting, RPG / fantasy (ornate frames), racing, roleplay / hangout (soft pastel, phone UIs), pet simulator (rarity tiers), tower defense, fashion. Layout, palette, fonts and Roblox recipes for each.
---

# Genre styles

Pull `ui-style` first for the genre map and shared rules. These are conventions seen across popular games in each genre, not hard rules: match the specific game's world first, and give it its own palette, shape idea and signature detail (`ui-style` section 4). Recipes reuse the `new(class, props, children)` helper from the other style files.

## Military / realistic shooter

Tactical, information-dense, quiet. The UI is an instrument, not a toy. Aimed at older players.

- **Layout**: health and armour bottom-left, ammo and weapon bottom-right, compass strip or minimap top-centre or top-left, kill feed top-right, objective/score top-centre, hit markers at the crosshair. Menus (loadout, shop) are full-screen with a left category list and a right detail pane.
- **Shape**: sharp everywhere: radius 0 on every panel, row, bar, chip, badge, button and the close button. No pills, no round avatars unless a written reason (a circular radar). Thin 1 px strokes in white at 0.7 to 0.85 transparency. Corner brackets instead of full borders (four small L shapes). Diagonal cut corners on headers via a rotated square at the corner.
- **Type**: RobotoCondensed or Oswald for labels, all caps; RobotoMono or BuilderMono for numbers (ammo, coordinates, timers, prices); Michroma or Sarpanch only for one big header. No stroke on text, or a 1 px black stroke at 0.5 transparency for readability over bright scenes.
- **Palette**: desaturated. Panels `#0B0E0C` at 0.35 to 0.5 transparency, text `#E7E9E2`, secondary `#8D948A`, accent amber `#F2B233` or olive `#9BA86A`, danger `#D9412B`, friendly `#5FA8D3`. No saturated green buy buttons: the purchase button is the accent fill with dark text.
- **Icons**: weapon side views with each weapon's own silhouette (stock, grip, magazine, rail, barrel), light metal in a hard three-step tone with darker polymer parts, a dark hairline, sharp joins (see the rifle in `ui-style-icons`). Small HUD glyphs (ammo, grenade) can be flat single-colour silhouettes.
- **Motion**: fast and linear-ish, 0.08 to 0.15 s, Quad Out. Number tickers for ammo, a short flash for hits, a small shake on damage. No bounce, no squash, no sparkles.
- **Shop**: called Armory or Loadout. Weapon list left (name, class, a stat bar), weapon art centre-right filling its pane (an SVG illustration or a ViewportFrame), stat bars (damage, range, fire rate) as thin segmented bars, price in mono with a small currency icon, "EQUIP" / "PURCHASE" in the accent.

```lua
local MONO = Font.new("rbxasset://fonts/families/RobotoMono.json", Enum.FontWeight.Medium)
local COND = Font.new("rbxasset://fonts/families/RobotoCondensed.json", Enum.FontWeight.Bold)
local ammo = new("TextLabel", { Text = "30 <font transparency=\"0.5\">/ 120</font>", RichText = true,
	FontFace = MONO, TextSize = 34, TextColor3 = Color3.fromHex("E7E9E2"), BackgroundTransparency = 1,
	AnchorPoint = Vector2.new(1, 1), Position = UDim2.new(1, -32, 1, -28), Size = UDim2.fromOffset(200, 40),
	TextXAlignment = Enum.TextXAlignment.Right })
local function bracket(parent, corner) -- corner = Vector2 (0 or 1, 0 or 1)
	for _, size in { UDim2.fromOffset(14, 2), UDim2.fromOffset(2, 14) } do
		new("Frame", { BorderSizePixel = 0, BackgroundColor3 = Color3.fromHex("E7E9E2"),
			AnchorPoint = corner, Position = UDim2.fromScale(corner.X, corner.Y), Size = size }).Parent = parent
	end
end
```

## Horror

Less UI is scarier. Every on-screen element breaks immersion.

- **Diegetic first**: health shown by vignette and breathing, items as a small inventory strip that fades out when unused, notes as `SurfaceGui` paper in the world, prompts through `ProximityPrompt` with a restyled look. Most of the time the screen is empty.
- **Type**: SpecialElite (typewriter) or Merriweather for notes and hints; Creepster only for a title card, never for UI text. Small sizes, wide letter spacing feel via caps.
- **Palette**: `#0B0A08` near-black, parchment `#D8C9A3`, dried red `#7A1E1E`, cold `#9FB4B8`. Low contrast for flavour text, but keep interactive text readable.
- **Texture**: grungy paper or scratched metal 9-slice panels (`ScaleType = Slice`, `SliceCenter` around the clean middle), soft dark vignette `ImageLabel` at the screen edges.
- **Motion**: slow fades 0.6 to 1.5 s, Sine InOut. Occasional flicker (transparency jitter) on danger. No bounce, no pops, no sparkles.
- **Shop** (pre-round, like a lobby store): a dim, cramped panel styled as a drawer, a ledger or a shop counter, items as pencil sketches or sepia photos, prices handwritten-style. Keep it outside the scary part of the game.

## Anime fighting / battlegrounds

- **Layout**: skill hotbar bottom-centre (4 to 6 slots with key letters in corner badges, cooldown wipe overlay), health and energy bars top-left or bottom-centre above the hotbar, combo counter right side, character select as tall slanted portrait cards.
- **Shape**: parallelograms (bars and buttons skewed by rotating an inner frame inside a clipping CanvasGroup, or by an authored slanted 9-slice image), sharp joins, thick 3 to 4 px black strokes.
- **Type**: Bangers or LuckiestGuy italic for headers and combo numbers, RobotoCondensed Bold for labels.
- **Palette**: black `#0D0D10`, white, one hot accent per game (red `#FF2A3D`, gold `#FFC02E`, cyan `#2EE6FF`), energy blue `#3D7BFF`.
- **Motion**: hard and fast: 0.06 to 0.12 s pops, screen-edge flashes on hits, Back Out for combo numbers, slight shake.

## RPG / fantasy

- **Panels**: ornate 9-slice frames: dark navy `#141B2E` or leather `#3B2A1E` centre, gold or bronze trim `#C9A24A` with corner flourishes. Author the frame as one image (`ui-style-icons`) and slice it so corners stay crisp:

```lua
local frame = new("ImageLabel", { Image = ORNATE_FRAME, BackgroundTransparency = 1,
	ScaleType = Enum.ScaleType.Slice, SliceCenter = Rect.new(48, 48, 208, 208), -- source pixels: left, top, right, bottom of the stretchy middle in a 256x256 image
	SliceScale = 0.5, Size = UDim2.fromScale(0.5, 0.6) })
```

- **Type**: Fondamento or GrenzeGotisch for headers, Merriweather for body, numbers in Merriweather Bold.
- **Inventory**: grid of square slots with rarity-coloured inner borders (grey, green, blue, purple, orange), item icons painted with soft shading, tooltips with stat lines in green/red.
- **Motion**: medium, 0.2 to 0.35 s, Quad or Back Out with small overshoot; parchment unroll for quests.

## Racing / driving

- **Layout**: speedometer bottom-right (big mono or Michroma number, arc gauge drawn as an image), position "2/8" and lap top-left, minimap bottom-left, countdown huge centre.
- **Type**: Michroma or Sarpanch italic for numbers, TitilliumWeb for labels.
- **Palette**: dark glass `#0A0F14` at 0.4 transparency, one neon accent `#00E5FF` or `#FF3D7F`, white text.
- **Shape**: slanted edges (parallelogram chips), thin neon strokes (1 to 2 px, accent, transparency 0.2) with a soft second stroke as glow.
- **Garage / shop**: car in a ViewportFrame on a turntable, stat bars (speed, acceleration, handling) as segmented neon bars, colour swatches as round chips.

## Roleplay / hangout (pastel, phone UIs)

- **Phone UI**: a phone frame bottom-right or centre (rounded rect, radius 28 to 36 px, dark bezel 6 px), an app grid inside (4 columns of rounded-square app icons with labels), each app opens a full-phone page. Players already know this metaphor.
- **Palette**: pastel. Pink `#FFC8DD`, lavender `#CDB4FF`, mint `#B8F2E6`, butter `#FFF1B8`, sky `#BDE0FE`, text `#3A3047`, white panels.
- **Shape**: rounded everywhere, big radius (16 to 24 px on panels, pills on chips and tabs, inner boxes radius minus padding), soft drop shadow (a black frame at 0.88 to 0.92 transparency offset 4 to 6 px down, same radius), thin 2 px stroke in a darker pastel or none.
- **Type**: Nunito (Bold for headers, Regular for body) or FredokaOne for a bubblier game. Text in dark plum, not black.
- **Icons**: soft two-tone, rounded, slight highlight, no thick black outline.
- **Motion**: gentle Back Out pops (0.2 s, small overshoot), slide-up sheets.

## Pet / clicker simulator

Base it on `ui-style-trendy` or `ui-style-cartoony`, plus:

- **Rarity tiers everywhere**: Common grey `#B9C2CC`, Uncommon green `#63E12E`, Rare blue `#2EB6FF`, Epic purple `#C23BFF`, Legendary gold `#FFC21A`, Mythic pink-red `#FF2E8A`, Secret/Exclusive rainbow (a `UIGradient` with 6 hue keypoints, slowly animated `Offset`). Used on tile fills, strokes, name text and egg-hatch reveals.
- **Inventory**: big grid of pet tiles with the pet render, rarity border, level or "x3" stack badge, equipped check badge, lock icon.
- **Egg hatch**: centre-screen egg shake, flash, pet reveal with rarity name in big stroked text and its odds shown (required for paid eggs, see `growth-monetization`).
- **Multipliers**: "x2" boost icons bottom-right with timers.

## Tower defense

- **Layout**: unit slot bar bottom-centre (5 to 6 square slots with unit portrait, cost tag under each, hotkey number), wave counter and base health top-centre, cash top-left or right of the slot bar, speed and skip-wave buttons top-right.
- **Upgrade panel**: docks beside the selected unit, shows current vs next stats with green deltas and an upgrade button with cost.
- **Style**: trendy or cartoony with strong outlines, rarity colours on unit slots.

## Fashion / dress-up

- **Palette**: pink `#FF8FC7`, violet `#B57CFF`, cream `#FFF5FA`, gold accent `#E8B04B`.
- **Layout**: category tabs as round icon chips along one side, big item grid with thumbnails, a mirror/preview area for the avatar, a timer and theme banner for rounds.
- **Type**: Nunito or FredokaOne, pills with large radius, light outlines (2 px darker pink), sparkles on rare items.
- **Events**: use `ui-style-striking` for the runway results or event shop.
