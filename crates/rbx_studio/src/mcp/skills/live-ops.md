---
name: live-ops
description: AnalyticsService events (economy, funnel, custom, custom fields, limits) and ConfigService live configs and A/B experiments. Pull it before adding telemetry, tunable values, feature flags or an experiment.
---

# Live ops: analytics, configs and experiments

Both systems run on the server only. Neither does anything in Studio play sessions for production data: analytics events are only accepted from published games (source: https://create.roblox.com/docs/en-us/production/analytics/economy-events), and live config values are only visible to players after you publish them (source: https://create.roblox.com/docs/en-us/production/configs).

## 1. Analytics with AnalyticsService

### Ground rules

- Server only. Events "can't be sent from the client or Studio" (source: https://create.roblox.com/docs/en-us/production/analytics/funnel-events). Put calls in server scripts, never in a LocalScript or a shared module that a client runs.
- Log after the thing succeeded. Call `LogEconomyEvent` after the purchase went through, not when the player tries (source: https://create.roblox.com/docs/en-us/production/analytics/event-types).
- Global limit: 120 + 20 * CCU requests per minute across all AnalyticsService calls. Batch high-frequency things into one event with a value (for example "10 zombies killed" as one `LogCustomEvent` with value 10) instead of ten calls (source: event-types page above, and https://create.roblox.com/docs/en-us/production/analytics/custom-events).
- Event data stays on the dashboard until 90 days after the last data received (source: event-types page).
- Use the current `Log*` methods. `FireEvent`, `FireCustomEvent`, `FireInGameEconomyEvent`, `FireLogEvent` and `FirePlayerProgressionEvent` are deprecated (source: https://create.roblox.com/docs/en-us/reference/engine/classes/AnalyticsService).

### Economy events

```
AnalyticsService:LogEconomyEvent(player, flowType, currencyType, amount, endingBalance, transactionType, itemSku?, customFields?)
```

- `flowType`: `Enum.AnalyticsEconomyFlowType.Source` (currency added) or `.Sink` (currency removed).
- `amount` is always positive, for sources and sinks alike. The dashboard shows sinks as negative.
- `endingBalance` is the balance after the transaction. It feeds the average wallet chart.
- `transactionType` is a string. Default names come from `Enum.AnalyticsEconomyTransactionType`: `IAP`, `TimedReward`, `Onboarding`, `Shop`, `Gameplay`, `ContextualPurchase`. Pass `.Name`. You may use your own string.
- `itemSku` is your stable id for the item or bundle. If omitted, the dashboard shows `N/A`.
- Limits: 10 currency types; transaction types beyond 20 and item SKUs beyond 100 are grouped as "Other".

```lua
--!strict
local AnalyticsService = game:GetService("AnalyticsService")

local function logShopSink(player: Player, sku: string, cost: number, balanceAfter: number)
	AnalyticsService:LogEconomyEvent(
		player,
		Enum.AnalyticsEconomyFlowType.Sink,
		"Coins",
		cost,
		balanceAfter,
		Enum.AnalyticsEconomyTransactionType.Shop.Name,
		sku
	)
end
```

(source: https://create.roblox.com/docs/en-us/production/analytics/economy-events)

Do: one currency name `XP`, with class in a custom field. Don't: `WarriorXP`, `MageXP`, `PaladinXP` as separate currencies, since that burns the 10-currency limit (source: event-types page).

### Funnel events

- One-time funnel (onboarding): `LogOnboardingFunnelStepEvent(player, step, stepName, customFields?)`.
- Recurring funnel (shop, upgrade): `LogFunnelStepEvent(player, funnelName, funnelSessionId, step?, stepName, customFields?)`.
- Limits: 10 funnels, 100 steps per funnel. Log each step after the player completes it. Only the first occurrence of a step per funnel session counts. A funnel starts when its first step is logged; log step 1 in `PlayerAdded` if you want it to start on join. A player's saved step expires after 180 days without events.
- `funnelSessionId` tells separate runs of a recurring funnel apart. For a shop that can open many times, use `HttpService:GenerateGUID()` per opening. For a long-lived path like an item upgrade, build a key such as `<playerId>-<itemId>`. Analytics keeps only each user's 10 most recent session ids per funnel; reusing an older id counts as a new session. One-time funnels do not need it.

```lua
--!strict
local AnalyticsService = game:GetService("AnalyticsService")
local HttpService = game:GetService("HttpService")

local sessions: { [Player]: string } = {}

local function shopStep(player: Player, step: number, name: string)
	if step == 1 then
		sessions[player] = HttpService:GenerateGUID()
	end
	local id = sessions[player]
	if id then
		AnalyticsService:LogFunnelStepEvent(player, "ShopCheckout", id, step, name)
	end
end
```

(source: https://create.roblox.com/docs/en-us/production/analytics/funnel-events)

Also exist and are current: `LogProgressionStartEvent`, `LogProgressionCompleteEvent`, `LogProgressionFailEvent` and `LogProgressionEvent` (level or stage progress, with `progressionPathName`, `level`, `levelName`). I did not find a dedicated guide page for them; check the class page before relying on dashboard behavior (source: AnalyticsService class page above).

### Custom events

`LogCustomEvent(player, eventName, value?, customFields?)`. No value makes it a counter (value 1 for aggregations). With a value you get count, unique users, avg, sum, min, max, avg per user. Limit: 100 event names (source: https://create.roblox.com/docs/en-us/production/analytics/custom-events).

Do: one event `PlantSeed` with the plant in a custom field. Don't: `PlantCabbage`, `PlantTurnip`, ... as separate event names, because event names have a much tighter cardinality limit than custom field values.

### Custom fields

Up to 3 per event, on economy, funnel and custom events. Keys must be `Enum.AnalyticsCustomFieldKeys.CustomField01/02/03` `.Name`; other keys are ignored. Values must be strings (convert numbers and booleans with `tostring`). After 8,000 combined values across the three fields, further values are grouped as "Other" (source: https://create.roblox.com/docs/en-us/production/analytics/custom-fields).

```lua
--!strict
local AnalyticsService = game:GetService("AnalyticsService")

local function logHarvest(player: Player, plant: string, level: number)
	AnalyticsService:LogCustomEvent(player, "HarvestPlant", 1, {
		[Enum.AnalyticsCustomFieldKeys.CustomField01.Name] = plant,
		[Enum.AnalyticsCustomFieldKeys.CustomField02.Name] = tostring(level),
	})
end
```

Don't put player ids, timestamps or free text in fields or event names. Unbounded values hit the 8,000 cap and turn into "Other".

### What to instrument first

1. Onboarding funnel: 4 to 8 steps from join to first meaningful action. This is where most players are lost (source: funnel-events page).
2. Economy: every place a currency is earned (sources) and spent (sinks), with transaction types and SKUs. Aim for net sources minus sinks near zero (source: economy-events page).
3. Shop funnel with a GUID session per opening.
4. One or two custom events on the core loop (for example a round finished, with duration as the value and map as a custom field).

### Reading results

Creator Hub, your experience, Analytics: Economy, Funnel and Explore (custom events) pages. Charts take about 24 hours to appear. For a quick check use **View Events** on each page for a near real-time list, and the experience Error Report for tracking errors (source: https://create.roblox.com/docs/en-us/production/analytics/event-types). Segment players at runtime with `AnalyticsService:GetPlayerSegmentsAsync(player)` (server only, yields, returns `HasData` plus bucket enums; check `HasData` first) (source: AnalyticsService class page).

## 2. ConfigService: live configs

Configs are key/value pairs (string, number, boolean, JSON) that you change in Creator Hub or Studio (File > Open Configs) and publish without a server restart or a new place version. Up to 1,000 active configs. String and JSON values max 100,000 characters (source: https://create.roblox.com/docs/en-us/production/configs).

### Reading values

```lua
--!strict
local ConfigService = game:GetService("ConfigService")

local DEFAULT_BOSS_HEALTH = 500

local snapshot = ConfigService:GetConfigAsync()   -- yields
local function bossHealth(): number
	local v = snapshot:GetValue("bossHealth")
	return if typeof(v) == "number" then v else DEFAULT_BOSS_HEALTH
end

snapshot.UpdateAvailable:Connect(function()
	snapshot:Refresh()
end)
snapshot:GetValueChangedSignal("bossHealth"):Connect(function(new)
	-- apply at a safe moment, e.g. next boss spawn
end)
```

Facts, all from the configs guide and class pages (https://create.roblox.com/docs/en-us/production/configs, https://create.roblox.com/docs/en-us/reference/engine/classes/ConfigService, https://create.roblox.com/docs/en-us/reference/engine/classes/ConfigSnapshot):

- `ConfigService` is server only; calling it from a client script errors.
- `GetConfigAsync()`: global snapshot, no targeting, no experiments. `GetConfigForPlayerAsync(player)`: applies conditional rules and experiments for that player. Call it per player.
- A snapshot is a point in time. Updates do not reach it by themselves. Listen to `UpdateAvailable`, then call `Refresh()` when it is safe (between rounds, not mid-match). `GetValueChangedSignal(key)` fires after a refresh changes that key.
- `GetValue(key)` returns nil for a missing key. Always keep a code default and type-check the result.
- `GetConfigAsync()` throws if config never loaded. If it loaded before and the connection drops, you get the latest values. Wrap the first call in `pcall` and fall back to code defaults.
- Publishing: "Publish now" reaches servers in roughly 15 seconds to 1 minute; "Publish over 15 min" is gradual. Staged values are visible only to you and your team in Studio.
- Testing: `ConfigService:SetTestingValue(key, value)` and `ClearTestingValue(key)` override a key for the current server only.
- Changes made through Open Cloud use the `InExperienceConfig` repository (source: https://create.roblox.com/docs/en-us/cloud/guides/configs).

### Configs vs constants vs DataStore

- Config: designer-tunable values that apply to everyone or to a segment (prices, drop rates, event start flags, kill switches).
- Hard-coded constant: values that must never change without a code review, or that are security-sensitive.
- DataStore: per-player saved state. Configs are not per-player storage. If an experiment value must stay fixed after a player stops being eligible, persist it yourself in a DataStore (source: https://create.roblox.com/docs/en-us/production/experiments).

### Targeting

Conditional configs choose values by country, language, new vs returning, source, tenure, payer status, activity, engagement. Rules are evaluated top to bottom, first match wins, otherwise the default. Limits: 100 conditions per game, 20 per key, and each conditional value must match the default's type (configs page).

## 3. A/B experiments

In-game experiments test variant values of a config key (source: https://create.roblox.com/docs/en-us/production/experiments).

1. Create the config key with its control value.
2. Creator Hub, Experiments, Create experiment, type In-experience. Set goal metric, duration (14 to 60 days), rollout percent, up to two variants plus control. A plain 50/50 with one variant on a large rollout is easiest to read.
3. In code, use `GetConfigForPlayerAsync(player)` and `GetValue` where the feature is used.
4. Read results on the Experiments page. It tracks D1 and D7 retention, playtime, ARPU, ARPPU, payer conversion and session time automatically.

Rules:

- Enrollment happens on the first `GetValue` of that key on a player snapshot, and is sticky for the experiment. Call it only where the player meets the feature, otherwise you enroll players who never saw it and dilute results.
- `GetConfigAsync()` never applies experiments.
- Only one active experiment per config key. The key and its conditions are locked while it runs.
- Targeting is re-evaluated each session. A player who stops matching falls back to normal config rules, but their data stays attributed to the variant.
- To measure a custom outcome, log it with analytics and put the variant in a custom field, for example `CustomField01 = tostring(variantValue)`. This is my suggested pattern; the docs describe custom fields and experiments separately.

```lua
--!strict
local ConfigService = game:GetService("ConfigService")
local AnalyticsService = game:GetService("AnalyticsService")

local function openShop(player: Player)
	local snap = ConfigService:GetConfigForPlayerAsync(player)
	local layout = snap:GetValue("shopLayout")
	local variant = if typeof(layout) == "string" then layout else "classic"
	AnalyticsService:LogCustomEvent(player, "ShopOpened", 1, {
		[Enum.AnalyticsCustomFieldKeys.CustomField01.Name] = variant,
	})
	-- build the shop UI from `variant`
end
```

## What not to do

- Don't log analytics from clients, or expect events from Studio.
- Don't log on attempt; log on success.
- Don't encode ids, names or timestamps in currencies, SKUs, event names or custom fields.
- Don't create one currency or event name per variant of something; use custom fields.
- Don't call `GetValue` for an experiment key early "just to cache it".
- Don't use `GetConfigAsync()` for per-player or experiment values.
- Don't apply a refreshed config mid-round if it changes balance; apply at a boundary.
- Don't trust a config value blindly: type-check it and keep a default. A bad publish can be restored from the History page (stage the old value, then publish).
