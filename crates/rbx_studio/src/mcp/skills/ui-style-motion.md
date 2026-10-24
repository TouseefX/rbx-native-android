---
name: ui-style-motion
description: UI motion per style for Roblox: TweenService timings and easing for open/close, hover, press squash, counters, notifications and idle loops; punchy Back/Elastic pops for cartoony and trendy, short Quad/Sine fades for sleek, fast linear for military, slow fades for horror; reduced-motion and how much is too much.
---

# UI motion by style

Pull `ui-style` first. Motion is part of the style: the same button must move differently in a pet sim and a war sim.

## Facts

- `TweenInfo.new(time, easingStyle, easingDirection, repeatCount, reverses, delayTime)`; defaults 1 s, Quad, Out, 0, false, 0 (source: `reference/engine/datatypes/TweenInfo.yaml`).
- `Enum.EasingStyle`: Linear, Sine, Quad, Cubic, Quart, Quint, Exponential, Circular, Back, Bounce, Elastic. `Enum.EasingDirection`: In, Out, InOut.
- `GuiObject:TweenSize` / `TweenPosition` are deprecated. Use `TweenService:Create`.
- Tween `UIScale.Scale` for pops instead of `Size`: it scales children, strokes and text together and does not fight layouts.
- Do not tween `UIStroke.Thickness` on text: glyphs re-render and flicker (UIStroke docs).
- `CanvasGroup.GroupTransparency` fades a whole panel in one tween.
- Connect `Activated` for clicks, `MouseEnter`/`MouseLeave` for hover (desktop only, so hover must never be the only feedback), `InputBegan`/`InputEnded` on the button for press-down state.

## Timing table

| Style | Open | Close | Hover | Press | Easing |
| --- | --- | --- | --- | --- | --- |
| Cartoony | 0.35 s scale 0.6 to 1 | 0.15 s to 0.8 and fade | 0.12 s scale 1.06 | 0.06 s scale 0.9, release Back Out 0.2 s | Back Out (overshoot), Elastic Out for rewards |
| Trendy | 0.25 s scale 0.85 to 1 | 0.12 s | 0.1 s scale 1.05 | face drops onto lip | Back Out, Quad Out |
| Striking | 0.4 s slide in from a diagonal plus rotate 6 to 0 | 0.15 s | glow transparency 0.6 to 0.3 | 0.06 s 0.92 | Back Out, Exponential Out |
| Sleek | 0.2 s fade plus 12 px slide up | 0.15 s fade | 0.12 s transparency only | 0.08 s transparency darker | Quad Out, Sine Out |
| Military | 0.12 s wipe or fade | 0.08 s | 0.06 s accent underline | instant tint | Quad Out, Linear |
| Horror | 0.8 to 1.5 s fade | 0.6 s | none or slow glow | none | Sine InOut |
| Pastel / hangout | 0.25 s slide up from bottom | 0.18 s | 0.12 s 1.04 | 0.06 s 0.95 | Back Out (small overshoot), Quad Out |

## Recipe: button feedback (cartoony / trendy)

```lua
local TweenService = game:GetService("TweenService")

local function juicy(button)
	local scale = button:FindFirstChildOfClass("UIScale") or Instance.new("UIScale")
	scale.Parent = button
	local face = button:FindFirstChild("Face") -- the raised layer over the lip
	local rest = face and face.Position
	local function to(s, t, style)
		TweenService:Create(scale, TweenInfo.new(t, style or Enum.EasingStyle.Quad, Enum.EasingDirection.Out), { Scale = s }):Play()
	end
	button.MouseEnter:Connect(function() to(1.06, 0.12) end)
	button.MouseLeave:Connect(function() to(1, 0.12) end)
	button.InputBegan:Connect(function(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 or input.UserInputType == Enum.UserInputType.Touch then
			to(0.92, 0.06)
			if face then face.Position = rest + UDim2.fromScale(0, 0.08) end -- sink onto the lip
		end
	end)
	button.InputEnded:Connect(function(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 or input.UserInputType == Enum.UserInputType.Touch then
			to(1, 0.25, Enum.EasingStyle.Back)
			if face then face.Position = rest end
		end
	end)
end
```

Pair the press with a short click sound (one per game, quiet). Gamepad users get `Activated` only, so also play the pop from `Activated`.

## Recipe: panel open / close

```lua
local function openPanel(panel)
	local scale = panel:FindFirstChildOfClass("UIScale") or Instance.new("UIScale")
	scale.Parent = panel
	scale.Scale = 0.6
	panel.Visible = true
	TweenService:Create(scale, TweenInfo.new(0.35, Enum.EasingStyle.Back, Enum.EasingDirection.Out), { Scale = 1 }):Play()
end

local function closePanel(panel)
	local scale = panel:FindFirstChildOfClass("UIScale")
	local t = TweenService:Create(scale, TweenInfo.new(0.15, Enum.EasingStyle.Quad, Enum.EasingDirection.In), { Scale = 0.8 })
	t.Completed:Once(function() panel.Visible = false end)
	t:Play()
end
```

Sleek version: wrap the panel in a `CanvasGroup`, tween `GroupTransparency` 1 to 0 and `Position` 12 px up, 0.2 s Quad Out. No scale.

## Recipe: counters

Never snap a currency number. Tween a NumberValue and format on change:

```lua
local function countTo(label, from, to)
	local v = Instance.new("NumberValue")
	v.Value = from
	v.Changed:Connect(function(n) label.Text = string.format("$%s", formatNumber(math.floor(n))) end)
	local t = TweenService:Create(v, TweenInfo.new(0.6, Enum.EasingStyle.Quart, Enum.EasingDirection.Out), { Value = to })
	t.Completed:Once(function() v:Destroy() end)
	t:Play()
end
```

Cartoony and trendy add a quick 1.15 pop on the currency icon and a floating "+250" label that rises 40 px and fades over 0.8 s. Sleek and military only tick the number.

## Recipe: idle attention loops

Only for the single thing that needs attention (claimable reward, new item):

```lua
local wiggle = TweenService:Create(button, TweenInfo.new(0.12, Enum.EasingStyle.Sine, Enum.EasingDirection.InOut, 3, true, 0), { Rotation = 6 })
-- run every 4 to 6 seconds while the reward is claimable, stop when claimed
```

Shine sweep on a premium button: a white rotated frame inside a `CanvasGroup`, tween its `Position.X.Scale` from -0.5 to 1.5 over 0.6 s, repeat every 3 s.

## How much is too much

- One idle loop on screen at a time. Two moving things compete; five feel cheap.
- No motion longer than 0.4 s on something the player opens often (shop, inventory). Long animations make the UI feel slow by the third visit.
- Elastic only for reward moments (level up, hatch, claim), never on a button the player presses all the time.
- Never block input during an animation: let a second tap close or skip.
- Respect reduced motion: if your game has a settings toggle, swap pops for fades and drop idle loops. Rotation and scale loops can be uncomfortable for some players.
- The anti-pattern has no motion at all, or the same fade on everything. Motion should carry the style.
