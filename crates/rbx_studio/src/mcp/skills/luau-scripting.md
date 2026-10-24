---
name: luau-scripting
description: Rules for writing Luau in Roblox experiences: strict typing, module layout, server/client placement, deprecated APIs and their replacements, connection cleanup, yielding and error handling, and naming. Pull it before writing or reviewing any script.
---

# Luau scripting

## Start every script with `--!strict`

`--!strict` makes the type checker assert every inferred or annotated type. `--!nonstrict` only checks what you annotate, and `--!nocheck` turns checking off (source: https://create.roblox.com/docs/luau/type-checking). Use `--!strict` in all new files and annotate function parameters and return types.

```lua
--!strict
local Players = game:GetService("Players")

export type PlayerStats = {
	coins: number,
	level: number,
	nickname: string?,
}

local function addCoins(stats: PlayerStats, amount: number): PlayerStats
	stats.coins += amount
	return stats
end
```

- `export type` makes a type visible to scripts that `require` the module.
- `typeof(expr)` takes the type of an expression, useful for metatable classes: `type Car = typeof(setmetatable({} :: { speed: number }, {} :: CarMT))`.
- `::` casts a value to a type. Any value can be cast to `any`, but unrelated types are rejected (`myNumber :: boolean` is an error). Use casts sparingly, mostly for `FindFirstChild` results and `WaitForChild` returns: `local gui = player:WaitForChild("PlayerGui") :: PlayerGui`.
- Generics work on functions and type aliases: `local function first<T>(list: { T }): T? return list[1] end`.
- Prefer optional types (`string?`) over sentinel values, and handle `nil` explicitly.

(source: https://create.roblox.com/docs/luau/type-checking)

## Script types and where code lives

- `Script` runs on the server or client depending on its `RunContext` (Legacy, Server, Client). The default for a new Script is Legacy: server-side, and it only runs in server containers such as ServerScriptService or Workspace.
- `LocalScript` runs only on the client. `ModuleScript` has no run context and runs in whichever side requires it.
- Roblox recommends one entry Script with `RunContext = Client` in ReplicatedStorage and one with `RunContext = Server` in ServerScriptService, with most logic in ModuleScripts they require.
- Contents of ReplicatedStorage replicate to clients, so never put server-only code there, even with a Server run context. ServerScriptService is a server container; keep server-only code and assets there or in ServerStorage.

(source: https://create.roblox.com/docs/scripting/locations)

Layout that works:

| Location | Put here |
| --- | --- |
| `ReplicatedStorage/Shared` | modules both sides need (config, types, pure helpers) |
| `ServerScriptService` | server entry Script, server-only modules |
| `ServerStorage` | server-only assets (templates, models) |
| `StarterPlayer/StarterPlayerScripts` | client scripts and client modules |

The client can read anything that replicates to it. Treat everything on the client as untrusted input and everything in client code as readable by exploiters. Validate every RemoteEvent/RemoteFunction argument on the server, never ship secrets (API keys, admin lists with logic) to the client, and keep authoritative game logic on the server. See the `security` skill for the full rules.

## Modules

A ModuleScript returns one value, normally a table. Keep one responsibility per module and expose a small API. Require each module once at the top of the file.

```lua
--!strict
-- ReplicatedStorage/Shared/Inventory.lua
local Inventory = {}
Inventory.__index = Inventory

export type Inventory = typeof(setmetatable({} :: { _items: { [string]: number } }, Inventory))

function Inventory.new(): Inventory
	return setmetatable({ _items = {} }, Inventory)
end

function Inventory.add(self: Inventory, id: string, count: number)
	self._items[id] = (self._items[id] or 0) + count
end

return Inventory
```

```lua
--!strict
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Inventory = require(ReplicatedStorage.Shared.Inventory)
```

- Do not create require cycles (A requires B requires A). Move the shared piece into a third module.
- `require()` cannot be used inside a desynchronized parallel phase, so require first (source: https://create.roblox.com/docs/scripting/multithreading).
- ReplicatedStorage is the usual home for modules shared by client and server (source: https://create.roblox.com/docs/scripting/module).

## Services and globals

Call `game:GetService("Name")` once at the top of the file and name the variable after the service. Do not use `game.Players` style access (source: https://create.roblox.com/docs/scripting/services).

Avoid globals (`_G`, `shared`, or undeclared variables). Pass dependencies through `require` and function arguments. Declare everything `local`.

## Deprecated APIs and replacements

Use these replacements in all new code (sources: https://create.roblox.com/docs/scripting/scheduler and https://create.roblox.com/docs/reference/engine/deprecated).

| Do not use | Use instead |
| --- | --- |
| `wait()`, `wait(n)` | `task.wait(n)` |
| `spawn(f)` | `task.spawn(f)` (runs now) or `task.defer(f)` (runs at the end of the current resume point, same frame) |
| `delay(n, f)` | `task.delay(n, f)` |
| `Workspace:FindPartOnRay*` | `workspace:Raycast(origin, direction, RaycastParams)` |
| `BodyVelocity`, `BodyPosition`, `BodyGyro`, `BodyForce` | constraint movers such as `LinearVelocity` (see https://create.roblox.com/docs/physics/mover-constraints) |
| `Humanoid:LoadAnimation`, `AnimationController:LoadAnimation` | `Animator:LoadAnimation` |
| `Model:SetPrimaryPartCFrame` | `Model:PivotTo` |
| `GuiObject:TweenPosition/TweenSize/TweenSizeAndPosition`, `Camera:Interpolate` | `TweenService` |
| `Instance:Remove()` | `Instance:Destroy()` |
| `Humanoid:ApplyDescription`, `Humanoid:PlayEmote` | `ApplyDescriptionAsync`, `PlayEmoteAsync` |
| lowercase variants: `:connect`, `:destroy`, `:getChildren`, `:findFirstChild`, `Players.players` | `:Connect`, `:Destroy`, `:GetChildren`, `:FindFirstChild`, `Players:GetPlayers()` |
| `TeleportService:TeleportToSpawnByName` | `TeleportAsync` |

`PVInstance:PivotTo` is the documented way to move a Model (source: https://create.roblox.com/docs/reference/engine/classes/Model).

Instance creation: set `Parent` last, after all other properties. Do not pass the parent as the second argument of `Instance.new`, because each property change on a parented instance is replicated, rendered or simulated (source: https://create.roblox.com/docs/reference/engine/datatypes/Instance).

```lua
-- Do
local part = Instance.new("Part")
part.Anchored = true
part.Position = Vector3.new(0, 10, 0)
part.Parent = workspace

-- Don't
local part = Instance.new("Part", workspace)
part.Position = Vector3.new(0, 10, 0)
```

## Scheduling

- `task.spawn(f, ...)` resumes immediately. `task.defer(f, ...)` resumes later in the same frame cycle. `task.delay(n, f, ...)` runs after n seconds. `task.wait(n)` yields (source: https://create.roblox.com/docs/scripting/scheduler).
- Use `task.spawn` around code that might yield when you iterate a list and cannot let one item block the rest (for example `Players.PlayerAdded` handlers for players already in the game).
- Prefer events over `while true do task.wait() end` polling.

## Connection cleanup

Event connections and the values they capture are never garbage collected while connected. Clean up in one of three ways: call `connection:Disconnect()`, destroy the instance that owns the event, or destroy the script (source: https://create.roblox.com/docs/performance-optimization/improve).

```lua
--!strict
local Players = game:GetService("Players")

local cleanup: { RBXScriptConnection } = {}

local function trackCharacter(player: Player)
	local connections: { RBXScriptConnection } = {}
	local function onCharacter(character: Model)
		local humanoid = character:WaitForChild("Humanoid", 10) :: Humanoid?
		if not humanoid then return end
		table.insert(connections, humanoid.Died:Connect(function()
			print(player.Name, "died")
		end))
	end
	table.insert(connections, player.CharacterAdded:Connect(onCharacter))
	if player.Character then
		task.spawn(onCharacter, player.Character)
	end
	return connections
end

local perPlayer: { [Player]: { RBXScriptConnection } } = {}
table.insert(cleanup, Players.PlayerAdded:Connect(function(player)
	perPlayer[player] = trackCharacter(player)
end))
table.insert(cleanup, Players.PlayerRemoving:Connect(function(player)
	for _, c in perPlayer[player] or {} do c:Disconnect() end
	perPlayer[player] = nil -- also drop the table entry so it can be collected
end))
```

Rules:
- Store the `RBXScriptConnection` returned by `:Connect`, and disconnect it when the owner goes away. A tiny cleanup list (above) or a Maid/Trove library does the job; do not write a framework for two connections.
- `CharacterAdded` fires on every respawn. Re-fetch the character, humanoid and parts each time and handle `player.Character` already existing when the script starts.
- Remove per-player table entries in `PlayerRemoving`. Tables keyed by Player that never shrink are a common server leak (source: https://create.roblox.com/docs/performance-optimization/improve).
- `Instance:Destroy()` disconnects all connections on that instance and destroys children. Prefer it over setting `Parent = nil`.

## Yielding and errors

- `WaitForChild(name)` with no timeout can yield forever and prints an infinite-yield warning after a while. Pass a timeout on the client and handle the `nil` result: `WaitForChild(name, 5)` returns `nil` on timeout (source: https://create.roblox.com/docs/reference/engine/classes/Instance).
- On the server, avoid `WaitForChild` on instances you create yourself: reference them directly. Never block a server script forever waiting for something that might not exist.
- Wrap every call that can fail at runtime in `pcall`: DataStore, MessagingService, HttpService, MarketplaceService and other `...Async` methods. Keep the pcall tight and check both results.

```lua
--!strict
local DataStoreService = game:GetService("DataStoreService")
local store = DataStoreService:GetDataStore("PlayerData")

local ok, result = pcall(function()
	return store:GetAsync("user_123")
end)
if not ok then
	warn("GetAsync failed:", result)
	return
end
```

- Retry with a small delay and a cap on attempts; do not retry in a tight loop.
- Use `error("message", 2)` for programmer errors in modules, and `warn` for recoverable problems. Use `assert(cond, "message")` for invariants.

## Strings

- Use `string.format("%s has %d coins", name, coins)` or string interpolation with backticks: `` `{name} has {coins} coins` ``.
- Concatenate large strings with `table.concat(parts, ",")` rather than repeated `..` in a loop.

## Naming and style

From the Roblox Lua style guide (source: https://roblox.github.io/lua-style-guide/):
- PascalCase for classes, modules and enum-like objects, and for all Roblox APIs (camelCase API names are mostly deprecated).
- camelCase for local variables, members and functions.
- LOUD_SNAKE_CASE for local constants.
- Prefix private members with an underscore: `_items`.
- Use `table.freeze` on constant tables so accidental writes error at runtime:

```lua
local DEFAULTS = table.freeze({
	MAX_HEALTH = 100,
	RESPAWN_TIME = 5,
})
```

`table.freeze` is shallow; freeze nested tables too if they must be immutable.

## Checklist before you finish a script

1. `--!strict` on top, no type errors in the Script Analysis panel.
2. Services fetched once at the top with `GetService`.
3. No `wait`, `spawn`, `delay`, `FindPartOnRay`, `BodyVelocity`, `Humanoid:LoadAnimation`.
4. Every connection has an owner that cleans it up; per-player state removed on `PlayerRemoving`.
5. Every Async call is in a `pcall`; every client `WaitForChild` has a timeout.
6. Nothing trusted or secret lives on the client; server validates remote arguments.
