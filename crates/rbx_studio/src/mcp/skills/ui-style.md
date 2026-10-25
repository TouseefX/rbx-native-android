---
name: ui-style
description: Start here before building or restyling any in-game UI. Reads the feeling behind the prompt and the game's genre, picks a fitting visual style (trendy default, cartoony, sleek, striking, or a genre style), lists the generic "AI made" look to avoid, how to give each game its own identity instead of copying a reference, the shared layout rules every style uses, and the self-review checklist to run before showing UI.
---

# Picking a UI style

The UI you build must look like it belongs to this game and was made on purpose by a UI artist. The most common failure is a generic look that players instantly read as "vibe-coded". Pick a style first, then build. The style files hold the recipes:

| Skill | Use it for |
| --- | --- |
| `ui-style-trendy` | The default. What top simulator, tycoon, "Roll a X", obby and hangout games ship: chunky icon buttons, colour-coded nav, title-bar panels |
| `ui-style-cartoony` | Kid-friendly, playful, loud: thick outlines, bevelled buttons, stroked display type, patterned fills, ribbons |
| `ui-style-sleek` | Calm, premium, mature: translucent panels, thin type, line icons, lots of space |
| `ui-style-striking` | High energy shops, events, battle passes: slanted italic headers, sparkles, glows, fanned cards |
| `ui-style-genres` | Horror, military/realistic, anime fighting, RPG/fantasy, racing, roleplay/hangout pastel, pet simulator, tower defense, fashion |
| `ui-style-motion` | Tweens per style: open/close, hover, press, counters, and how much is too much |
| `ui-style-icons` | Drawing your own SVG icons that match the style, rasterizing and uploading them |

## 1. Read the intent before choosing

Answer these from the prompt and the place itself (look at the Workspace, existing UI, scripts, game name) before you write a single Frame:

1. **Genre.** What does the player do? Collect pets, fight, survive, race, hang out, build a base?
2. **Audience.** Young kids, teens, older players? Most Roblox players are young, but a war sim or horror game is aimed older.
3. **Feeling words in the prompt.** "cute", "fun", "bubbly" mean cartoony. "clean", "modern", "minimal", "premium", "chill" mean sleek. "hype", "epic", "insane", "event", "season" mean striking. "realistic", "tactical", "gritty" mean the military/realistic genre style. "scary", "creepy" mean horror.
4. **Existing art.** Low-poly bright world, realistic PBR world, anime characters, dark corridors? The UI must match the world's palette and line weight.
5. **Platform.** Assume mobile first (most players), then check desktop and console.

If the prompt names a vibe that conflicts with the genre ("make the shop of my horror game cute"), the prompt wins for colour and shape, but keep the genre's layout conventions (horror stays sparse).

If nothing points anywhere, use `ui-style-trendy`. It is the safe, expected look for most Roblox games.

## 2. Genre to style map

| Genre or vibe | Style | Never use |
| --- | --- | --- |
| Pet / clicker / "Roll a X" simulator | trendy or cartoony, rarity colours (`ui-style-genres`) | sleek, military |
| Tycoon, obby, tower of anything | trendy | military, horror |
| Hangout, roleplay, life sim | roleplay pastel (`ui-style-genres`) or sleek | military, striking everywhere |
| Fashion, dress-up | roleplay pastel or striking for events | military |
| Battle pass, event shop, limited offers in any kid game | striking for that one screen | striking for the whole HUD |
| Anime fighting, battlegrounds | anime (`ui-style-genres`) | pastel |
| Military sim, realistic shooter, tactical | military (`ui-style-genres`) | cartoony, bevels, bubbly fonts, rainbow |
| Horror | horror (`ui-style-genres`), mostly diegetic | cartoony, bright saturated buttons |
| RPG, fantasy, dungeon | RPG ornate (`ui-style-genres`) | sleek glass, neon |
| Racing, driving | racing (`ui-style-genres`) | pastel |
| Tower defense | trendy with a bottom unit bar (`ui-style-genres`) | sleek |
| Social deduction, party games | cartoony | military |
| Premium story or puzzle game for older players | sleek | cartoony |

A cartoony UI never goes on a war sim. A military HUD never goes on a pet game.

