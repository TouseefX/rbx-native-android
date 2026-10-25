---
name: ui-style-motion
description: The signature motion every Roblox UI must ship, per style: a LocalScript with staggered panel and tile entrance, bouncy or snappy open and close, hover and press feedback, shine sweep on featured offers, currency count-up and one idle pulse on the CTA. TweenService recipes and timings for cartoony, trendy, striking, sleek, pastel, military and horror, layout gotchas, a performance guard, reduced motion and how much is too much.
---

# UI motion by style

Pull `ui-style` first. Motion is part of the style and part of the "wow": the first second after a menu opens is what makes a UI feel made by a studio. The same shop must move differently in a pet sim and a war sim. Static UI is unfinished UI.

## What every build ships (hard rule)

A `LocalScript` named `UIMotion` inside each ScreenGui (it runs from the player's PlayerGui copy, so it uses `script.Parent`). It must drive all of these:

1. **Entrance**: when the UI first appears and each time a panel opens, the panel comes in with the style's open motion, then its children arrive in a stagger: header, tabs, tiles one by one, the detail pane, the CTA last.
2. **Open and close** in the style's character, with a shared helper so every panel moves the same way.
3. **Hover and press** on every button: hover lift, press squash, a release that settles.
4. **Count-up** on every currency number when it changes, and on first show.
5. **Shine sweep** on the featured offer or premium button.
6. **One idle pulse** on the main CTA or the one claimable thing.

Write the signature motion down with the identity before building ("pops in with a jelly bounce, tiles drop in left to right, the coin counter rolls"). It can be the game's signature detail.

## Facts and gotchas

- `TweenInfo.new(time, easingStyle, easingDirection, repeatCount, reverses, delayTime)`; defaults 1 s, Quad, Out (source: `reference/engine/datatypes/TweenInfo.yaml`). Use `delayTime` for staggers instead of `task.wait` loops.
- A new tween on the same property cancels the old one; tweens on different properties run together. Keep the last tween per object and `:Cancel()` it before starting the next, so a quick reopen never fights a close.
- `GuiObject:TweenSize`/`TweenPosition` are deprecated. Use `TweenService:Create`.
- **Children of a `UIListLayout` or `UIGridLayout` ignore `Position`**: the layout places them. To move a tile, tween a `UIScale` inside it, or put the tile's content in an inner `Content` frame and tween that frame's `Position`. Tweening the tile itself does nothing.
- Tween `UIScale.Scale` for pops instead of `Size`: it scales children, strokes and text together and does not re-run layouts. Nested `UIScale`s multiply, so this works on top of the root fit scale.
- Do not tween `TextSize` or a text `UIStroke.Thickness`: text re-shapes every frame and flickers. Scale it with `UIScale`.
- `CanvasGroup.GroupTransparency` fades a whole subtree in one property, but every CanvasGroup renders to its own texture, renders blank if texture memory runs out (the budget follows the client's quality level), and resizing it re-allocates the texture. Use at most one per open panel, never one per tile, never around the whole HUD.
- Shine: tween a `UIGradient.Offset` on a white overlay, not a rotated frame in a CanvasGroup. No extra texture.
- `GuiService.ReducedMotionEnabled` (read-only boolean) is true when the player asked for reduced motion. Check it: drop loops and shines, turn pops and slides into short fades.
- `GuiObject.Interactable = false` stops a button reacting while its panel is closing.
- `Activated` fires for mouse, touch and gamepad. `MouseEnter`/`MouseLeave` are desktop only, so hover is never the only feedback. Use `InputBegan`/`InputEnded` for press-down state, and play the press from `Activated` too for gamepad.

## Signature motion per style

| Style | Panel open | Stagger (children) | Hover / press | Close | Signature extras |
| --- | --- | --- | --- | --- | --- |
| Cartoony | UIScale 0.5 to 1, 0.4 s Back Out, plus Rotation 4 to 0 | tiles UIScale 0 to 1, 0.3 s Back Out, 0.04 s apart | 1.06 in 0.1 s / 0.9 in 0.06 s, release Back Out 0.25 s | 0.15 s Back In to 0.7 | sticker wobble, coin "+N" float, Elastic only on rewards |
| Trendy | UIScale 0.8 to 1, 0.25 s Back Out | tiles UIScale 0.6 to 1, 0.2 s Back Out, 0.03 s apart | 1.05 / face sinks onto lip | 0.12 s Quad In | number punch 1.15 on change |
| Striking | slide from a diagonal plus Rotation 6 to 0, 0.4 s Exponential Out | cards fan in from the centre, 0.06 s apart | glow up / 0.92 | 0.15 s | sparkles twinkle, light rays rotate slowly |
| Sleek | fade (CanvasGroup) plus 16 px slide up, 0.25 s Quint Out | Content frames 12 px up plus fade, 0.2 s Quint Out, 0.025 s apart | 0.12 s transparency / 0.08 s darker | 0.15 s fade | soft count-up, underline glides between tabs |
| Pastel / hangout | 24 px slide up plus fade, 0.3 s Back Out (small overshoot) | tiles 8 px up plus UIScale 0.9 to 1, 0.03 s apart | 1.04 / 0.95 | 0.18 s | heart or sparkle pop on favourite |
| Military | clip wipe left to right, 0.15 s Linear | rows slide in from left 24 px, 0.1 s Quad Out, 0.02 s apart | instant accent bar / instant tint | 0.08 s | count-up ticks like a readout, brackets snap to the selection |
| Horror | 0.8 to 1.2 s Sine fade | none, or one slow flicker | none or slow glow | 0.6 s | rare flicker on the title |

Cap the whole entrance (open plus stagger) at about 0.6 s for military and sleek and 0.8 s for cartoony. With many tiles, shrink the gap so the last one starts by 0.35 s.

### Entrance rules that keep it from looking broken

- **Never show an empty container.** Children start while the panel is still opening (at 30 to 40 percent of the open time), so the panel is never on screen blank for more than about 0.1 s. A big empty panel waiting for its tiles looks like a loading bug.
- **A group enters as one unit.** A tile, its backing, its art, its decoration and its labels move and fade together. Tag the tile, not its parts. Nothing decorative (a disc, a glow, a pattern, a badge) may be visible before the container it belongs to.
- **Record fade targets once.** For fade-ins, store each object's resting transparency at startup (as an attribute) before anything animates, and fade back to that value. Reading the current transparency while a tween is running captures a half-faded value and leaves parts invisible.
- **Order follows reading order**: header, tabs, then tiles left to right and top to bottom, the detail pane, the CTA last.

## The motion script

One script per ScreenGui. Fill in the style values from the table, keep the helpers. The names (`Shop`, `Grid`, `Featured`, `BuyButton`) are whatever your build uses.

```lua
-- UIMotion (LocalScript inside the ScreenGui)
local TweenService = game:GetService("TweenService")
local GuiService = game:GetService("GuiService")
local gui = script.Parent
local REDUCED = GuiService.ReducedMotionEnabled

local active = {}
local function tween(obj, info, goal)
	if active[obj] then active[obj]:Cancel() end
	local t = TweenService:Create(obj, info, goal)
	active[obj] = t
	t:Play()
	return t
end
local function scaleOf(obj)
	local s = obj:FindFirstChild("MotionScale")
	if not s then
		s = Instance.new("UIScale")
		s.Name = "MotionScale"
		s.Parent = obj
	end
	return s
end

-- style values (cartoony shown)
local OPEN = TweenInfo.new(0.4, Enum.EasingStyle.Back, Enum.EasingDirection.Out)
local TILE_TIME, TILE_GAP = 0.3, 0.04

local function pop(obj, from, delay)
	local s = scaleOf(obj)
	s.Scale = REDUCED and 1 or from
	tween(s, TweenInfo.new(TILE_TIME, Enum.EasingStyle.Back, Enum.EasingDirection.Out, 0, false, delay), { Scale = 1 })
end

local function open(panel)
	panel.Visible = true
	local s = scaleOf(panel)
	s.Scale = REDUCED and 1 or 0.5
	tween(s, OPEN, { Scale = 1 })
	if not REDUCED then
		panel.Rotation = 4
		tween(panel, OPEN, { Rotation = 0 })
	end
	local i = 0
	for _, child in panel:GetDescendants() do
		if child:GetAttribute("Stagger") then -- tag tiles, tabs, CTA in the build
			i += 1
			pop(child, 0, 0.12 + math.min(i * TILE_GAP, 0.35))
		end
	end
end

local function close(panel)
	local t = tween(scaleOf(panel), TweenInfo.new(0.15, Enum.EasingStyle.Back, Enum.EasingDirection.In), { Scale = 0.7 })
	t.Completed:Once(function(state)
		if state == Enum.PlaybackState.Completed then panel.Visible = false end
	end)
end

local function juicy(button)
	local s = scaleOf(button)
	local function to(v, time, style)
		tween(s, TweenInfo.new(time, style or Enum.EasingStyle.Quad, Enum.EasingDirection.Out), { Scale = v })
	end
	local function down(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 or input.UserInputType == Enum.UserInputType.Touch then to(0.9, 0.06) end
	end
	button.MouseEnter:Connect(function() to(1.06, 0.1) end)
	button.MouseLeave:Connect(function() to(1, 0.1) end)
	button.InputBegan:Connect(down)
	button.InputEnded:Connect(function() to(1, 0.25, Enum.EasingStyle.Back) end)
	button.Activated:Connect(function()
		s.Scale = 0.9
		to(1, 0.25, Enum.EasingStyle.Back)
	end)
end

local function countUp(label, to, prefix)
	local v = Instance.new("NumberValue")
	local function show(n)
		label.Text = (prefix or "") .. string.format("%d", math.floor(n)):reverse():gsub("(%d%d%d)", "%1,"):reverse():gsub("^,", "")
	end
	v.Changed:Connect(show)
	show(0)
	local t = TweenService:Create(v, TweenInfo.new(REDUCED and 0 or 0.8, Enum.EasingStyle.Quart, Enum.EasingDirection.Out), { Value = to })
	t.Completed:Once(function() v:Destroy() end)
	t:Play()
end

local function shine(card) -- card holds a full-size white Frame "Shine" with a UIGradient
	local g = card.Shine.UIGradient
	g.Transparency = NumberSequence.new({
		NumberSequenceKeypoint.new(0, 1), NumberSequenceKeypoint.new(0.45, 1),
		NumberSequenceKeypoint.new(0.5, 0.55), NumberSequenceKeypoint.new(0.55, 1),
		NumberSequenceKeypoint.new(1, 1),
	})
	g.Rotation = 20
	task.spawn(function()
		while card.Parent and not REDUCED do
			g.Offset = Vector2.new(-1, 0)
			if card.Visible then
				TweenService:Create(g, TweenInfo.new(0.7, Enum.EasingStyle.Quad, Enum.EasingDirection.InOut), { Offset = Vector2.new(1, 0) }):Play()
			end
			task.wait(3.5)
		end
	end)
end

local function pulse(holder) -- a plain Frame wrapping the CTA button
	if REDUCED then return end
	local s = Instance.new("UIScale")
	s.Name = "PulseScale"
	s.Parent = holder
	TweenService:Create(s, TweenInfo.new(0.7, Enum.EasingStyle.Sine, Enum.EasingDirection.InOut, -1, true), { Scale = 1.05 }):Play()
end
```

Then wire it: `juicy` every `GuiButton` in the gui (`gui:GetDescendants()`), `countUp` each currency label with its value, `shine(Featured)`, `pulse(BuyHolder)`, run `open` on the HUD clusters and on any panel shown at join, and connect each nav button to `open`/`close` its panel. The pulse goes on a holder frame around the CTA, not the button: give each object one `UIScale`, and the holder's pulse then multiplies with the button's press scale instead of fighting it.

The shine overlay needs `ClipsDescendants` on the card (or the card's corner shape) so the white band stays inside, `BackgroundColor3` white, `BackgroundTransparency` 0, and a `UIGradient` whose transparency does the work. Give the overlay the card's `UICorner` too.

## Style notes

- **Sleek**: replace `pop` with a fade-and-rise. Wrap the panel in one `CanvasGroup`, tween `GroupTransparency` 1 to 0 and `Position` 16 px up. For staggered children, tween each child's inner `Content` frame from 12 px lower to rest, and fade its own transparencies from 1 (record each `BackgroundTransparency`, `ImageTransparency`, `TextTransparency` target first). No scale on panels, no Back easing.
- **Military**: the panel sits in a clipping frame whose width tweens from 0 to full (Linear 0.15 s), rows slide from 24 px left. Count-ups use Linear, short. No overshoot anywhere, no wobble, no pulse except a slow colour pulse on an objective.
- **Pastel**: like sleek but with a small Back overshoot and a scale from 0.9; a quick heart pop (UIScale 0 to 1.2 to 1) on favourite.
- **Striking**: the featured card sweeps in on a diagonal; scatter sparkles and twinkle two or three of them (scale 0.8 to 1.1, 1.2 s Sine loop). Still only one CTA pulse.

## Performance guard

- One `TweenService` tween per animated property; never drive UI from `RenderStepped` unless a curve truly needs it.
- An entrance plays at most about 40 tweens at once. More than that, animate rows or groups instead of every item.
- Loops (pulse, shine, twinkle) stop or skip while their panel is hidden. A `while` loop checks `Visible` and `Parent`.
- At most one `CanvasGroup` per open panel. Never tween a `CanvasGroup`'s `Size`.
- Cancel the previous tween on an object before the next. Never stack.
- `ImageLabel`s with the same image reuse one texture; keep art at 2x display size, not bigger.

## How much is too much

- One idle loop on screen at a time (the CTA pulse or the claimable reward wiggle), plus the shine on one featured card. A third moving thing is noise.
- Anything the player opens often (shop, inventory) finishes its entrance in 0.6 to 0.8 s. A slow entrance feels sluggish by the third visit.
- Elastic only for reward moments (level up, hatch, claim).
- Never block input during an animation: a second tap closes or skips.
- The overshoot stays small: Back Out on a scale from 0.5 to 1 is plenty. Never scale above 1.1 for hover.
- The anti-pattern is no motion at all, or the same fade on everything. Motion should carry the style.
