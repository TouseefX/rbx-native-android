---
name: input-and-devices
description: Cross-platform player input on Roblox: the Input Action System (InputContext, InputAction, InputBinding), ContextActionService and UserInputService, input-type detection, glyphs, touch buttons, gamepad UI navigation, VR, and testing with Studio's Device Simulator. Pull it before writing controls or UI that must work on keyboard, gamepad and touch.
---

# Input and devices

Roblox runs on phones, tablets, PC, console and VR, and one player can switch devices mid-session. Define what the player wants to do (actions), then bind several hardware inputs to each action.

## Choose an API

| API | Use when |
| --- | --- |
| Input Action System (`InputContext`, `InputAction`, `InputBinding`) | Default choice for new gameplay input. Edit-time bindings for keyboard, gamepad and touch, rebinding, swappable contexts. Roblox presents it as the cross-platform way (source: https://create.roblox.com/docs/en-us/input/input-action-system , https://create.roblox.com/docs/en-us/projects/cross-platform) |
| `ContextActionService:BindAction` | Code-only, runtime-scoped actions (bind while in a vehicle, unbind on exit), priority/sink control, quick auto-made touch button. Still valid. |
| `UserInputService` events | Raw input: custom camera, gesture handling (`TouchPinch`, `TouchSwipe`), mouse position, reading device state. Check `gameProcessedEvent` and ignore processed input. |

Do not mix several systems for the same key without thinking about who consumes it (sink/priority below).

## Input Action System

Classes (all documented under https://create.roblox.com/docs/reference/engine/classes/):

- `InputContext`: a group of related actions, such as `PlayContext` or `NavContext`. Properties `Enabled`, `Priority` (higher runs first) and `Sink` (consume the bound inputs at that priority so lower-priority listeners do not get them).
- `InputAction`: one mechanic. `Type` is an `Enum.InputActionType`: `Bool`, `Direction1D`, `Direction2D`, `Direction3D`, `ViewportPosition`. Events `Pressed`/`Released` (Bool only) and `StateChanged` (all types). Methods `GetState()` and `Fire()`. Read-only `PreferredBinding` returns the child binding that best matches the current device.
- `InputBinding`: child of an action. `KeyCode` for a key or button, composite `Up/Down/Left/Right/Forward/Backward` for keys-as-direction, `UIButton` to link an on-screen button (this is how touch is bound), `Scale`, `ResponseCurve`, `PressedThreshold`/`ReleasedThreshold` for analog inputs.
- `InputActionLabel`: a GuiObject that shows the right key text or icon for an action and updates on device change or rebind.

Recommended layout: a folder `Inputs` in `ReplicatedStorage` holding contexts; each action has one binding per device family (keyboard, gamepad, touch). Roblox's docs recommend each action have gamepad, keyboard/mouse and touch bindings for cross-platform support (source: input-action-system page).

Sprint example from the docs: action `CharacterSprint` (Bool) with bindings `LeftShift`, `ButtonY`, and a touch binding whose `UIButton` points at an on-screen `SprintButton`. A client Script then connects `Pressed`/`Released`.

```lua
--!strict
-- LocalScript (or Script with RunContext = Client)
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")

local play = ReplicatedStorage:WaitForChild("Inputs"):WaitForChild("PlayContext")
local sprint = play:WaitForChild("CharacterSprint") :: InputAction
local look = play:WaitForChild("CameraRotate") :: InputAction -- Type = Direction2D

sprint.Pressed:Connect(function() print("sprint on") end)
sprint.Released:Connect(function() print("sprint off") end)

-- continuous analog input: StateChanged fires once per change, so poll each frame
RunService.RenderStepped:Connect(function(dt)
	local v = look:GetState() :: Vector2
	if v.Magnitude > 0 then
		-- rotate camera by v * sensitivity * dt
	end
end)
```

Notes:
- For `MouseDelta` and `TouchDelta` bindings the values are in pixels; use `Scale` (the docs use 0.01) to get a sane magnitude.
- Switching modes (battle vs spectator, menu vs play): toggle `InputContext.Enabled` rather than rebinding.
- Setting `Workspace.PlayerScriptsUseInputActionSystem` to Enabled makes the default player scripts use the system, so defaults can be rebound (the docs mark it recommended; read the property page for rollout details).
- Reserved inputs cannot be overridden: Esc/Start (Roblox menu), F9 (developer console), F11, F12, PrintScreen. Default character, camera, chat (`/`), player list (`Tab`), backpack and tool keys exist unless the feature is disabled. Do not bind over them without a reason (source: input-action-system page).

## ContextActionService

```lua
--!strict
local CAS = game:GetService("ContextActionService")

local function onHonk(name: string, state: Enum.UserInputState, input: InputObject): Enum.ContextActionResult?
	if state == Enum.UserInputState.Begin then
		-- start horn
	elseif state == Enum.UserInputState.End then
		-- stop horn
	end
	return Enum.ContextActionResult.Sink -- consume so default handlers do not also fire
end

-- keyboard + gamepad + auto touch button
CAS:BindAction("Honk", onHonk, true, Enum.KeyCode.H, Enum.KeyCode.ButtonX)
CAS:SetTitle("Honk", "Horn")
CAS:SetPosition("Honk", UDim2.fromScale(0.75, 0.55))
-- later, when leaving the vehicle:
CAS:UnbindAction("Honk")
```

(source: https://create.roblox.com/docs/reference/engine/classes/ContextActionService)

- Third argument `createTouchButton` makes an ImageButton in `PlayerGui.ContextActionGui.ContextButtonFrame` on touch devices. At most 7 touch buttons can be created through `BindAction`. Customise with `SetTitle`, `SetImage`, `SetPosition`, or `GetButton`.
- Bind when the player enters the context, unbind when they leave. Use `BindActionAtPriority` to control which binding wins.
- Handlers return `Sink` to consume the input or `Pass` to let lower-priority handlers also see it.

## UserInputService essentials

Detect the primary device with `UserInputService.PreferredInput` (an `Enum.PreferredInput`: `KeyboardAndMouse`, `Gamepad`, `Touch`, `MicroGamepad`). It follows the player's most recent interaction with a connected gamepad or keyboard/mouse. Roblox's guidance is to prefer it over the older checks (source: https://create.roblox.com/docs/en-us/input):

- `TouchEnabled` is true on touch laptops, so it is not an "is mobile" check.
- `GetLastInputType()` returns several values for one device and thrashes between MouseMovement and Keyboard.
- A phone with a Bluetooth gamepad should show gamepad UI, even though touch is available.

```lua
--!strict
local UIS = game:GetService("UserInputService")
local function refresh()
	local p = UIS.PreferredInput
	-- show/hide touch buttons, swap glyph sets, change layout
end
UIS:GetPropertyChangedSignal("PreferredInput"):Connect(refresh)
refresh()
```

Other flags that still exist: `TouchEnabled`, `KeyboardEnabled`, `MouseEnabled`, `GamepadEnabled`, `VREnabled`, `GetGamepadConnected`, `LastInputTypeChanged`, `GetNavigationGamepads`. `GamepadConnected`/`GamepadDisconnected` events cover gamepad hot-plug.

## Button glyphs

Show the right prompt for the device; never hardcode "Press E".

- Best: `InputActionLabel` (auto-updates with device and rebinds, no scripting).
- Custom UI: `UserInputService:GetImageForKeyCode(keyCode)` returns a `ContentId` for the connected gamepad (Xbox, PlayStation, Windows families). `UserInputService:GetStringForKeyCode(keyCode, format?)` returns the key label for the player's keyboard layout (AZERTY aware). Pass the key the *action* is bound to (read it from `action.PreferredBinding.KeyCode`) (source: https://create.roblox.com/docs/reference/engine/classes/UserInputService).

## Mobile

- Default orientation is landscape (`StarterGui.ScreenOrientation = LandscapeSensor`). Other modes: `Sensor`, `LandscapeLeft`, `LandscapeRight`, `Portrait`. Change at runtime via `PlayerGui.ScreenOrientation`; read `PlayerGui.CurrentScreenOrientation` (source: https://create.roblox.com/docs/en-us/input/mobile).
- Use Scale for UI sizes and positions, and adapt per screen class with `GuiService.ViewportDisplaySize` (`Small` ~ phones/tablets, `Medium` ~ laptops, `Large` ~ TVs), because 75% width looks huge on a 4K TV (source: cross-platform page).
- Touch buttons: either a UI button linked via `InputBinding.UIButton`, or `BindAction(..., true, ...)`. Make hit areas large and put them where thumbs rest. Practical advice, not documented numbers: leave the bottom-left (movement thumbstick) and bottom-right (jump button) default zones free, and test on a small phone. Verify against the default controls in the Device Simulator.
- Haptics exist (`HapticEffect`, `HapticEffect:Play()`) on supported gamepads and Quest controllers (source: https://create.roblox.com/docs/en-us/input/gamepad).

## Gamepad and console UI

- Menus must be navigable without a pointer. `GuiService.GuiNavigationEnabled` (default on) lets the thumbstick/D-pad move between `GuiObject`s that have `Selectable = true`. `GuiService.SelectedObject` is the current selection; set it yourself when opening a menu so focus starts somewhere sensible. `GuiService.AutoSelectGuiEnabled` controls whether the Select button (or backslash) auto-picks a GUI. Turn it off if you manage selection yourself, and then you must set `SelectedObject` manually (source: https://create.roblox.com/docs/reference/engine/classes/GuiService).
- Control the focus order with `NextSelectionUp/Down/Left/Right` on GuiObjects; style the focus with `SelectionImageObject`.
- Reserved UI-selection keys in the default scheme: Select button / `\` toggles selection mode; ButtonR2 or Enter activates (see input-action-system page table).
- Use Roblox's common Xbox/PlayStation schemes for consistency (source: gamepad page). Gamepads without thumbsticks (TV remotes) are `PreferredInput.MicroGamepad`; see https://create.roblox.com/docs/en-us/input/micro-gamepad.
- Console screens are large: use `ViewportDisplaySize` to scale text and hit targets. Keep important UI inside the safe area; read `GuiService:GetGuiInset()` for the top bar inset.

## VR basics

Roblox VR uses OpenXR for all devices (Quest and PC VR). Check `UserInputService.VREnabled`. Standalone headsets need a comfortable frame rate (Auto Quality Mode exists), and Roblox provides comfort options (vignette, stepped rotation, fixed third-person camera). Prefer the default camera scripts unless you are building VR-first (source: https://create.roblox.com/docs/en-us/production/publishing/vr-guidelines).

## Testing across devices

Device Simulator (beta at the time of writing: enable "New Device Simulator" under File > Beta Features and restart Studio) (source: https://create.roblox.com/docs/en-us/studio/device-simulator):

- A toolbar above the 3D viewport, available in edit and play mode; you can change device mid-playtest.
- Categories: Phone, Desktop, Console, Headset. Phones are grouped Flagship / Mid-range / Entry level. Custom devices via Manage devices (Device Manager).
- Rotate button flips phone/tablet orientation. Mock Android back button. Display scaling: Physical size, Actual resolution, Fit to window (calibrate physical size against a card for real-world sizing).
- Touch emulation: mouse input stands in for touch on simulated phones; multi-touch gestures map onto Alt + drag (pinch + rotate) and Alt + Shift + drag (two-finger pan). The Touch controls button lists them.
- Virtual controller opens the Controller Emulator (gamepads, VR controllers, TV remotes) so you can test gamepad input without hardware (source: https://create.roblox.com/docs/en-us/studio/controller-emulator).
- Scriptable: `StudioDeviceSimulatorService` (plugins can switch devices, override resolution, orientation) and `VirtualInput` from `UserInputService:CreateVirtualInput()` for automated input (source: https://create.roblox.com/docs/en-us/studio/testing-modes).

Also test on real devices: a low-end phone for performance and thumb reach, a tablet, a gamepad on PC, and console if you ship there. Emulation does not reproduce performance, notches or OS gestures. Checklist: every action works on all three input families; UI reachable without a mouse; text readable on Small and Large displays; no UI under the top bar or thumb zones; switching input device mid-session updates glyphs.
