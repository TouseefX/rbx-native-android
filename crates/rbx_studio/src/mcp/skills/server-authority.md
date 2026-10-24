---
name: server-authority
description: Roblox Server Authority (client-side prediction, rollback, resimulation): setup properties, BindToSimulation, InputAction, attributes, PredictionMode, and a compliance checklist. Pull it before writing or reviewing movement, combat or physics scripts in a place that sets Workspace.AuthorityMode to Server.
---

# Server Authority: prediction-compatible scripts

Status: opt-in feature. The DevForum "Client Beta" announcement is dated 2026-04-30 and said players can join published Server Authority games, APIs were locked but could still change before general release (source: https://devforum.roblox.com/t/client-beta-publish-test-your-server-authoritative-experiences/4606949). A Roblox newsroom post dated 2026-07-09 introduces the feature (source: https://about.roblox.com/newsroom/2026/07/creating-responsive-cheat-resistant-games-roblox-server-authority). The current docs page carries no beta label (source: https://create.roblox.com/docs/en-us/projects/server-authority, last updated 2026-10-23). I could not confirm a general-availability date; treat names below as current docs but re-check before shipping. Expect more CPU use on clients and servers than the classic model (announcement above).

## What it is

The server is the single source of truth. Clients send only inputs. To hide latency the client predicts a few frames ahead of the last server state. When the server's state for a frame differs from what the client predicted (a misprediction), the client rolls back to the server state and resimulates forward, re-applying its local inputs. Small mispredictions are normal, since clients cannot predict other players' inputs (source: https://create.roblox.com/docs/en-us/projects/server-authority).

Why: it blocks whole cheat classes (fly, speed hacks) because the client never reports its own position, while keeping input responsive. See security.md for the general rule.

## Setup

On `Workspace` (source: server-authority page above):

- `AuthorityMode = Enum.AuthorityMode.Server` (the other value is `Automatic`, the classic network-ownership model). Setting it also sets the next five.
- `NextGenerationReplication` enabled.
- `PlayerScriptsUseInputActionSystem` enabled.
- `SignalBehavior = Enum.SignalBehavior.Deferred`.
- `UseFixedSimulation` enabled.
- `StreamingEnabled` enabled.

## What changes for scripts

1. Core gameplay logic lives in a ModuleScript in ReplicatedStorage, required by both a server Script and a LocalScript, and runs inside `RunService:BindToSimulation(function(deltaTime) ... end)`. The engine re-runs bound functions during resimulation.
2. Inside a bound function you may only read or write members marked "Simulation Access" in the API reference (for example `BasePart.CFrame`, `InputAction:GetState`). Anything else errors. This is deliberate, to force a pure simulation (source: https://create.roblox.com/docs/en-us/reference/engine/classes/RunService).
3. Inputs come from `InputAction` objects, not `UserInputService`. Their `InputContext` must be a descendant of the `Player`; clone a template folder under each player on the server.
4. Game state that affects the simulation (health, ammo, cooldowns, last-frame input) goes in attributes on predicted instances, written only inside bound functions. Plain Luau variables are not rolled back (announcement above).
5. Visuals and audio go in `RenderStepped` (or another non-simulation path) and read the simulated state.

```lua
--!strict
-- ReplicatedStorage/Simulation (ModuleScript), required by both a Script and a LocalScript
local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

local Simulation = {}

function Simulation.Initialize()
	RunService:BindToSimulation(function(dt: number)
		for _, player in Players:GetPlayers() do
			local actions = player:FindFirstChild("Inputs")
			local move = actions and actions:FindFirstChild("Move") :: InputAction?
			local character = player.Character
			local root = character and character:FindFirstChild("HumanoidRootPart") :: BasePart?
			if move and root then
				local dir = move:GetState() :: Vector2
				-- apply the same math on client and server, using dt only
				root.CFrame += Vector3.new(dir.X, 0, -dir.Y) * 16 * dt
			end
		end
	end)
end

return Simulation
```

This is a minimal sketch of the documented pattern; real movement should respect collisions, ground checks and speed caps. The player-clone pattern for `InputContext` is in the docs page above.

## Rules for prediction-safe code

Do:

- Keep the simulation deterministic: same inputs, same state, same result on client and server.
- Use `time()` for time-based logic in the simulation. With `AuthorityMode.Server` it is synchronized and rewinds on resimulation (source: server-authority page).
- Validate every `InputAction` value (type, range, rate) before using it. The server still must not trust client input.
- Store simulation state in attributes: first 64 per instance, name at most 50 chars, string values at most 50 chars, or they will not replicate.
- Use `task.defer`, not `task.spawn`, to schedule code outside simulation restrictions. `task.spawn` from a bound function still runs inside simulation (source: server-authority page).
- Create instances only inside bound callbacks for predictive stitching, and parent them into the DataModel in that same frame (source: https://create.roblox.com/docs/en-us/projects/server-authority/techniques).
- Use `RunService:SetPredictionMode(instance, Enum.PredictionMode.On | Off | Automatic)` (client only) to force prediction on or off for an instance. `Automatic` currently predicts only BaseParts near the local character's Humanoid. `RunService:GetPredictionStatus(instance)` returns `Authoritative`, `Predicted` or `None` (source: RunService class page and https://create.roblox.com/docs/en-us/reference/engine/enums/PredictionMode).

Don't:

- Read `UserInputService.InputBegan` or similar events in the simulation.
- Use `tick()`, `os.time()` or `os.clock()` for simulation logic: not synchronized and not rolled back.
- Use `math.random` or other unseeded randomness in the simulation. This follows from the determinism requirement; the docs do not give a specific replacement, so derive any randomness from synchronized state such as frame time and attributes.
- Play sounds, spawn effects, or call `Destroy` on things clients show from inside a bound function. Resimulation can replay it.
- Rely on RemoteEvents for core gameplay. They are not guaranteed to be ordered with property and attribute updates (source: server-authority page, replication order).
- Keep a reference to a predicted instance and parent it in a later frame.

## Reviewing an existing script

Check, in order:

1. Where does it run? Anything that moves characters, vehicles or projectiles must be in a bound simulation function, not `Heartbeat` or a `while true` loop.
2. Inputs: any `UserInputService`, `ContextActionService` or RemoteEvent that drives core movement or abilities should become an `InputAction`.
3. State: variables that change gameplay (`local health = 100`) should move to attributes on a predicted instance.
4. Time: replace `tick()`, `os.time()`, `os.clock()` and `task.wait`-based timing in simulation with `time()` and the `deltaTime` argument.
5. Side effects: sounds, particles, UI and `Destroy` calls move to a `RenderStepped` reader of the simulation state.
6. Instance access: any property or method used in the bound function must have the Simulation Access label in the API reference. Write non-simulation properties (colors, names) on instances before parenting, or outside simulation.
7. Test under real latency, with a private or Experience Beta publish (announcement above). Watch for rollback loops and visible snaps; smooth rendering with a separate renderer part if needed (techniques page above).

## Open questions

- The newsroom post does not state beta versus released; the devforum post that announced it called it Client Beta on 2026-04-30.