## 3. The anti-pattern: what "AI made" UI looks like

Do not produce this. Every item here is a tell:

- A flat dark navy or near-black card (`#11131C`-ish) floating in the middle of the screen.
- A thin 1 to 2 px purple or electric blue border on that card and on every button inside it.
- A faint top-to-bottom gradient on the card, same on every element.
- The default 8 to 12 px radius dropped on everything with no thought, or rounded and sharp boxes mixed by accident.
- Centred title in a default sans (Arial, Arimo, Gotham/Montserrat Bold, Builder Sans Bold) with no stroke, no icon.
- List rows as outlined rectangles with text like "Sword - 50 coins".
- A lone green pill "BUY" button.
- No icons, no item art, no currency symbol, no close button, or a text "X".
- Everything equal weight: no primary action, no hierarchy, no colour coding.
- Emoji used as icons.

Why it fails: it carries no genre, no identity, no depth, and no game feel. Real Roblox UI has outlines, depth (lips, shadows, bevels), icons and item art, colour-coded actions and personality in the type.

## 4. Give the game its own identity

The style files describe a family, not a template. Their palettes, button recipes and example layouts are references, the same way a UI artist keeps a mood board. A UI that copies them straight is a reskin, and players notice reskins as fast as they notice the "AI made" card. Every game gets its own look inside the chosen style:

1. **Its own palette.** Start from the style's palette, then pull the base and accent from the game itself: the world's sky and ground, the main character, the game's name. A lava obby leans ember orange and charcoal, an ocean pet game leans teal and coral. Keep the action colours readable (green buy, red close), but shift their exact hue so they sit with the base.
2. **Its own shapes.** Pick one shape idea and repeat it: clipped corners, a leaf-shaped tab, a ticket stub notch, a shield outline, a paw-print badge. Use it on panels, tabs, badges and icon frames so the set reads as one family.
3. **One signature detail.** Exactly one thing a player would remember: a pattern (stitching, bubbles, hex grid, wood grain, star field), an ornament (a ribbon, a rivet row, a tape strip, a mascot peeking over the title), or a motion quirk (buttons wobble like jelly, the coin counter flips like a scoreboard). One, done well, used in a few places. Three signature details is clutter.
4. **Its own icons.** Draw the icon set for this game (`ui-style-icons`): the coin carries the game's emblem, the shop icon fits the theme (a cauldron for a witch game, a crate for a military one).
5. **Beautiful but simple.** Fewer elements, each one finished. Prefer one strong panel with good spacing over many boxes. If a piece has no job, delete it.

Write the identity down before building, next to the palette: "base and accent from X, shape idea Y, signature detail Z". The test: could someone look at your UI and tell which reference screenshot or style example it was copied from? If yes, rework it until they could not.

## 5. Shared rules for every style

### ScreenGui setup

```lua
local gui = Instance.new("ScreenGui")
gui.Name = "ShopGui"
gui.ResetOnSpawn = false
gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
gui.ScreenInsets = Enum.ScreenInsets.CoreUISafeInsets -- default; keeps buttons clear of notches and the Roblox top bar
gui.Parent = game:GetService("StarterGui")
```

- `CoreUISafeInsets` is the default and the one to use for anything interactive. `DeviceSafeInsets` ignores the Roblox top bar, `TopbarSafeInsets` lays out inside the top bar strip, `None` is only for full-screen backgrounds (source: `reference/engine/enums/ScreenInsets.yaml`).
- Need the actual numbers? `GuiService:GetGuiInset()` returns two Vector2 (top-left, bottom-right inset). `GuiService.TopbarInset` is a Rect of the free space in the top bar, and it changes at runtime, so listen to its changed signal if you put things there.
- One ScreenGui per screen (HUD, Shop, Inventory), each with its own `DisplayOrder`. Modals get a higher `DisplayOrder` than the HUD.

### Sizing: author at 1280x720, scale with one UIScale

Production UI is laid out in pixels at a 1280x720 reference, so every padding and gap is an exact step of the spacing scale (section 6). Each ScreenGui holds one transparent `Root` frame with a `UIScale`. A LocalScript resizes `Root` so it always covers the screen in reference units, and everything inside is Offset sized:

