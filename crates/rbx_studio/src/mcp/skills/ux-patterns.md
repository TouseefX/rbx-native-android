---
name: ux-patterns
description: How in-game Roblox UI should behave: input on touch, gamepad and mouse, press feedback, purchase and confirm flows, notifications, onboarding, empty and error states, closing and back navigation, and short motion rules. Pull it when wiring interactions, not just drawing them.
---

# Interaction patterns for in-game UI

Companions: `ui-style` (how it looks), `accessibility` (contrast, text size, reduced motion), `animation` (tweens in depth). Roblox's own split is useful: UI is what the player sees, UX is how they act and move from one choice to the next (source: https://create.roblox.com/docs/production/game-design/ui-ux-design). Claims marked "observed" are common practice in popular games, not documented rules.

## 1. Know the device before placing anything

Roblox's adaptive design guidance lists four principles: input fluidity (every action reachable by keyboard/mouse, touch and gamepad), responsive layout, dynamic sizing, and legibility (source: https://create.roblox.com/docs/production/publishing/adaptive-design). Apply them like this:

- **Detect the input, don't guess the device.** `UserInputService.PreferredInput` and `LastInputTypeChanged` tell you what the player is using now. A tablet can have touch and a keyboard. Update prompts to match: "Press A" on gamepad, "Tap" on touch, "E" on keyboard.
- **Phones.** The default controls occupy parts of the bottom-left and bottom-right corners. Keep important info and virtual buttons out of those zones. Players use two thumbs, so frequently used buttons belong within thumb reach. A button 40 percent below the top edge is reachable on a phone and almost unreachable on a tablet, so scale placement with device size (source: https://create.roblox.com/docs/ui/position-and-size).
- **Extra action buttons** go next to the jump button, not in the thumbstick zone (devforum tutorial, source: https://devforum.roblox.com/t/the-correct-way-to-design-mobile-buttons/2494558). The jump button changes size and position between phones and tablets, so anchor relative to it, or disable the defaults with `GuiService.TouchControlsEnabled = false` and own the whole control scheme.
- **Touch target size.** Roblox's docs give no pixel minimum. Use the cross-industry bars: at least 44 by 44 px as a practical target (Apple and Google guidance) and never below 24 by 24 CSS px, the WCAG 2.2 AA floor (source: https://www.w3.org/TR/WCAG22/#target-size-minimum). Space neighbouring buttons at least 8px apart. A small icon can have a larger invisible hit area (an `ImageButton` with transparent padding).
- **Gamepad and console (TV at 10 feet).** `GuiService:IsTenFootInterface()` identifies console. Make text larger, margins bigger, and everything reachable by d-pad or stick (section 2).
- **Hover is not touch.** Never put required information or an action only behind hover. Use `MouseEnter`/`MouseLeave` as enhancement, `Activated` for the action (it fires for mouse, touch and gamepad).

## 2. Gamepad navigation

Roblox supports gamepad UI navigation: `GuiService.GuiNavigationEnabled`, `GuiService.AutoSelectGuiEnabled`, `GuiService.SelectedObject`, `GuiObject.Selectable`, `GuiObject.NextSelectionUp/Down/Left/Right`, `GuiObject.SelectionImageObject`, `GuiBase2d.SelectionGroup` and `SelectionBehaviorUp/Down/Left/Right` (source: https://create.roblox.com/docs/reference/engine/classes/GuiService and https://create.roblox.com/docs/input/gamepad). ButtonA accepts GUI selections and ButtonB cancels them (same gamepad page).

Rules:

- Every interactive element is `Selectable` (buttons already are; plain Frames are not).
- When a menu opens, set `GuiService.SelectedObject` to its first sensible control, and clear it (`nil`) when it closes. Do not leave focus on an invisible object.
- Provide a visible focus indicator that is not just colour. Either keep the default selection image or set `SelectionImageObject` to a frame with a clear outline, and make it at least 3:1 against both the element and the background (see `accessibility`).
- Fix the order. If the automatic neighbour is wrong, set `NextSelection*` explicitly. Use `SelectionGroup = true` on a modal so focus stays inside it.
- Bind B to close with `ContextActionService:BindAction` while a menu is open, and `UnbindAction` when it closes.
- Test with the Studio controller emulator (source: https://create.roblox.com/docs/input/gamepad).

```lua
--!strict
local ContextActionService = game:GetService("ContextActionService")
local GuiService = game:GetService("GuiService")

local ACTION = "CloseShop"

local function openMenu(menu: GuiObject, firstControl: GuiObject, close: () -> ())
	menu.Visible = true
	GuiService.SelectedObject = firstControl
	ContextActionService:BindAction(ACTION, function(_, state: Enum.UserInputState)
		if state == Enum.UserInputState.Begin then
			close()
		end
		return Enum.ContextActionResult.Sink
	end, false, Enum.KeyCode.ButtonB, Enum.KeyCode.Escape)
end

local function closeMenu(menu: GuiObject)
	menu.Visible = false
	GuiService.SelectedObject = nil
	ContextActionService:UnbindAction(ACTION)
end
```

Note: Escape normally opens the Roblox menu. Binding it in a game menu is common and fine while the menu is open, but unbind it on close so the player can always reach the Roblox menu.

## 3. Press feedback

Feedback is communication from the game to the player; a button that shows hover, pressed and released states is the documented example (source: https://create.roblox.com/docs/production/game-design/ui-ux-design). Missing feedback makes players wonder whether a feature works at all.

- Respond on the same frame as the input. Do the visual change first, the network call after.
- Every button has four states: normal, hover (mouse and gamepad focus), pressed, disabled. Disabled must look different by more than colour (dimmer plus a lock or a reason label).
- Pressed = darker or inset or 2 to 4px shift, not a long animation. A tiny scale (`UIScale` 0.96) is fine, short.
- After an action completes, confirm with something visible (number ticks, item appears, sound). Optionally add a sound, see `animation`.
- If the action is waiting on the server, show a busy state on that button (label "Buying..." and ignore further clicks) and handle the failure path (section 5).

```lua
--!strict
local TweenService = game:GetService("TweenService")
local GuiService = game:GetService("GuiService")

local function attachStates(button: TextButton, normal: Color3, hover: Color3, pressed: Color3)
	button.AutoButtonColor = false
	local function paint(c: Color3)
		local t = if GuiService.ReducedMotionEnabled then 0 else 0.1
		TweenService:Create(button, TweenInfo.new(t, Enum.EasingStyle.Quad, Enum.EasingDirection.Out), { BackgroundColor3 = c }):Play()
	end
	button.MouseEnter:Connect(function() paint(hover) end)
	button.MouseLeave:Connect(function() paint(normal) end)
	button.MouseButton1Down:Connect(function() paint(pressed) end)
	button.MouseButton1Up:Connect(function() paint(hover) end)
	button.SelectionGained:Connect(function() paint(hover) end)
	button.SelectionLost:Connect(function() paint(normal) end)
end
```

(`ReducedMotionEnabled` is documented in the accessibility guidelines; the class page lists it as hidden. Source: https://create.roblox.com/docs/production/publishing/accessibility.)

## 4. Motion, only as it affects UX

- UI motion is short: about 0.1 to 0.3 seconds. Use `Quad` or `Quint` with `Out` for things arriving or reacting (observed practice, not a documented number). Reserve `Back` and `Elastic` (which overshoot or spring) for rare rewards, not for every panel open.
- Animate to explain (a panel slides from the edge it lives on, a number ticks up), not to decorate.
- Motion draws attention; too much confuses (source: https://create.roblox.com/docs/production/game-design/ui-ux-design). One looping animation on screen at a time at most.
- Honour reduced motion: when `GuiService.ReducedMotionEnabled` is true, snap or cross-fade instead of moving (see `accessibility`).
- Never block input waiting for a tween to finish. Detail on easing, sequences and particles belongs to the `animation` skill.

## 5. Purchase and spending flows

Real-money flows use Roblox's own prompts, so the player sees the real price and confirms there (source: https://create.roblox.com/docs/reference/engine/classes/MarketplaceService).

- Use `MarketplaceService:PromptProductPurchase`, `PromptGamePassPurchase`, `PromptPurchase` etc. Your own button only opens the prompt. Never fake a Robux confirmation dialog.
- The grant happens on the server (`ProcessReceipt` for developer products), not in the `Prompt*Finished` events, which are for UI reaction. Update the UI when the server confirms (a RemoteEvent or a changed attribute/leaderstat), not when the prompt closes.
- **In-game currency:** if the price is low and reversible, buy immediately and show Undo or an obvious result. If it is high, irreversible, or a large share of the player's balance, confirm first with a small dialog that restates the item and the cost: "Buy Rust Pickaxe for 2,500 Scrap?" with the cancel as the easy default. Confirm only when it is expensive; confirming every 5-coin purchase is friction.
- Show price and balance before the click. Show the missing amount when unaffordable ("Need 340 more") rather than a silent disabled button. Paint unaffordable prices in a consistent colour (red in the documented examples).
- If the player can afford it, one tap = one purchase. Guard against double taps (disable the button until the server answers).
- Failure copy says what happened and what to do: "Purchase did not go through. You were not charged. Try again." Not "Error 500".
- Success: show what changed (item added to backpack, balance down) and a clear way to continue (Equip now / Keep shopping).
- Put premium offers where the player expects them, labelled as such. Rank offers by size and a colour stripe, as Dragon Adventures, Jailbreak and Tower Defense Simulator do (source: https://create.roblox.com/docs/production/game-design/ui-ux-design). Don't use fake countdowns or confusing currency stacks.

## 6. Notifications and toasts

- One queue, capped. Show at most 3 at a time; collapse repeats ("+5 Scrap" x4 becomes "+20 Scrap"); drop the oldest.
- Each lives 2 to 4 seconds, longer if it carries text the player must read (about 1 second per 3 words is a reasonable floor, observed practice). Pause the timer while hovered or focused.
- Place them where they don't cover play: top-centre or beside the HUD in the corner opposite the thumbs on phones, never over the crosshair or the controls.
- Critical, actionable messages (disconnect, out of stock for a quest item) use a persistent banner or dialog with a clear action, not a toast.
- Don't use a toast for something already visible (a number in the HUD going up is the feedback).
- Mirror important sound cues visually (see `accessibility`).
- Roblox's built-in notifications and prompts are reachable via `StarterGui:SetCore` for a few cases; prefer your own styled UI so it matches the game.

## 7. Onboarding

Roblox's onboarding docs exist (https://create.roblox.com/docs/production/game-design/onboarding and onboarding-techniques); the principles below are the UI side.

- Teach by doing: the first time a mechanic matters, show one short hint at that moment, anchored near what it refers to (a `BillboardGui` over the object, a `ProximityPrompt` for interaction), and dismiss it automatically when the player does the thing.
- No wall of text. One idea per hint, under about 12 words. No modal tutorial deck before the player can move.
- Show input-appropriate keys and icons (section 1).
- Highlight with restraint (a pulse on one element). A wiggling arrow is acceptable in a tutorial and annoying forever, so remove it after use (source: https://create.roblox.com/docs/production/game-design/ui-ux-design).
- Make tutorials skippable and remembered (DataStore), never repeat them.
- Players know conventions: E for prompts, X closes, gray means unusable, a lock means not yet unlocked. Use them and spend explanation on what's new.

## 8. Empty, loading and error states

- **Empty:** say what goes here and how to fill it, with a next step. "No pets yet. Hatch an egg at the shop." with a button that goes there, not a blank frame or "Nothing found".
- **Loading:** show structure (placeholder rows) or a small spinner after a delay of about 200 ms. Never freeze the screen. Set a timeout and move to an error state.
- **Error:** what failed, whether anything was lost, what to do next, with a Retry button. No stack traces, codes only in small text.
- **Locked:** show the unlock requirement ("Reach level 12") on the locked item instead of hiding it.
- **Long lists:** scrolling with a visible scrollbar (`ScrollBarThickness` about 4 to 8) and a search or sort when over about 30 items (observed practice in inventory and trade UIs).

## 9. Closing and navigation

- Close button: square, top right of the dialog, same colour and place everywhere (conventions per Roblox's guidance; source: https://create.roblox.com/docs/production/game-design/ui-ux-design). It must be inside the safe area and a comfortable touch size.
- Every full-screen or modal UI closes with: the X, Escape on keyboard, B on gamepad, and (for modals) a tap outside if that can't lose data.
- Back navigation is consistent: one level per press. Escape or B goes back one level, not to the root.
- Only one major panel open at a time. Opening a second closes or replaces the first (a shop and an inventory should not stack on each other).
- Don't block movement unless the UI demands full attention. A HUD, toasts and non-modal panels should let the player keep walking. If you must lock input (a dialogue), do it explicitly with `ContextActionService:BindAction` at high priority and release on close, and make sure it can never get stuck (always unbind on `Destroying` and on character removal).
- Don't put a full-screen overlay behind a panel that swallows clicks meant for the world unless it is intentionally modal. Set `Active = false` on decorative frames; `Active = true` on a frame sinks input to the world.
- Remember panel state per session (selected tab, scroll position), reset on purpose.
- Persistent HUD buttons that open panels should toggle: second press closes.

## 10. Quick pre-flight

- [ ] Works with mouse, touch and gamepad; focus lands in the right place on open, is cleared on close.
- [ ] Nothing important sits in the phone thumb zones; extra buttons relative to the jump button.
- [ ] Every button has normal, hover, pressed, disabled states and instant response.
- [ ] Purchases use Roblox prompts for Robux, confirmation only for costly in-game spends, server-confirmed result.
- [ ] Toasts capped, merged, timed.
- [ ] Empty, loading and error states written.
- [ ] Closes with X, Escape and B; one level back at a time; never traps the player.
- [ ] Motion short, optional (`ReducedMotionEnabled`), never blocking.

## Sources used

- https://create.roblox.com/docs/production/game-design/ui-ux-design
- https://create.roblox.com/docs/production/publishing/adaptive-design
- https://create.roblox.com/docs/production/publishing/accessibility
- https://create.roblox.com/docs/ui/position-and-size
- https://create.roblox.com/docs/input/gamepad
- https://create.roblox.com/docs/reference/engine/classes/GuiService
- https://create.roblox.com/docs/reference/engine/classes/MarketplaceService
- https://devforum.roblox.com/t/the-correct-way-to-design-mobile-buttons/2494558
- https://www.w3.org/TR/WCAG22/#target-size-minimum
