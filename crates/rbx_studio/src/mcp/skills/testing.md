---
name: testing
description: How to test Roblox code: pure-logic unit tests (TestEZ, Jest-Lua), structuring code for testability, Studio test modes (Test, Run, Server and Clients, Team Test), safe DataStore and purchase testing, and CI via Open Cloud Luau execution. Pull it before writing tests or verifying multiplayer behaviour.
---

# Testing Roblox code

## 1. Make logic testable first

Most bugs live in logic that never needed an Instance. Split code in two layers:

- Pure ModuleScripts: take plain tables and numbers, return plain tables and numbers. No `game:GetService` at module top level, no `workspace`, no `task.wait`. Damage formulas, inventory rules, cooldown math, reward tables, data migrations, receipt idempotency, rate limiters.
- Thin glue Scripts: read Instances/events, call the pure layer, apply results.

Inject what you cannot avoid (clock, random, data store) as parameters.

```lua
--!strict
-- Inventory.luau (pure)
export type Inv = { [string]: number }
local Inventory = {}

function Inventory.add(inv: Inv, item: string, n: number): Inv
	assert(n > 0 and math.isfinite(n) and n == math.floor(n), "bad count")
	local out = table.clone(inv)
	out[item] = (out[item] or 0) + n
	return out
end

function Inventory.remove(inv: Inv, item: string, n: number): (Inv?, string?)
	if (inv[item] or 0) < n then return nil, "not enough" end
	local out = table.clone(inv)
	out[item] -= n
	if out[item] == 0 then out[item] = nil end
	return out
end

return Inventory
```

Rules: return new tables instead of mutating (easier to assert on), return `(value?, errString?)` instead of erroring for expected failures, keep `--!strict` on so type errors surface before run time.

## 2. Unit-test frameworks

Both run inside Roblox (Studio or a headless server) because they need the Luau runtime with Roblox's `ModuleScript`/`require`.

- Jest-Lua (described by its repo as "Delightful testing for Luau", Jest-style `describe`/`it`/`expect`; repository was still receiving commits in October 2026): https://github.com/Roblox/jest-roblox . Check its README for the current install method and runner (source: that repository).
- TestEZ (BDD-style, `describe`/`it`/`expect`): https://github.com/Roblox/testez . The repository is archived and was last updated in March 2024, so prefer Jest-Lua for new work and keep TestEZ for existing suites (source: GitHub repo metadata at time of writing).

TestEZ style:

```lua
--!strict
-- Inventory.spec.luau (next to Inventory.luau)
return function()
	local Inventory = require(script.Parent.Inventory)

	describe("add", function()
		it("adds a new item", function()
			expect(Inventory.add({}, "sword", 1).sword).to.equal(1)
		end)
		it("rejects NaN counts", function()
			expect(function() Inventory.add({}, "sword", 0/0) end).to.throw()
		end)
	end)
end
```

Conventions: one `.spec` ModuleScript per module, placed beside it or in a `Tests` folder; keep specs out of `StarterPlayer`/`ReplicatedStorage` paths that ship to clients in production (put them in `ServerStorage` or a folder your build strips), because anything replicated can be read by exploiters (see the security skill).

Run it from a runner Script in Studio's command bar or a Script in `ServerScriptService` that requires the framework, points it at your spec folder, prints the result, and reports pass/fail (the framework README shows the exact call).

## 3. Roblox's built-in TestService