```lua
local root = gui.Root -- Frame, BackgroundTransparency 1, AnchorPoint 0.5,0.5, Position 0.5,0.5 scale
local uiScale = root.UIScale
local function fit()
	local v = gui.AbsoluteSize -- the safe area, after ScreenInsets
	local s = math.clamp(math.min(v.X / 1280, v.Y / 720), 0.6, 1.5)
	uiScale.Scale = s
	root.Size = UDim2.fromOffset(v.X / s, v.Y / s)
end
gui:GetPropertyChangedSignal("AbsoluteSize"):Connect(fit)
fit()
```

- Inside `Root`, place HUD pieces with Scale positions and an AnchorPoint at the corner they hug (`Position = UDim2.new(0, 16, 1, -16)`, `AnchorPoint = Vector2.new(0, 1)`), and size them in Offset. Centre modals with `AnchorPoint = Vector2.new(0.5, 0.5)`, `Position = UDim2.fromScale(0.5, 0.5)`.
- At the 0.6 floor a phone in landscape gives `Root` about 650 reference units of height, so keep every modal at 600 or less tall. Tappable things are at least 64x64 reference (about 40 px on a phone).
- Nested `UIScale`s multiply, so the motion skill's pop `UIScale` on a panel works on top of this one.
- `UIAspectRatioConstraint` only shrinks a box to fit inside its own Size (default `AspectType = FitWithinMaxSize`). A box sized `(0, 0, 0, 24)` stays 0 wide and everything inside it vanishes. Give both axes a real size, or set `AspectType = ScaleWithParentSize`.
- Text: fixed `TextSize` from a small type scale (for example 14, 16, 20, 28, 40 at reference). `TextScaled` only for a big number or title that must fill its box, always with a `UITextSizeConstraint`.

### Touch targets and mobile layout

- Roblox's adaptive design guide (`production/publishing/adaptive-design.md`) asks for every action to work on touch, gamepad and keyboard, layouts that condense on small screens, and readable non-thin fonts on phones, but gives no minimum touch size. Google's Material guidance is 48dp and Apple's is 44pt; with the 0.6 scale floor, 64x64 at reference lands near 40 px on a phone, so treat 64 as the minimum, with 8 between targets.
- Mobile puts the thumbstick bottom-left and jump bottom-right. Keep the bottom 30 percent of both corners free of your buttons. Put the HUD button rail on the left edge, vertically centred, and currency bottom-left above the thumbstick zone or top-left.
- Test at phone landscape (about 844x390 viewport), tablet (1024x768), desktop (1920x1080). Nothing may overlap at the phone size.
- Support every input: connect `GuiButton.Activated` (fires for mouse, touch and gamepad), not `MouseButton1Click` (mouse only). Set `SelectionImageObject` or keep default selection so gamepad users can navigate.

### Layout instances (use them, do not hand-place rows)

- `UIListLayout`: `FillDirection`, `Padding` (UDim), `HorizontalAlignment`, `VerticalAlignment`, `SortOrder = LayoutOrder`, `Wraps`, `HorizontalFlex`/`VerticalFlex` (`Fill`, `SpaceBetween`, `SpaceEvenly`, `SpaceAround`).
- `UIGridLayout`: `CellSize`, `CellPadding`, `FillDirectionMaxCells`. Good for shop grids. Put it in a `ScrollingFrame` with `AutomaticCanvasSize = Y`.
- `UIPadding` on every container, from the spacing scale (section 6).
- `UIFlexItem` on a child to make it grow or shrink inside a flex list.

### Strokes, corners, gradients: the facts

