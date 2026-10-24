---
name: data-persistence
description: How to save and load player and game data safely with DataStoreService, OrderedDataStore and MemoryStoreService: budgets, retries, session locking, shutdown saves, schema versioning. Pull it before writing any code that persists data or shares data across servers.
---

# Data persistence

## Basics

```lua
--!strict
local DataStoreService = game:GetService("DataStoreService")
local store = DataStoreService:GetDataStore("PlayerData")  -- one store for all players
local key = "User_" .. 12345                                 -- stable id, never DisplayName
```

- `GetAsync(key)` read. `SetAsync(key, value)` blind overwrite. `UpdateAsync(key, fn)` read-modify-write. `IncrementAsync(key, delta)` atomic number add. `RemoveAsync(key)` delete.
- Every call yields and can fail. Always wrap in `pcall`.
- Values must be serializable: numbers, strings, booleans, tables of those. Never save Instances, Vector3, CFrame, Color3 or functions directly. Serialize them yourself (for example `{x, y, z}` or a hex string). Table keys must not be NaN or nil, strings must be valid UTF-8 (source: https://create.roblox.com/docs/en-us/scripting/security/client-server-boundary).
- Prefer `UpdateAsync` whenever a write depends on the current value or several servers might write the key. Use `SetAsync` for brand-new keys or values that ignore the old one. `UpdateAsync` consumes both the read and write budgets (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/best-practices).
- Inside the `UpdateAsync` callback do not yield. Return `nil` to cancel the write.

## Layout

- Few data stores, many keys. One `PlayerData` store, one key per player like `User_{UserId}`. Static key patterns also let automated right-to-be-forgotten processing find player data (source: best-practices page above).
- Keep data that must change together in the same key.
- Do not create a data store per player (that is the DataStore2 pattern, which Roblox calls legacy and says not to use for new experiences).
- Buffer in memory: load once on join, mutate a server-side table during play, write on a schedule, on leave and on shutdown. Do not hit the data store per gameplay event (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/player-data-purchasing).

## Limits (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/error-codes-and-limits)

| Item | Limit |
| --- | --- |
| Data store name, key name, scope | 50 characters each |
| Value per key | 4,194,304 characters (about 4 MB), measured on the JSON-encoded form |
| Metadata | 300 pairs, key up to 50, value up to 250 characters |
| Per-key throughput | 25 MB per minute read, 4 MB per minute write (rounded up to the next KB per request) |
| Server-wide read requests per minute | 300 + concurrent users x 40 |
| Server-wide write requests per minute | 300 + concurrent users x 20 |
| List requests per minute | 300 + concurrent users x 2 |
| Remove requests per minute | 300 + concurrent users x 40 |
| Ordered stores | same read/write/list/remove formulas as above |
| Queued requests | 30 per queue (set, ordered set, get, ordered get). Beyond that, requests are dropped with error codes in the 301 to 306 range |
| Storage per experience | 500 MB + 1 MB x lifetime users (compressed size of latest versions) |

Throttled calls surface as `DatastoreThrottled` or `KeyThrottled`. Read `DataStoreService:GetRequestBudgetForRequestType(Enum.DataStoreRequestType.X)` before bursts, and use `SetRateLimitForRequestType` to cap one server's share. Open Cloud traffic shares the same budget as game servers.

## Retry with backoff

Retry transient errors with capped exponential backoff and jitter. Do not retry errors caused by invalid data (too large, unserializable). Retry in order per key: an old retry landing after a newer write can overwrite it. A failed call may still have succeeded on the backend, so make writes idempotent (source: best-practices page above).

```lua
--!strict
local function withRetry<T>(fn: () -> T, attempts: number): (boolean, T | string)
	local delay = 1
	local lastErr = ""
	for i = 1, attempts do
		local ok, res = pcall(fn)
		if ok then return true, res end
		lastErr = tostring(res)
		if i < attempts then
			task.wait(delay + math.random())
			delay = math.min(delay * 2, 16)
		end
	end
	return false, lastErr
end
```

Stagger recurring requests: give each server or player a random initial offset so servers do not all save at the same second.

## Default template, deep copy, versioning

```lua
--!strict
local DEFAULT = { version = 2, coins = 0, inventory = {} :: { [string]: number }, settings = { music = true } }

local function deepCopy(t: any): any
	if type(t) ~= "table" then return t end
	local out = {}
	for k, v in t do out[k] = deepCopy(v) end
	return out
end

local function reconcile(data: any, template: any)  -- add missing fields, keep extras
	for k, v in template do
		if data[k] == nil then data[k] = deepCopy(v)
		elseif type(v) == "table" and type(data[k]) == "table" then reconcile(data[k], v) end
	end
end

local migrations: { [number]: (any) -> () } = {
	[1] = function(d) d.settings = { music = true }; d.version = 2 end,
}

local function load(raw: any?): any
	local data = if raw then raw else deepCopy(DEFAULT)
	while (data.version or 1) < DEFAULT.version do migrations[data.version or 1](data) end
	reconcile(data, DEFAULT)
	return data
end
```

Do: store a `version` number in every record, migrate on load, never mutate `DEFAULT` itself (deep copy, or every new player shares one table). Don't: rename or repurpose fields without a migration.

## Session locking

Problem: when a player hops servers, the old server may not have finished its final save when the new server loads. The new server reads stale data, then the two overwrite each other, which causes data loss and duplication exploits.

Solution (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/player-data-purchasing): on first read, atomically write a lock id into the key's metadata inside `UpdateAsync`. Another server that sees a fresh foreign lock refuses to load (or waits and retries). Release the lock in the final save. Refresh it on the autosave loop so it does not expire; an expired lock may be taken over.

Roblox publishes a sample `SessionLockedDataStoreWrapper` in that page. Community libraries implement this too:
- ProfileStore by MadStudioRoblox (community, not Roblox): https://github.com/MadStudioRoblox/ProfileStore , the successor to ProfileService (https://github.com/MadStudioRoblox/ProfileService). Both do autosave, session locking and shutdown handling.
- Roblox's best-practices page says to review a third-party module's ownership, maintenance and features, and to know how to read and migrate your data without it.

If you write your own, you need all of: lock on load, refresh on interval, release on final save, and refuse to save when the lock was lost.

## When to save

- `Players.PlayerRemoving`: save and release.
- Periodic autosave: pick an interval that fits your budget (it scales with concurrent users) and jitter it per player. Many projects use roughly 1 to 5 minutes; that range is a judgment call, not a documented number.
- Critical checkpoints: right after a purchase is granted.
- `game:BindToClose`: the server waits only 30 seconds for bound functions to finish before shutting down (source: https://create.roblox.com/docs/reference/engine/classes/DataModel). Save every player in parallel (spawn a thread per player, wait for all), not one after another.

```lua
--!strict
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

game:BindToClose(function()
	if RunService:IsStudio() then return end -- optional: skip so Studio stops quickly
	local pending = 0
	for _, p in Players:GetPlayers() do
		pending += 1
		task.spawn(function()
			saveAndRelease(p) -- your function, must pcall internally
			pending -= 1
		end)
	end
	while pending > 0 do task.wait() end
end)
```

PlayerRemoving can fire after BindToClose has already started saving that player. Track a per-player "saved/released" flag so you do not save twice.

## OrderedDataStore (leaderboards)

Values must be positive integers; use `GetSortedAsync(ascending, pageSize, min, max)` and read pages with `GetCurrentPage()`/`AdvanceToNextPageAsync()` (source: https://create.roblox.com/docs/reference/engine/classes/OrderedDataStore). Keep full profile data in a normal store and only the score in the ordered store. Cache the top list on the server and refresh every minute or so; do not query per player join.

## MemoryStoreService (ephemeral, cross-server)

For data that is short-lived and shared by servers: matchmaking queues, session/lobby lists, rate-limit counters, event state. Not for anything that must survive (source: https://create.roblox.com/docs/en-us/cloud-services/memory-stores).

- Structures: `MemoryStoreSortedMap`, `MemoryStoreQueue`, `MemoryStoreHashMap`.
- Every item has an expiration. Default is 45 days; the allowed range is 0 to 3,888,000 seconds. Set the smallest TTL that works, and delete explicitly when done.
- Request units per minute: 1000 + 120 x concurrent users, shared across all calls. `GetRangeAsync` and `ReadAsync` cost per item returned.
- Memory quota: 64 KB + 1.2 KB x users. One sorted map or queue: max 1,000,000 items and 100 MB.
- Sorted map: key up to 128 characters, value up to 32 KB, sort key up to 128 characters (source: https://create.roblox.com/docs/en-us/cloud-services/memory-stores/sorted-map).
- Single partition throttles at roughly 30,000 request units per minute; shard hot structures.
- Handle `DataUpdateConflict` with exponential backoff.
- Studio and production memory stores are isolated, so testing in Studio does not touch live data.

## Studio and live data warning

Studio is blocked from data stores by default. Enabling "Enable Studio Access to API Services" (File > Experience Settings > Security) makes Studio read and write the same data stores as the live game. Do not turn it on for a live experience; use a separate test place/universe (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores). Guard so tests never use production keys, e.g. prefix the store name when `RunService:IsStudio()`. If access is off, calls error: pcall them and fall back to defaults so Studio play still works.

## Privacy: right to be forgotten

If you store user data, define deletion templates so Roblox can erase it when a user requests erasure. Use a static key/scope pattern containing `{UserId}` (up to 100 templates, standard or ordered stores), set in Data Stores Manager or the Open Cloud Configs API. If your schema cannot be matched by pattern, use the right-to-erasure webhook (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/right-to-be-forgotten). Keys named by display name or random ids cannot be matched; use `User_{UserId}`.

## Rules of thumb

- Load fails: do not overwrite. If the read errored, kick the player or run with a "do not save" flag. Saving defaults over a failed load wipes their progress.
- Never save from the client side or trust client values going into the save (see the security skill).
- Keep one in-memory table per player on the server; replicate only what the UI needs.
- Observe with the Data Stores Observability dashboard and alerts (source: best-practices page).