`TestService` is a service that Roblox uses internally. It has assertion-style helpers that print coloured output: `Check(condition, description)`, `Require(condition, description)` (ends the test on failure), `Error`, `Fail`, `Warn`, `Message`, `Done()` ("Testing Done"), plus `Timeout`, `ErrorCount`, `WarnCount` and `NumberOfPlayers` properties. Scripts parented to `TestService` can be run with `TestService:RunAsync()` (plugin security; `Run` is deprecated) (source: https://create.roblox.com/docs/reference/engine/classes/TestService). Good for quick smoke scripts; use a framework for real suites.

## 4. Studio test modes (source: https://create.roblox.com/docs/en-us/studio/testing-modes)

| Mode | Key | What it does |
| --- | --- | --- |
| Test | F5 | Simulates the game and inserts your avatar at a `SpawnLocation` (or about (0, 100, 0)). Studio runs a separate client and server simulation; a toggle switches the view between Client and Server |
| Test Here | | Same but spawns the avatar in front of the camera |
| Run | F8 | Simulates the game with no avatar, you fly the Studio camera. Good for server-only logic, physics and NPCs |
| Stop | Shift+F5 | Stops and resets everything to pre-test state |
| Server & Clients | F7 | Launches one server session and 1 to 8 client sessions (1 or 2 is usually enough) |
| Team Test | | Collaborative testing: a Studio session with your character, joined by teammates. One team test at a time |

Practical use:

- Single-player logic: Test (F5), switch Client/Server view to inspect both sides.
- Replication bugs, remotes, anything where another player's view matters: Server & Clients. Test what client B sees when client A acts, join/leave order, `PlayerRemoving` saves, spawn handling.
- Server-only logic with no avatar: Run.
- Pause/step: pause only the client or only the server and step 1/60 s to catch race conditions.
- Network conditions: Network Simulator adds latency, jitter and packet loss to playtest connections (source: testing-modes page and https://create.roblox.com/docs/en-us/studio/network-simulator). Test remotes and prediction with 200+ ms.
- Devices: Device Simulator for screens, touch and gamepad (see the input-and-devices skill).
- Party APIs: the Party Simulator assigns test players to simulated parties.
- Secrets are unavailable in local playtests, only in live servers or collaborative testing (source: https://create.roblox.com/docs/en-us/cloud-services/secrets). Add Local Secrets in Experience Settings for local work.

Programmatic runs (plugins): `StudioTestService` can start Test or Run mode (`ExecutePlayModeAsync`, `ExecuteRunModeAsync`), launch a multiplayer test with a number of simulated clients (`ExecuteMultiplayerTestAsync(numPlayers, args)`), add players mid-session, pass args (`GetTestArgs`) and end the test from the server with `EndTest(value)`. Use it to build repeatable, automated multi-client tests inside Studio (source: https://create.roblox.com/docs/reference/engine/classes/StudioTestService).

Detect where code is running with `RunService:IsStudio()`, `IsServer()`, `IsClient()`, `IsRunMode()`, `IsEdit()`.

## 5. Testing data stores safely

- Studio cannot reach data stores unless "Enable Studio Access to API Services" is on, and when on, Studio reads and writes the same live data stores as the published game. Do not enable it on a live experience (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores).
- Best: a separate test place in a separate universe with its own data stores.
- Alternative: wrap data access behind an interface (`load(userId)`, `save(userId, data)`), and in tests inject an in-memory fake that can also simulate failures (throw on Nth call, return nil, be slow). Test retry/backoff, failed load (must not overwrite), `PlayerRemoving` and `BindToClose` paths this way. Then run a small integration pass against the real service in the test universe.
- Prefix or namespace store names in Studio (for example `"Test_" .. name` when `RunService:IsStudio()`) so a mistake cannot touch production.
- MemoryStore data is isolated between Studio and production, so Studio playtests do not touch live memory stores (source: https://create.roblox.com/docs/en-us/cloud-services/memory-stores).
- Delete test data when finished (Data Stores Manager).

## 6. Testing purchases

Roblox's docs I could find do not describe a dedicated test-purchase mode. So:

- Unit-test grant logic and `ProcessReceipt` by calling the handler with a fabricated receipt table (`PurchaseId`, `PlayerId`, `ProductId`, `CurrencySpent`), twice, to prove idempotency, and with failing saves to prove it returns `NotProcessedYet`.
- Studio provides test subscription ids `EXP-0`, `EXP-1`, `EXP-2` for subscription states (source: https://create.roblox.com/docs/reference/engine/classes/MarketplaceService).
- Verify real prompts end to end on a published test place with a test account before launch. See the monetization skill.

## 7. CI and headless runs

- Open Cloud Luau Execution: run a Luau script headlessly in a place on a Roblox server through the API, get return values and logs back. Documented use cases include automated testing against specific place versions in CI/CD. Tasks run up to 5 minutes, with a limit of 10 concurrent tasks per place. Flow: create a task with universeId, placeId and the script; poll for completion; read results and logs. Authenticate with an API key (`x-api-key`) or OAuth 2.0 (source: https://create.roblox.com/docs/en-us/cloud/reference/features/luau-execution , walkthrough: https://create.roblox.com/docs/en-us/cloud/guides/instance). Build your Jest-Lua/TestEZ runner as the script, return a pass/fail table, and fail the pipeline when the table says so.
- Keep secrets (API keys) in CI secret storage, never in the repo. Rate-limit CI against shared quotas; it shares experience data store budgets if it touches data stores.
- run-in-roblox (https://github.com/rojo-rbx/run-in-roblox): community tool that runs a place or script inside Roblox Studio and pipes output to stdout; needs a Studio install. Its repository has not been updated since March 2024, so check it still works with your Studio version (source: its README and repository metadata).
- A file-sync tool such as Rojo (community) lets you keep code and specs as files in git so CI can build a place file to run.

## Checklist before shipping

- Pure logic has unit tests, including NaN, inf, negative, empty and very large inputs.
- Multi-client test done for every feature involving remotes or shared state.
- Data layer tested with injected failures; shutdown save path exercised.
- Receipt handler tested for duplicate delivery.
- Tested at high latency and on at least one phone profile and one gamepad.
- Test code does not ship in replicated containers.