- `UIStroke.ApplyStrokeMode = Contextual` on a text object outlines the glyphs. `Border` outlines the object's box. On a Frame it always outlines the box.
- `Thickness` is in pixels by default (`StrokeSizingMode = FixedSize`). `ScaledSize` makes it relative to the parent's smaller side, or to font size on text; this may need the "Improved UIStrokes" beta, so default to FixedSize and scale through `UIScale` if needed.
- `LineJoinMode`: `Round` (default), `Bevel`, `Miter`. Cartoony uses Round, military and striking use Miter.
- A Border stroke draws outside the box by default and gets clipped by a `ClipsDescendants` parent, so leave room.
- Several `UIStroke`s on one object draw in `ZIndex` order: use two for an outline plus glow.
- A `UIGradient` under a `UIStroke` makes a gradient stroke. A `UIGradient` under a GuiObject tints its fill (and text colour on text objects). `Rotation` 90 runs top to bottom. Max 20 keypoints.
- `UICorner.CornerRadius` is a UDim. `UDim.new(0.25, 0)` scales with size (good for square buttons), `UDim.new(0, 12)` is fixed. Per-corner radii exist (`TopLeftRadius`, etc.).
- Any label whose text has tags (`<font color>`, `<b>`, `<s>`, `<stroke>`) needs `RichText = true`, or players see the raw tags. Check every one.
- The legacy `TextStrokeTransparency` renders the outline four times and looks thin and blurry. Use a `UIStroke` child on text instead. Rich text also takes `<stroke color="#1A1A1A" thickness="3">`.
- There is no blur-behind-UI. `BlurEffect` in `Lighting` blurs the 3D world only; use it when a menu opens for the sleek "frosted" look.
- `CanvasGroup` flattens a subtree so `GroupTransparency` fades it as one, and it always clips. Good for fades and for clipping rotated shine strips. Costs texture memory, so do not wrap the whole HUD in one.

### Fonts that exist

Set `FontFace` with `Font.new(family, weight, style)` or `Font.fromName("FredokaOne")` (maps to `rbxasset://fonts/families/FredokaOne.json`). Families available (source: `reference/engine/datatypes/Font.yaml`):

- Chunky / cartoony display: **FredokaOne**, **LuckiestGuy**, **Bangers** (comic, slanted feel), DenkOne, ComicNeueAngular (Enum.Font.Cartoon), PatrickHand, PermanentMarker, IndieFlower, Kalam.
- Clean sans: **BuilderSans** (weights Regular to ExtraBold), Montserrat (what Gotham now maps to), Nunito (soft, rounded), Roboto, Ubuntu, TitilliumWeb, JosefinSans, Arimo.
- Condensed / tactical / tech: **RobotoCondensed**, **Oswald**, **RobotoMono**, BuilderMono, **Michroma** (wide tech), **Sarpanch** (squared sci-fi), Jura, Zekton, BuilderExtended, Inconsolata (Code).
- Serif / fantasy: Merriweather, **Fondamento**, GrenzeGotisch, Balthazar (Fantasy), RomanAntique (Antique), AccanthisADFStd, Guru.
- Themed: **SpecialElite** (typewriter, horror), **Creepster** (horror title only), PressStart2P (Arcade, retro), AmaticSC, HighwayGothic.

Use one display font for headers and buttons and one readable body font. Never Arial, Arimo, Legacy or SourceSans for a styled game.

### Colour

- Pick a palette before building: 1 base, 1 to 2 surface tones, 1 primary action colour, and the action colours (buy green, close red, info blue, premium gold).
- Colour-code by meaning, consistently: green buy/confirm, red close/cancel, blue navigation, gold/yellow premium or Robux, purple rare/event.
- Text on a fill needs contrast. White text on a light fill needs a dark `UIStroke`. Never mid-grey text on mid-grey.
- Derive strokes from the fill (a darker shade of the same hue) for cartoony and trendy; pure black strokes only on white or very bright fills.

### Icons and item art

