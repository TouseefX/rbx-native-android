---
name: animation
description: UI tweening with TweenService (durations, easing choices, press feedback, reduced motion) and character animation practice (Animator, AnimationTrack, priorities, markers, replication, ownership). Pull it before writing any tween, UI motion or animation-playback code.
---

# Animation

Part 1 covers UI and property tweens. Part 2 covers character and rig animation.

# Part 1: TweenService

## TweenInfo

`TweenInfo.new(time, easingStyle, easingDirection, repeatCount, reverses, delayTime)` with defaults `1`, `Quad`, `Out`, `0`, `false`, `0`. A `repeatCount` of `-1` repeats forever (source: https://create.roblox.com/docs/reference/engine/datatypes/TweenInfo).

```lua
--!strict
local TweenService = game:GetService("TweenService")

local info = TweenInfo.new(0.2, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local tween = TweenService:Create(frame, info, { Position = UDim2.fromScale(0.5, 0.5) })
tween:Play()
tween.Completed:Wait() -- or Connect, to chain
```

Chain tweens with `tween.Completed` (source: https://create.roblox.com/docs/ui/animation). Tween UI with `UDim2` scale values plus an `AnchorPoint`, and add a `UIAspectRatioConstraint` when tweening size so the shape holds (same source).

Stop using `GuiObject:TweenPosition/TweenSize/TweenSizeAndPosition` and `Camera:Interpolate`; they are deprecated in favor of TweenService (source: https://create.roblox.com/docs/reference/engine/deprecated).

## Durations

General motion research gives a usable range: most UI animations fit 100 to 500 ms, simple feedback such as a toggle is about 100 ms, a modal opening is 200 to 300 ms, appearing elements need a little longer than disappearing ones (about 300 ms in, 200 to 250 ms out), and 400 ms is already slow, reserved for big movement across large screens (source: https://www.nngroup.com/articles/animation-duration/).

| Use | Duration |
| --- | --- |
| Hover, press, toggle, colour change | 0.1 to 0.15 s |
| Panel or menu enter | 0.2 to 0.3 s |
| Panel or menu exit | 0.15 to 0.25 s (shorter than enter) |
| Large travel across the screen | up to about 0.5 s |
| Idle loops (bob, pulse) | 1 to 2 s per cycle, subtle amplitude |

If it feels slow, shorten it. Players repeat UI hundreds of times; long animations become friction.

## Easing choices

Roblox's EasingStyle values: Linear, Sine, Back, Quad, Quart, Quint, Bounce, Elastic, Exponential, Circular, Cubic (source: https://create.roblox.com/docs/reference/engine/enums/EasingStyle). Sharper curves (Quad, Cubic, Quart, Quint, Exponential) accelerate or decelerate harder. Back overshoots once; Bounce and Elastic overshoot or bounce several times.

| Situation | Style and direction |
| --- | --- |
| Element entering the screen | Quad, Cubic, Quart or Quint with `Out` (starts fast, settles gently; ease-out is the usual choice for entering elements per the NN/g source above) |
| Element leaving | same family with `In` |
| Idle loop, breathing, hover bob | `Sine` with `InOut` |
| Colour or transparency change | `Quad` `Out`, short |
| Pop-in for a reward or badge | `Back` `Out`, sparingly |
| `Elastic`, `Bounce` | rarely: playful rewards only, never on frequent UI |
| `Linear` | progress bars, rotating spinners, anything with constant speed |

Do not mix more than two styles on one screen; consistency reads as polish.

## Button press feedback

Use a `UIScale` child so the button's own layout and `Size` are untouched. Scale down on press, back on release, plus a small colour shift.

```lua
--!strict
local TweenService = game:GetService("TweenService")

local PRESS = TweenInfo.new(0.08, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local RELEASE = TweenInfo.new(0.15, Enum.EasingStyle.Back, Enum.EasingDirection.Out)

local function attachPressFeedback(button: GuiButton): { RBXScriptConnection }
	local scale = Instance.new("UIScale")
	scale.Parent = button
	local normal = button.BackgroundColor3
	local pressed = normal:Lerp(Color3.new(0, 0, 0), 0.15)
	local current: { Tween } = {}

	local function play(target: Instance, info: TweenInfo, goal: { [string]: any })
		local old = current[1]
		if old then old:Cancel() end
		local t = TweenService:Create(target, info, goal)
		current[1] = t
		t:Play()
	end

	return {
		button.MouseButton1Down:Connect(function()
			play(scale, PRESS, { Scale = 0.95 })
			button.BackgroundColor3 = pressed
		end),
		button.MouseButton1Up:Connect(function()
			play(scale, RELEASE, { Scale = 1 })
			button.BackgroundColor3 = normal
		end),
	}
end
```

Disconnect the returned connections (or destroy the button) when the UI is torn down.

## Avoid over-animating

- Animate the one thing the player should look at. A screen where everything moves has no focal point.
- Cancel a running tween (`tween:Cancel()`) before starting another on the same property, or you get fighting tweens and flicker. Keep a reference per target.
- Do not create tweens in a per-frame loop. If you need to follow a moving target every frame, set the property directly in the frame event or tween to a point with a short time, not a new tween per frame.
- Use TweenService for simple A-to-B motion rather than hand-written `RenderStepped` lerps; the engine handles stepping for you. Use frame events only for continuous, input-driven motion such as cameras (see the `performance` skill for choosing RunService events).
- Clean up: `Tween.Completed` connections and tweens on destroyed instances should not outlive the UI.

## Reduced motion

`GuiService.ReducedMotionEnabled` is `true` when the player turned on the Reduce Motion setting, and means motion effects should be reduced or removed. The simplest honoring: set tween time to `0` when it is on (source: https://create.roblox.com/docs/production/publishing/accessibility). The class page lists the property as hidden but documents the behavior (https://create.roblox.com/docs/reference/engine/classes/GuiService).

```lua
--!strict
local GuiService = game:GetService("GuiService")
local DURATION = if GuiService.ReducedMotionEnabled then 0 else 0.25
local info = TweenInfo.new(DURATION, Enum.EasingStyle.Cubic, Enum.EasingDirection.Out)
```

Apply it to slides, bounces, shakes and looping decoration. Fades and colour changes can stay.

# Part 2: Character animation

## Animator

Play animations through an `Animator`. A `Humanoid` already has one (`humanoid:WaitForChild("Animator")`). For rigs without a Humanoid, create an `AnimationController` with a child `Animator` (source: https://create.roblox.com/docs/animation/using).

`Humanoid:LoadAnimation` and `AnimationController:LoadAnimation` are deprecated. Call `Animator:LoadAnimation(animation)` (source: https://create.roblox.com/docs/reference/engine/deprecated). The Animator must be in the Workspace when you load.

```lua
--!strict
local Players = game:GetService("Players")

local ATTACK_ID = "rbxassetid://0000000" -- your own animation asset
local player = Players.LocalPlayer

local tracks: { [string]: AnimationTrack } = {}

local function setup(character: Model)
	table.clear(tracks)
	local humanoid = character:WaitForChild("Humanoid", 10) :: Humanoid?
	if not humanoid then return end
	local animator = humanoid:WaitForChild("Animator", 10) :: Animator?
	if not animator then return end

	local anim = Instance.new("Animation")
	anim.AnimationId = ATTACK_ID
	local track = animator:LoadAnimation(anim)
	track.Priority = Enum.AnimationPriority.Action
	tracks.Attack = track
end

player.CharacterAdded:Connect(setup)
if player.Character then task.spawn(setup, player.Character) end

-- later, on input:
-- tracks.Attack:Play(0.1)
```

## Load once, reuse tracks

- Load each animation once per character (on `CharacterAdded`), store the `AnimationTrack`, and call `:Play()` on it later. Loading on every input creates new tracks that you must stop and release.
- Do not preload dozens of tracks "just in case". Scene Analysis has an Animation memory view because animation assets are a common production leak: scripts keep them alive after the model is destroyed (source: https://create.roblox.com/docs/performance-optimization/scene-analysis). Drop references (`table.clear`) when the character is removed.
- Keep a short list per character (idle variants, a handful of actions). Load rarely used tracks lazily on first use.

## Playing and controlling a track

`AnimationTrack:Play(fadeTime, weight, speed)` defaults to `0.1`, `1`, `1` (source: https://create.roblox.com/docs/reference/engine/classes/AnimationTrack).

- `track:AdjustSpeed(n)` changes playback speed; `track:AdjustWeight(w, fadeTime)` changes blend weight.
- `track:Stop(fadeTime)` stops with a fade.
- Events on the track: `Stopped`, `Ended`, `DidLoop`, `KeyframeReached`. Check the class page for exact timing before relying on the order of `Stopped` and `Ended`.
- `track.Looped` controls looping on the track at runtime; the asset also stores a loop flag set in the Animation Editor (Looping button; a looping animation does not interpolate between last and first keyframes, so duplicate the first keyframes at the end) (source: https://create.roblox.com/docs/animation/editor). Set `Looped` in the asset for animations that always loop, and override on the track only when a use case differs.
- A track's `Priority` can be set in the editor or per track.

## AnimationPriority

Lower priorities are overridden by higher ones (source: https://create.roblox.com/docs/reference/engine/enums/AnimationPriority):

| Priority | Use |
| --- | --- |
| `Core` | lowest; Roblox default animations and catalog bundles |
| `Idle` | idle animations |
| `Movement` | walk, run, swim, climb |
| `Action` | actions that must override idle and movement |
| `Action2`, `Action3`, `Action4` | each overrides the one before; `Action4` is highest |

Use `Action` for most one-shots (attack, emote). Use `Action2` and up only to layer something over another action. Do not set everything to `Action4`; you lose the ability to override.

## Markers, not keyframe names

Define named event markers on the animation timeline in the Animation Editor and listen with `track:GetMarkerReachedSignal("Name")` (source: https://create.roblox.com/docs/animation/events). The signal passes the marker's parameter string.

```lua
track:GetMarkerReachedSignal("Hit"):Connect(function(param: string)
	-- apply damage / spawn effect at the exact frame
end)
```

`AnimationTrack.KeyframeReached` also exists and fires by keyframe name (names need not be unique), but markers are the documented way to drive gameplay events. Connect the marker signal after loading, before `Play`.

## Replication and who plays what

- The Animator handles playback and replication of tracks (source: https://create.roblox.com/docs/reference/engine/classes/Animator).
- Calling `LoadAnimation` on an Animator in a model the client has network ownership of (for example the local player's character) also loads the animation for the server, so animations a client plays on its own character replicate to others.
- For NPCs, running animations on the server means simulating on the server and replicating to clients. With many NPCs, play animations on the client instead: create the Animator on the client and run them locally, which reduces server load and replication (source: https://create.roblox.com/docs/performance-optimization/improve).
- Static NPCs that only need animation do not need a Humanoid; use an `AnimationController` with an `Animator`. For procedural animation of an avatar, update `Motor6D.Transform`, not `C0`/`C1` (same source).

## Ownership and permissions

Publish animations under the creator that owns the experience. If an animation will be used in a group-owned game, choose the group in the Creator field when publishing (source: https://create.roblox.com/docs/animation/editor). If an animation does not play, check the Output for asset-load errors and confirm the asset owner before debugging code.

## Customizing the default Animate script

Player characters carry a default `Animate` script with children per state (`run`, `walk`, `jump`, `idle`, `fall`, `swim`, `swimidle`, `climb`) each containing `Animation` instances. To replace defaults, edit their `AnimationId` on `CharacterAdded` (source: https://create.roblox.com/docs/animation/using):

```lua
local animate = character:WaitForChild("Animate")
animate.run.RunAnim.AnimationId = "rbxassetid://656118852" -- Ninja Run from the docs
```

Stop `animator:GetPlayingAnimationTracks()` first if the old default is already playing, as the docs sample does.

## AnimationClipProvider

`AnimationClipProvider` loads and previews `AnimationClip`s; use it when you need the clip data (keyframes or curves) rather than a playable track (source: https://create.roblox.com/docs/reference/engine/classes/AnimationClipProvider). For normal gameplay you do not need it.

## Checklist

1. Tweens: 0.1 to 0.3 s, Out for in, In for out, one running tween per property, reduced motion respected.
2. Animations: `Animator:LoadAnimation`, loaded once per character, tracks stored and reused.
3. Priorities set deliberately; events driven by markers.
4. Asset owned by the experience's creator or group.
5. Many NPCs: animate on the client.
