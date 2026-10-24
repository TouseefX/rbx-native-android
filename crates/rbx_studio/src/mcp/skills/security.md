---
name: security
description: Server-authority rules for Roblox games: validating remotes, rate limiting, what replicates and what an exploiter can see, network ownership, secrets, HTTP and text filtering. Pull it before writing any RemoteEvent/RemoteFunction handler, shop, combat, currency or HTTP code.
---

# Security: never trust the client

Assume every player controls their own client completely. They can read any replicated script, fire any remote at any rate with any arguments (except the first `Player` argument, which the engine sets), move their own character anywhere, and change their local DataModel without any event firing (source: https://create.roblox.com/docs/en-us/scripting/security/security-tactics).

The only fix is design: the server owns state and decides. The client renders and sends intent ("I pressed buy on item X"), never results ("I now have 500 coins").

## What replicates and what does not

- Instances and properties are server-authoritative. A client changing a Part or a GUI value changes only its own copy, other players never see it (source: https://create.roblox.com/docs/en-us/scripting/security/access-control).
- Exception: the client simulates physics for parts it has network ownership of, including its own character. Position, velocity and Humanoid properties like `WalkSpeed` set locally are accepted as the truth for that physics.
- Anything that reaches a client can be read: LocalScripts, client-context scripts and every ModuleScript in a replicated container can be decompiled even if never required. Scripts in `ServerScriptService` and `ServerStorage` are not sent to clients (source: access-control page above).

Rules:

- Keep server logic, prices, drop tables, admin lists and anything confidential in `ServerScriptService` or `ServerStorage`. Never in `ReplicatedStorage` or `Workspace`.
- Do not write one ModuleScript with `if RunService:IsServer()` branches. The server half ships to the client and can be read.
- Never hardcode secrets, passwords or API keys in any script that replicates.
- Unreleased content you put in a replicated container leaks even if gated by code. Keep dev/test places in a separate private universe.
- Restricted subplaces: a client can teleport itself to any place in the universe. Set Access Control for Places to "Secure within universe only" in the Creator Dashboard so only server-initiated teleports work (source: access-control page above).

## Validating a remote handler

Every argument is hostile. Validate in this order (source: https://create.roblox.com/docs/en-us/scripting/security/client-server-boundary):

1. Type and structure: `typeof(x)` for every argument. Cap string length and table size. A table can mimic an Instance, so check `typeof(item) == "Instance"` and where it lives (for example `IsDescendantOf` your item folder).
2. Value: numbers can be NaN or infinity and both have `typeof == "number"`. NaN fails every comparison, so `n < 0 or n > max` does not reject it. Use `math.isfinite`. Check integer-ness, sign, upper bound.
3. Context: is the character alive, close enough to the shop or target, does the player own the item or have the resource, is the action allowed in the current game state.
4. Rate: per-player cooldown or token bucket.

Do:

```lua
--!strict
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local Players = game:GetService("Players")

local buy = ReplicatedStorage:WaitForChild("BuyItem") :: RemoteEvent
local MAX_SHOP_DISTANCE = 20
local lastBuy: { [Player]: number } = {}

buy.OnServerEvent:Connect(function(player: Player, itemId: unknown, quantity: unknown)
	if typeof(itemId) ~= "string" or #itemId > 32 then return end
	if typeof(quantity) ~= "number" or not math.isfinite(quantity) then return end
	if quantity ~= math.floor(quantity) or quantity < 1 or quantity > 99 then return end

	local now = os.clock()
	if now - (lastBuy[player] or 0) < 0.25 then return end
	lastBuy[player] = now

	local root = player.Character and player.Character:FindFirstChild("HumanoidRootPart") :: BasePart?
	if not root or (root.Position - workspace.Shop.Position).Magnitude > MAX_SHOP_DISTANCE then return end

	-- price comes from a server-side table, never from the client
	-- charge(player, itemId, quantity)
end)

Players.PlayerRemoving:Connect(function(p) lastBuy[p] = nil end)
```

Don't: accept a price, damage amount, or "new balance" from the client. Don't use a client-supplied Instance as a source of truth (for example a NumberValue holding a price). Don't let a remote take an arbitrary path or asset id for the server to load or `require` (source: client-server-boundary page).

## Rate limiting

Anything a client can trigger (remotes, ProximityPrompt, ClickDetector, Touched) can be spammed. Docs recommend a token bucket per UserId: capacity N, refill N/window per second, spend one token per action, clear the bucket on `PlayerRemoving` (source: client-server-boundary page, full module there). Cheap alternative: a last-fired timestamp per player as above.

## Prompts and detectors are not remotes but need the same care

An exploiter can fire `ProximityPrompt` and `ClickDetector` events from any distance, even when disabled. Only `ProximityPrompt.Triggered` has a server distance check. Re-check enabled state, player state, distance and rate in your handler (source: client-server-boundary page).

## Server must be a gatekeeper, not a relay

Do not write `remote.OnServerEvent:Connect(function(p, pos) remote:FireAllClients(pos) end)`. Validate, rate limit, check permission, then broadcast (source: client-server-boundary page, "Client to client communication").

## RemoteFunction: never InvokeClient from the server

`RemoteFunction:InvokeClient` throws if the client disconnects mid-call and yields forever if the client never returns (source: https://create.roblox.com/docs/reference/engine/classes/RemoteFunction). An exploiter can simply not answer. Use a `RemoteEvent` pair instead (server fires, client replies by firing back, server handles the reply with a timeout via `task.delay`). `InvokeServer` from the client is fine: the server handler must still validate and must not yield on anything the client controls.

## UnreliableRemoteEvent

One-way, unordered, may be dropped and never resent. Use it for ephemeral or continuously changing data (cosmetic effects, position hints, aim direction). Do not use it for purchases, state changes or anything that must arrive. Payloads over 1000 bytes are dropped. Messages are discarded if no listener is connected when they arrive (source: https://create.roblox.com/docs/reference/engine/classes/UnreliableRemoteEvent and https://create.roblox.com/docs/en-us/scripting/events/remote). Same validation rules apply on the server side.

## Network ownership and movement

A client with ownership of a part or assembly (its character by default, nearby unanchored parts automatically) can teleport, fly, change speed, and put NaN/inf into CFrames and velocities (source: https://create.roblox.com/docs/en-us/scripting/security/network-ownership).

- Anchor anything gameplay-critical that does not need physics. Pickups, doors and prompts on unanchored parts can be dragged to the exploiter.
- For critical unanchored assemblies call `part:SetNetworkOwner(nil)` (server owns) or set the owner explicitly. Only unanchored parts can have an owner set.
- Movement validation: compare the server-seen position over time against max speed, project onto the XZ plane for ground games, add latency tolerance, exempt legitimate teleports. A simple distance check false-positives on vehicles and lag. Roblox says Server Authority (beta at time of writing) is the robust fix.
- Respond softly: rubber-band to the last valid position instead of kicking. Use a suspicion score across several heuristics (rate of gain, fastest completion time, action cadence) before banning, and use `Players:BanAsync` for persistent offenders (source: https://create.roblox.com/docs/en-us/scripting/security/server-side-detection).
- Honeypot remotes (a remote no legitimate client script fires) are a cheap signal: log, do not instantly ban.

## Authoritative economy

- Currency, inventory, health and damage live on the server (in a table or server-only data store session), not in client-readable `IntValue`s that the client could be trusted to update.
- Design so cheating is pointless: sequential server-verified obby checkpoints, server-side damage from server weapon stats, no points for killing freshly spawned players (source: https://create.roblox.com/docs/en-us/scripting/security/defensive-design).
- Combat: client sends origin and claimed hit; server checks origin near the character, hit near the claimed part, no static obstruction, fire rate, ammo, team, alive (source: client-server-boundary page).
- Purchases: grant only inside `ProcessReceipt` (see the monetization skill). Never trust `PromptProductPurchaseFinished` from the client.
- Data store writes from client input: reject Instances, unbounded strings, NaN keys, invalid UTF-8 (`utf8.len(s) ~= nil`), and trades that can half-complete (source: client-server-boundary page, "Data store manipulation").

## HTTP and secrets

- `HttpService` requests work only from server scripts. Enable Allow HTTP Requests in Experience Settings.
- Never put API keys in scripts, ReplicatedStorage or data stores. Use the Secrets store (Creator Dashboard, up to 500 secrets per experience, 1,024 characters each, bound to a domain) and `HttpService:GetSecret("name")` in a server script. Printing it shows only `Secret(name)`. Secrets go in URL or headers, not the request body. They are not available in local playtests, add local secrets under File > Experience Settings > Security (source: https://create.roblox.com/docs/en-us/cloud-services/secrets).

```lua
--!strict
local HttpService = game:GetService("HttpService")
local res = HttpService:RequestAsync({
	Url = "https://my.example.com/endpoint",
	Method = "GET",
	Headers = { ["x-api-key"] = HttpService:GetSecret("APIKey") :: any },
})
```

## Text filtering

Any user-typed text that other players can see must be filtered by the server, including pet names, signs, custom chat, and text loaded back from data stores or web servers. Roblox moderates games that skip this (source: https://create.roblox.com/docs/en-us/ui/text-filtering).

- `TextService:FilterStringAsync(text, fromUserId)` returns a `TextFilterResult`. Wrap in `pcall` (it can fail).
- `result:GetNonChatStringForBroadcastAsync()` for text shown to everyone (signs, names).
- `result:GetNonChatStringForUserAsync(toUserId)` for text shown to one specific user.
- Filter after the user submits, never per keystroke.
- Store the raw text, filter on display, because filter results depend on the viewer.

```lua
--!strict
local TextService = game:GetService("TextService")
local function filterForAll(text: string, fromUserId: number): string?
	local ok, result = pcall(function()
		return TextService:FilterStringAsync(text, fromUserId)
	end)
	if not ok then return nil end
	local ok2, out = pcall(function()
		return (result :: TextFilterResult):GetNonChatStringForBroadcastAsync()
	end)
	return if ok2 then out else nil
end
```

## Threat-model checklist for each feature

What if the client sends nil, NaN, a huge string, or a wrong type? What if it fires 1,000 times a second? What if the player is dead, far away, or just left? What is the worst a malicious value can do to other players? (source: security-tactics page.) Also see third-party asset risks: free models can contain backdoor scripts, audit them (source: https://create.roblox.com/docs/en-us/scripting/security/third-party-vulnerabilities).