Robux prices always carry the Robux symbol (draw your own hexagon-style mark in the UI's icon style), never a coin or gem icon; soft-currency prices carry that currency's icon. Mixing them up misleads players about what they pay with.

Every shop item, currency and HUD button needs an icon. Text-only buttons are a tell. If no matching asset exists, draw one with `ui-style-icons` and upload it with `store_image`. Use one icon style across the whole game.

## 6. Production bar: hard rules

The goal is UI a player would not guess was AI-made or unfinished, that the developer ships without polishing it. Each rule below is a pass/fail check, not advice.

### Corners: one decision for the whole UI

Decide rounded or sharp once, write it down with the radius values, and apply it to every box: panels, tiles, tabs, buttons, chips, badges, bars, stat segments, item backings, the close button, the scrollbar.

- **Rounded**: every visible box gets a `UICorner` from one small scale (for example 6, 12, 20 px at reference, or `UDim.new(1, 0)` for pills and round badges). A box nested inside another uses the outer radius minus the padding between them (never below 4), so the curves run parallel. A sharp rectangle anywhere in a rounded UI is a bug.
- **Sharp**: radius 0 everywhere, or the style's cut-corner shape everywhere. No pill, no rounded chip, no circle badge.
- The icons follow the same decision: round joins and soft shapes for rounded UI, miter joins and hard angles for sharp UI.
- Mixing is allowed only when the concept calls for it, as one rule you can say in a sentence and write next to the palette: "pet portraits are circles because pets are round, everything else is rounded-square", "the HUD is sharp but the radar is a circle because it is a radar". A mix with no written reason is an accident.

### Spacing: one scale, padding everywhere

Use one spacing scale at the 1280x720 reference: **4, 8, 12, 16, 24, 32**. Every padding, gap and inset is one of those numbers, and the root `UIScale` scales them all together.

| Where | Value |
| --- | --- |
| Panel inner padding | 24 (16 on a compact panel) |
| Between sections inside a panel (header to tabs, tabs to grid) | 16 |
| Grid cell padding, list padding | 12 (8 in dense lists) |
| Tile inner padding | 12 |
| Button side padding | 16, top/bottom 8 |
| Chip or badge padding | 4 vertical, 8 side |
| Icon to its label | 8 |
| Screen edge to HUD | 16 |

- Every container with children has a `UIPadding`. Every `UIListLayout` and `UIGridLayout` has a `Padding`/`CellPadding` from the scale. Sibling groups use the same gap everywhere they repeat.
- Text never touches an edge. Any text on a fill or in a button sits inside padding at least 8 side, and the text must fit: check `TextFits` is true at phone scale. A label that runs into its button's edge or gets clipped (a compass strip, a long price) is a fail; shorten it, widen the box, or lower the size from the type scale.
- A stroke takes room: a 3 px `Border` `UIStroke` draws outside the box, so keep at least that much gap to the next sibling and to a clipping parent.

### No empty space

- Every panel is either sized to its content (`AutomaticSize`, or sizes you compute from the item count) or filled with purposeful content. No region larger than about 15 percent of a panel is blank, except deliberate breathing room around hero art.
- A grid fills its rows. With 7 items in a 4-column grid, either add one, use 7 that fit another column count, or let it scroll. Never a big blank half-row in a fixed panel.
- A detail or preview pane is full top to bottom: big art, name and tag, a short description, stats, price and the action. A tall preview box with a small icon in the middle is empty space.
- Content starts right under the header, never centred in a tall void.
- Hero art fills its pane: in a preview or detail pane the illustration takes at least 60 percent of the pane's width (or height for a tall object). A small rifle floating in a big dark box is empty space.

### Icons and item art are images, never Frames

Never build an icon or item art out of Frames, UICorners and rotated bars. It always reads as stacked rectangles. Author every icon and item illustration as an SVG (`ui-style-icons`), rasterize it to PNG, show it in an `ImageLabel`/`ImageButton`. Item art in a shop tile or detail pane must be a real illustration with a proper silhouette, two-tone shading, a highlight and the style's outline, readable at 32 px.

### Motion ships with the UI

Every build ships a LocalScript that drives the signature motion from `ui-style-motion`: staggered entrance, a styled open and close, hover and press feedback on every button, count-up on currency, a shine on the featured offer and one idle pulse on the main CTA. Static UI is unfinished UI.

### Audit script

Run it as its own command after the build, with the menus visible (command bar or the MCP code tool). In the same chunk that created the UI nothing is laid out yet, so every label reports as not fitting. It flags missing padding, text that does not fit, children spilling out of their parent, and corner mistakes for the decision you wrote down:

```lua
local ROUNDED = true -- your corner decision
local root = game:GetService("StarterGui")
local function visible(o) return o.BackgroundTransparency < 1 end
for _, o in root:GetDescendants() do
	if o:IsA("GuiObject") then
		local kids = 0
		for _, c in o:GetChildren() do if c:IsA("GuiObject") then kids += 1 end end
		if kids > 0 and visible(o) and not o:FindFirstChildOfClass("UIPadding") then print("no padding:", o:GetFullName()) end
		if o:IsA("TextLabel") or o:IsA("TextButton") then
			if o.Text ~= "" and not o.TextFits then print("text does not fit:", o:GetFullName()) end
		end
		if visible(o) and (o:FindFirstChildOfClass("UICorner") ~= nil) ~= ROUNDED then print("corner breaks the rule:", o:GetFullName()) end
		local p = o.Parent
		if p:IsA("GuiObject") then
			local a, b = o.AbsolutePosition, o.AbsolutePosition + o.AbsoluteSize
			local pa, pb = p.AbsolutePosition, p.AbsolutePosition + p.AbsoluteSize
			if a.X < pa.X - 1 or a.Y < pa.Y - 1 or b.X > pb.X + 1 or b.Y > pb.Y + 1 then print("spills out of parent:", o:GetFullName()) end
		end
	end
end
```

Fix every line it prints that is not deliberate (a close button on a corner, a badge that overhangs on purpose, a reasoned corner exception).

### Details that give it away

- Tabs and chips are sized to their text (`AutomaticSize = X` plus side padding 16), with one gap between them. Fixed-width tabs leave uneven gaps around short and long labels.
- A selected-state marker (underline, dot, bar) sits inside the element's padding or flush on its edge, never half hanging off it.
- Use at most four button hues in one cluster (a rail, a tab row). A rainbow of fills reads as noise.
- Every number has its unit or icon, every price its currency mark, every count its label. No orphan numbers.

## 7. Composition: HUD and menus must not collide

Generated UI most often fails here: each piece looks fine, but at phone size the shop panel runs into the left rail, the currency pill sits under the close button, a sticker covers a title. Plan the screen as zones before building:

| Zone | Holds |
| --- | --- |
| Left edge strip (about 12 to 15 percent wide) | button rail |
| Top-left / bottom-left corner | currency |
| Top centre strip (about 12 percent tall) | nav, round info, compass |
| Right edge strip | boosts, daily reward, kill feed, secondary menu |
| Bottom corners (lower 30 percent) | kept free for thumbstick and jump on touch |
| Centre (what is left) | modals: shop, inventory, settings |

Rules:

- A modal is sized to the centre zone at the narrowest root, not to the screen. Check it with numbers: at 4:3 (1024x768) the root is 1280 reference units wide, so a centred modal of width `w` starts at `(1280 - w) / 2`. Every HUD piece that stays visible must end at least 16 left of that, counting anything that overhangs the modal (a title banner, a sticker, the close button). A 880 wide shop starts at 200, so a 260 wide wallet at 16 collides. Shrink the modal, move the HUD piece, or hide it while the modal is open.
- Or hide what it would cover: when a full menu opens, hide the rail and nav (`Visible = false`, or fade them) and keep only the currency it spends. Most top games do this. Decide per menu and write it down.
- Every element stays fully on screen. Nothing hangs off an edge unless it is a deliberate bleed decoration.
- Overlaps are only deliberate: a badge on a corner, a banner sticking out of a panel's top edge, a close button on a corner. Deliberate overlaps never cover text: give the title right padding equal to the sticker's width, keep badges in the corner outside the label.
- Inside grids, labels must fit the tile at phone size: shorten names, or `TextScaled` with a `UITextSizeConstraint` and a `MinTextSize`, never let a badge sit on the name.

Check it numerically, not by eye. Run this in Studio (command bar or the MCP's code tool) with the menus visible; it lists top-level pieces that intersect or leave the screen:

```lua
local gui = game:GetService("StarterGui")
local cam = workspace.CurrentCamera.ViewportSize
local items = {}
for _, sg in gui:GetChildren() do
	if sg:IsA("ScreenGui") and sg.Enabled then
		for _, c in sg:GetChildren() do
			if c:IsA("GuiObject") and c.Visible then table.insert(items, c) end
		end
	end
end
local function rect(o) return o.AbsolutePosition, o.AbsolutePosition + o.AbsoluteSize end
for i, a in items do
	local a0, a1 = rect(a)
	if a0.X < 0 or a0.Y < 0 or a1.X > cam.X or a1.Y > cam.Y then print("off screen:", a:GetFullName()) end
	for j = i + 1, #items do
		local b0, b1 = rect(items[j])
		if a0.X < b1.X and b0.X < a1.X and a0.Y < b1.Y and b0.Y < a1.Y then
			print("overlap:", a:GetFullName(), "<>", items[j]:GetFullName())
		end
	end
end
```

Repeat at a phone-sized viewport (Studio's device emulator) and fix every line it prints that you did not intend. A full-screen dim or backdrop frame overlaps everything by design; skip it.

## 8. Build order

1. Choose the style and write down: palette (hex), fonts, the corner decision and radius values, the spacing scale, stroke weight, light direction, the signature motion, and the identity from section 4 (palette source, shape idea, signature detail).
2. Build reusable pieces as functions: `makeButton(color, label, icon)`, `makePanel(title, color)`, `makeCurrency(icon, amount)`. Every button in the game comes from the same function so they match.
3. Draw the icon set and item art as SVG, rasterize, check each PNG at 32 px (`ui-style-icons`).
4. Build the HUD, then each menu. Hide menus by default (`Visible = false`) and open them from HUD buttons.
5. Write the motion LocalScript (`ui-style-motion`).
6. Run the audit script (section 6), the overlap check (section 7) and the checklist below, then look at it at phone size and play the entrance once.

## 9. Self-review checklist (run before showing UI)

Answer honestly. Any "yes" in the first group means rework.

**Is it the anti-pattern?**
- Is the main panel a flat dark card with a thin coloured border and nothing else?
- Is every element the same default radius and the same border colour, with no thought behind either?
- Is any font Arial, Arimo, Legacy, SourceSans or an unstroked default bold?
- Are there text-only buttons, emoji icons, or a text "X" close button?
- Would this UI look the same if the game were a different genre?
- Could someone tell which reference screenshot or style example this was copied from? Is it the style file's palette and layout unchanged?

**Does it fit?**
- Can you name the style and say why it fits this genre and this prompt in one sentence?
- Does the palette match the world (bright low-poly, realistic, dark, pastel)?
- Is there one clear primary action per screen, colour-coded?
- Do buttons have depth appropriate to the style (lip, bevel, shadow, or deliberately flat for sleek/military)?
- Does every item and currency have an icon in one consistent style?
- Can you name this game's palette source, shape idea and single signature detail? Would a player recognise this UI as this game's without the title?
- Is it simple: does every element have a job?

**Does it work?**
- When the shop is open, does anything collide with it, or did you decide which HUD pieces hide? Does any sticker or badge cover text? Does anything hang off the screen?
- Every tappable thing at least 64x64 at reference (about 40 px on a phone), 8 apart?
- Nothing under the top bar, the thumbstick or the jump button?
- Text readable at phone size (14 px minimum body)?
- Close button top-right of every modal, reachable with a thumb, plus Escape/B closes?
- Buttons react on hover and press?
- Fits at 844x390, 1024x768 and 1920x1080 without overlap?

**Is it production ready?** Any "no" means rework.
- Is the corner decision the same on every box, or is each exception written down with its reason?
- Does every container have a `UIPadding` from the scale, does every layout use a gap from the scale, and do repeated groups use the same gap?
- Does every text sit at least 8 px from its box edge, with `TextFits` true at phone scale? Nothing clipped?
- Is every panel filled or sized to its content, with no blank region over about 15 percent?
- Is every icon and item art an image drawn from SVG, never Frames? Does each item have a real silhouette (an egg is egg-shaped, a rifle has a stock, grip, magazine and barrel), two-tone shading, a highlight and the style's outline? Does it read at 32 px?
- Is there a LocalScript with a staggered entrance, a styled open, hover and press on every button, a currency count-up, a shine on the featured offer and one idle pulse?
- Does the entrance never show an empty panel, move each tile as one unit, and leave nothing decorative floating before its container?
- At 1280 reference width (4:3), does the open modal clear every visible HUD piece by 16, banners and stickers included? Does the hero art fill at least 60 percent of its pane?
- Would a player guess this was AI-made, or that it still needs a pass? If yes, find out why and fix it.

If you can, take a screenshot and compare it side by side with a top game in the same genre. If yours looks emptier, flatter or more uniform, it is not done.
