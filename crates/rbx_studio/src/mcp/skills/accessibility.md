---
name: accessibility
description: Accessibility rules for in-game Roblox UI: contrast ratios, readable text sizes, honouring the player's PreferredTextSize, PreferredTransparency and ReducedMotionEnabled settings, colour-independent signalling, localization room, captions for audio cues and input alternatives. Pull it before finishing any UI and whenever text, colour or motion is involved.
---

# Accessibility for in-game UI

These rules are for UI inside the Roblox experience you build. The editor's own UX guidelines use the same bars (WCAG AA 4.5:1 text contrast, 3:1 non-text, reduced motion honoured), but those are about the editor, not your game. Roblox's own checklist covers text size, contrast, colour non-reliance, sound non-reliance and player preferences (source: https://create.roblox.com/docs/production/publishing/accessibility). Its guidance also notes that over 26 percent of people have some disability, so this is a large audience, not an edge case.

## 1. Contrast

Roblox says to choose text and background colours with sufficient contrast but gives no numbers. Use WCAG 2.2 AA (source: https://www.w3.org/TR/WCAG22/#contrast-minimum and https://www.w3.org/TR/WCAG22/#non-text-contrast):

| What | Minimum ratio |
|---|---|
| Body text (below about 24px regular or 19px bold) | 4.5:1 |
| Large text (24px+ regular or 19px+ bold) | 3:1 |
| Non-text that carries meaning: icons, input borders, focus rings, bar fills against their track, selected-state outlines | 3:1 |
| Disabled controls | exempt, but keep them readable as "unavailable" |

Practical rules:

- **Check against the real background, not the panel colour.** Text drawn straight over the 3D world changes with lighting. Put it on a solid or mostly opaque plate (`BackgroundTransparency` 0 to 0.3), or use `UIStroke` or `TextStrokeTransparency` as an outline in a contrasting colour.
- **Semi-transparent backgrounds lower contrast** because the world shows through. Roblox's setting `GuiService.PreferredTransparency` exists for this (section 4).
- **Light on light and dark on dark** are the failure case Roblox calls out. White text on a yellow button fails; pick dark text on yellow.
- **Accent colours are the usual offenders.** Orange or lime text on a mid-grey panel often lands near 2:1. Test it.

Compute the ratio in code when building a theme, so a bad pair is caught early:

```lua
--!strict
local function linear(c: number): number
	return if c <= 0.03928 then c / 12.92 else ((c + 0.055) / 1.055) ^ 2.4
end

local function luminance(c: Color3): number
	return 0.2126 * linear(c.R) + 0.7152 * linear(c.G) + 0.0722 * linear(c.B)
end

-- Color3 channels are already 0..1 in sRGB, which is what WCAG expects.
local function contrast(a: Color3, b: Color3): number
	local la, lb = luminance(a), luminance(b)
	local hi, lo = math.max(la, lb), math.min(la, lb)
	return (hi + 0.05) / (lo + 0.05)
end

assert(contrast(Color3.fromHex("EFE6D6"), Color3.fromHex("1B1612")) >= 4.5)
```

Use the final composited colour if the background is translucent (blend it with what is behind it).

## 2. Text size

- Roblox's guidance: small text is hard to read, and enlarging smaller labels helps most players; choose sizes readable on mobile and test on real devices; avoid thin or overly decorative fonts (source: https://create.roblox.com/docs/production/publishing/accessibility and https://create.roblox.com/docs/production/publishing/adaptive-design).
- Roblox gives no pixel minimum. Use these working floors: **14px** for secondary captions at the absolute minimum, **16px or more** for anything the player must read to act (prices, names, objectives), larger on console where players sit far from a TV. Treat anything under 14 as a defect.
- Test at the smallest expected viewport (about 640x360) after scaling. If a `UIScale` shrinks the UI on small screens, check the resulting size, not the source size.
- Never rely on `TextScaled` for body text (section 3 explains why).
- Don't set text smaller than its neighbours to "fit"; give the container room (`AutomaticSize`, `TextWrapped`, a `ScrollingFrame`).

## 3. Respect the player's text size setting

`GuiService.PreferredTextSize` (an `Enum.PreferredTextSize`: `Medium` default, `Large`, `Larger`, `Largest`) reflects the player's Text Size setting and is applied through the engine's font rendering (source: https://create.roblox.com/docs/production/publishing/accessibility). Documented behaviour:

- Text constrained by a `UITextSizeConstraint` will not go below `MinTextSize` or above `MaxTextSize`, whatever the setting.
- `TextScaled = true` text is not scaled by the preference.
- `AutomaticSize` objects grow and shrink with the resized text.
- `TextWrapped = true` text wraps onto more lines as the size grows, within the object's absolute size.
- `TextService:GetTextSize()` and `GetTextBoundsAsync()` honour the setting.

So:

- Use fixed `TextSize`, `TextWrapped = true`, and `AutomaticSize` on labels and rows so they can grow. Give containers room, or a `ScrollingFrame`.
- Avoid `TextScaled` on anything but tiny fixed badges. If you use `UITextSizeConstraint`, set `MaxTextSize` generous enough (at least about 1.5x your base size) not to cancel the player's setting.
- Don't put text in fixed-height boxes. Rows should be `AutomaticSize.Y` with `UIPadding`.
- Test at `Largest`: nothing clips, nothing overlaps, buttons still fit their labels, no horizontal scroll.
- React to changes while playing:

```lua
--!strict
local GuiService = game:GetService("GuiService")

GuiService:GetPropertyChangedSignal("PreferredTextSize"):Connect(function()
	-- Re-run any manual layout that depends on text bounds
	-- (AutomaticSize and TextWrapped layouts update on their own).
end)
```

## 4. Preferred transparency

`GuiService.PreferredTransparency` (a number; `1` = the player is fine with default transparency, `0` = prefers fully opaque backgrounds) maps to the Background Transparency setting. Roblox's recommendation is to multiply an element's `BackgroundTransparency` by it, so backgrounds become opaque as it approaches 0 (source: https://create.roblox.com/docs/production/publishing/accessibility). The class page lists the property as hidden, but the guideline documents it.

```lua
--!strict
local GuiService = game:GetService("GuiService")
local CollectionService = game:GetService("CollectionService")

local TAG = "TransparentBack"
local base: { [GuiObject]: number } = {}

local function apply(obj: GuiObject)
	obj.BackgroundTransparency = (base[obj] :: number) * GuiService.PreferredTransparency
end
local function track(obj: Instance)
	if obj:IsA("GuiObject") then
		base[obj] = obj.BackgroundTransparency
		apply(obj)
	end
end

for _, obj in CollectionService:GetTagged(TAG) do track(obj) end
CollectionService:GetInstanceAddedSignal(TAG):Connect(track)
CollectionService:GetInstanceRemovedSignal(TAG):Connect(function(obj) base[obj :: GuiObject] = nil end)
GuiService:GetPropertyChangedSignal("PreferredTransparency"):Connect(function()
	for obj in base do apply(obj) end
end)
```

Tag every translucent panel and plate (the name is yours to choose) so one script handles them.

## 5. Reduced motion

`GuiService.ReducedMotionEnabled` is `true` when the player turned on Reduce Motion; it means UI animations and tweens should be reduced or removed (source: https://create.roblox.com/docs/production/publishing/accessibility). The class page lists it as hidden, but the guideline documents it and shows the usage. Do this:

- Read it before every tween and set the duration to 0, or switch to a fade (no positional movement).
- Subscribe to its change signal so a mid-game change applies.
- Reduce also: screen shake, parallax, constant bobbing, looping pulses, spinning loading icons, large scale pops. Keep state changes (a bar filling, a number changing) but make them instant or a short fade.
- Do not use motion as the only way to show something (see section 6).

```lua
--!strict
local GuiService = game:GetService("GuiService")
local TweenService = game:GetService("TweenService")

local function tweenUI(obj: Instance, seconds: number, goal: { [string]: any }, fadeOnly: { [string]: any }?)
	if GuiService.ReducedMotionEnabled then
		if fadeOnly then
			goal = fadeOnly -- e.g. { BackgroundTransparency = 0 } instead of a Position slide
		end
		seconds = 0
	end
	local tween = TweenService:Create(obj, TweenInfo.new(seconds, Enum.EasingStyle.Quad, Enum.EasingDirection.Out), goal)
	tween:Play()
	return tween
end
```

This is the player-facing setting; in-game it is your responsibility to apply it, the engine does not do it for your own tweens. See the `animation` skill for the rest of the motion rules.

## 6. Do not signal with colour alone

Over 5 percent of people have some colour blindness; Roblox recommends adding symbols or other differences alongside colour (source: https://create.roblox.com/docs/production/publishing/accessibility). WCAG 1.4.1 says the same (https://www.w3.org/TR/WCAG22/#use-of-color).

- Pair every colour state with a second cue: icon (check, cross, lock), text ("Equipped", "Need 340 more"), shape, position, pattern or outline.
- Rarity tiers: colour plus a label or a distinct frame shape or star count.
- Team colours: colour plus a name tag or icon.
- Health or cooldown: bar fill length plus a number.
- Red vs green pairs are the worst combination; vary lightness too.
- Selection or focus: an outline or marker as well as a tint.
- Check by viewing the UI in greyscale. If states are indistinguishable, add a cue.

## 7. Sound is never the only channel

Players who are deaf, hard of hearing or who muted the game miss audio-only events (source: https://create.roblox.com/docs/production/publishing/accessibility).

- Every gameplay-relevant sound has a visual twin: a directional damage indicator, a flash on the HUD for a hit, an on-screen prompt for a ringing phone or a boss roar.
- For spoken dialogue or narration, show subtitles. I found no documented captions feature for your own sounds, so build a caption `ScreenGui` yourself: speaker name, line, 3 to 6 seconds on screen (longer for more words), readable contrast on a plate (section 1), position clear of HUD and thumbs.
- Describe important non-speech sounds briefly in square brackets or with an icon ("[Footsteps behind you]"), only for sounds that matter to gameplay.
- Offer a toggle for captions and remember it.

## 8. Localization and text expansion

Roblox localizes UI automatically for players who turn on Automatic Translations, and manual translations always win (source: https://create.roblox.com/docs/production/localization). Set `AutoLocalize = false` (a `GuiBase2d` property) on text that must not be translated, such as NPC names or street signs. For strings built in script use the translator, for example `LocalizationService:GetTranslatorForPlayerAsync(player)` and `translator:FormatByKey(key, args)` (source: https://create.roblox.com/docs/production/localization/localize-with-scripting).

Design for it:

- Leave generous room in every text container; translated strings are often noticeably longer than English, and some languages need more line height. Use `AutomaticSize`, `TextWrapped`, `UIListLayout` and 9-slice frames that stretch (source: https://create.roblox.com/docs/ui/9-slice).
- Don't build sentences by concatenation ("You have " .. n .. " coins"). Use one template string per message with placeholders so word order can change.
- Don't bake text into images. If an image contains text, it cannot be translated or scaled; use a text label over an image.
- Avoid abbreviations only English speakers understand; avoid idioms in labels.
- Format numbers and dates with the locale in mind (separators vary).
- Test with a long locale string (German, Finnish) and a right-to-left language if you support it.
- Keep icons culture-neutral; avoid hand gestures and letters used as symbols.

## 9. Input alternatives

Roblox's adaptive-design guidance: every action should be reachable by gamepad (buttons and d-pad), keyboard shortcut and touch, with focusable controls and prompts that show the input in use (source: https://create.roblox.com/docs/production/publishing/adaptive-design).

- Don't require precise dragging, holding, rapid tapping or simultaneous keys as the only way. Give a tap or click alternative, and a hold-free option for hold-to-confirm interactions (a setting or a longer timeout).
- Keep controls reachable by directional navigation (see `ux-patterns`).
- Let players rebind keys where the game is control-heavy. The Input Action System lets you define actions with multiple bindings (source: https://create.roblox.com/docs/input/input-action-system); `ContextActionService:BindAction` accepts several inputs per action. Show the player's current binding in prompts, not a hard-coded key.
- Time limits on decisions (dialogue choices, quick-time events) should be generous, adjustable, or removable.
- Avoid flashing content: no more than three flashes per second (WCAG 2.3.1, https://www.w3.org/TR/WCAG22/#three-flashes-or-below-threshold). Applies to big full-screen flashes, strobing damage indicators and rapid colour cycling.
- Make hit areas big (see `ux-patterns`: 44px target, 24px absolute floor per WCAG 2.5.8, https://www.w3.org/TR/WCAG22/#target-size-minimum).

## 10. Settings screen

Offer a small in-game accessibility section only for things the game itself controls: caption toggle, hold vs tap, UI scale, screen-shake amount, high-contrast UI variant, key rebinding. The text size, transparency and reduced-motion toggles live in Roblox's own settings and arrive through the `GuiService` properties above, so read those instead of duplicating them (an override is fine if clearly labelled).

## 11. Checklist

- [ ] Text contrast 4.5:1 (3:1 large text), computed on the real, composited background.
- [ ] Icons, focus rings, bar fills and borders at least 3:1.
- [ ] No text under 14px; action-critical text 16px or more; checked at the smallest viewport.
- [ ] Labels use `TextWrapped` + `AutomaticSize`, not `TextScaled`, and survive `PreferredTextSize.Largest`.
- [ ] Translucent backgrounds multiply by `PreferredTransparency`.
- [ ] Every tween reads `ReducedMotionEnabled`; no looping or shaking motion in reduced mode.
- [ ] No state conveyed by colour alone; greyscale check done.
- [ ] Every important sound has a visual cue; dialogue has subtitles.
- [ ] Containers have room for longer translations; no concatenated sentences; `AutoLocalize` off only where intended.
- [ ] Gamepad, touch and keyboard all complete every task; bindings shown from live data.
- [ ] No full-screen flashing above 3 per second.

## Sources used

- https://create.roblox.com/docs/production/publishing/accessibility
- https://create.roblox.com/docs/production/publishing/adaptive-design
- https://create.roblox.com/docs/production/localization
- https://create.roblox.com/docs/production/localization/localize-with-scripting
- https://create.roblox.com/docs/ui/9-slice
- https://create.roblox.com/docs/input/input-action-system
- https://create.roblox.com/docs/reference/engine/enums/PreferredTextSize
- https://www.w3.org/TR/WCAG22/ (contrast-minimum, non-text-contrast, use-of-color, three-flashes-or-below-threshold, target-size-minimum)
