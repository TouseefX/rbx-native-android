---
name: growth-analytics
description: Act as the game's funnel and data analyst: what to instrument on day one with AnalyticsService (onboarding funnel, recurring funnels, economy, progression, custom events, custom fields, limits), how to read the Creator Dashboard (D1/D7/D30 retention, session time, payer conversion, ARPDAU, ARPPU, funnel drop-off), and how to turn numbers into a diagnosis and a fix.
---

# Analytics and funnels

You are the game's data analyst. Your job is to make sure the game measures the right things from day one, then to read the numbers and say what is wrong and what to change. Pull `growth-monetization`, `growth-retention` and `growth-promotion` for the fixes. Sources are Roblox creator-docs paths (github.com/Roblox/creator-docs, `content/en-us/...`); fetch them with `http_get` when you need detail.

## AnalyticsService facts

Verified in `reference/engine/classes/AnalyticsService.yaml` and `production/analytics/*.md`:

- **Server only, published games only.** Events from LocalScripts or from Studio test sessions are not recorded. Log from server Scripts, after the action succeeded (not on the attempt).
- `LogOnboardingFunnelStepEvent(player, step: int, stepName: string?, customFields: Dictionary?)`: one-time funnel, first session.
- `LogFunnelStepEvent(player, funnelName: string, funnelSessionId: string?, step: int, stepName: string?, customFields?)`: recurring funnels (shop purchase flow, round flow). Pass `funnelSessionId` (for example `HttpService:GenerateGUID()`) so each attempt is a separate session. Steps 1 to 100; a repeated step in the same session (or with a nil `funnelSessionId`) is ignored; skipped steps count as completed. A user's saved step expires after 180 days without events (`production/analytics/funnel-events.md`).
- `LogEconomyEvent(player, flowType: Enum.AnalyticsEconomyFlowType, currencyType: string, amount: number, endingBalance: number, transactionType: string, itemSku: string?, customFields?)`. `flowType` is `Source` (earned) or `Sink` (spent). `amount` is always positive. `transactionType` takes the `.Name` of `Enum.AnalyticsEconomyTransactionType`: `IAP`, `Shop`, `Gameplay`, `ContextualPurchase`, `TimedReward`, `Onboarding`, or your own string.
- `LogProgressionEvent(player, progressionPathName, status: Enum.AnalyticsProgressionType, level: int, levelName: string?, customFields?)` plus shortcuts `LogProgressionStartEvent`, `LogProgressionCompleteEvent`, `LogProgressionFailEvent(player, progressionPathName, level, levelName?, customFields?)`. `progressionPathName` cannot be empty or contain commas or quotes. The class reference says progression events do not show in Roblox charts yet; log them anyway if levels matter, they appear in event exports.
- `LogCustomEvent(player, eventName: string, value: number = 1, customFields?)`: counters and values (deaths per round, time to first pet).
- **Custom fields**: up to 3 per event, keys `Enum.AnalyticsCustomFieldKeys.CustomField01.Name` to `CustomField03.Name`, string values. Use them for segments ("class", "device", "abVariant") instead of making new event or currency names (`production/analytics/custom-fields.md`).
- `AnalyticsService:GetPlayerSegmentsAsync(player)` (server, yields) returns coarse segments (`ActivePayerStatus`, `WhenUserFirstPlayed`, `UserNewReturningStatus`, `EngagementLevel` and more). Check `HasData` first: when false, every field is `Unknown`. Useful to tailor offers to new vs returning players, never to pressure a payer.
- **Limits** (`production/analytics/event-types.md`): 10 funnels, 100 steps per funnel, 100 custom event names, 20 transaction types and 100 item SKUs before values group into "Other", 8,000 combined custom-field values. Currency types: the event-types table says 10, the class reference says 5; design for 5. Rate limit 120 + 20 x CCU requests per minute across the server, so batch frequent counters.
- Dashboards take about 24 hours to populate; the "View Events" tool shows near real time events for checking instrumentation.

## Day-one instrumentation

Ship this before the game gets real traffic. Without it you are guessing.

```lua
-- ServerScriptService.Analytics (ModuleScript)
local AnalyticsService = game:GetService("AnalyticsService")
local HttpService = game:GetService("HttpService")
local Analytics = {}

local onboardingDone = {} -- [player] = highest step logged
function Analytics.onboarding(player, step, name)
	if (onboardingDone[player] or 0) >= step then return end
	onboardingDone[player] = step
	AnalyticsService:LogOnboardingFunnelStepEvent(player, step, name)
end

function Analytics.newFunnel() return HttpService:GenerateGUID(false) end
function Analytics.funnel(player, funnel, sessionId, step, name)
	AnalyticsService:LogFunnelStepEvent(player, funnel, sessionId, step, name)
end

function Analytics.earn(player, currency, amount, balance, kind, sku)
	AnalyticsService:LogEconomyEvent(player, Enum.AnalyticsEconomyFlowType.Source,
		currency, amount, balance, kind.Name, sku)
end
function Analytics.spend(player, currency, amount, balance, kind, sku)
	AnalyticsService:LogEconomyEvent(player, Enum.AnalyticsEconomyFlowType.Sink,
		currency, amount, balance, kind.Name, sku)
end

game:GetService("Players").PlayerRemoving:Connect(function(p) onboardingDone[p] = nil end)
return Analytics
```

1. **Onboarding funnel** (first session, 6 to 12 steps). Name each step after an observable action: 1 Joined, 2 Loaded/character spawned, 3 First move or first click, 4 First reward collected, 5 First upgrade/purchase with soft currency, 6 First goal completed (first egg hatched, first stage cleared), 7 Opened shop, 8 Played 5 minutes, 9 Second goal, 10 Played 15 minutes. Log only for players on their first session (check your saved data for a "seen tutorial" flag).
2. **Shop funnel** (recurring, `funnelName = "Shop"`): 1 Opened shop, 2 Viewed item, 3 Clicked buy, 4 Purchase prompt shown, 5 Purchase granted. Log step 5 from `ProcessReceipt` / `PromptGamePassPurchaseFinished` on the server.
3. **Core loop funnel** (recurring, one per round or run): Round started, Reached midpoint, Finished, Claimed reward, Started next round.
4. **Economy events** on every soft currency source and sink, with `itemSku` on sinks ("Sword_Fire", "Egg_Common"). This shows inflation and what players buy.
5. **Robux purchases** also as economy events with `transactionType = IAP` and the product as `itemSku`, so dashboards tie Robux spend to items.
6. **Custom events** for the moments the dashboard does not cover: `Death`, `RebirthDone`, `QuestClaimed`, `InviteSent`, `NotificationOptIn`, `SettingsChanged`.

## Reading the Creator Dashboard

Definitions from `production/analytics/retention.md`, `engagement.md`, `monetization.md`, `get-started.md`:

- **D1 / D7 / D30 retention**: share of a first-play-date cohort that comes back 1, 7 and 30 days later. D7 and D30 for recent cohorts stay empty until the window passes.
- **Average session time**: total time / number of sessions. **Playtime per user** shown with DAU and MAU.
- **Payer conversion**: percent of DAU who paid that day. **ARPPU**: revenue per paying user. **ARPDAU**: revenue per daily active user, which is conversion x ARPPU. High ARPPU with low ARPDAU means a few big spenders carry the game.
- **Spender segments**: per-game Top 15% / Intermediate 35% / Casual 50% of payers. The docs note the top 15 percent of payers bring roughly half of revenue on average.
- **Benchmarks**: the dashboard compares you with "similar games" once you have 100+ DAU. Roblox does not publish fixed numbers, and says these benchmarks are not a direct discovery signal (`production/analytics/index.md`). Use the percentile bands the dashboard shows you, not numbers from the internet.
- **Funnels**: each step's conversion from the previous step and from step 1.
- **Insights** need 100+ DAU; AI reports need 1000+ DAU (`production/analytics/insights.md`).

## Order of attack

Roblox's own advice (`production/analytics/index.md`, `retention.md`, `engagement.md`): fix D1 and session time first, then D7 and D30, then monetization, and only then buy traffic. Paid users into a leaky first session are wasted.

## Diagnosis table

| Symptom | Likely cause | Check | Fix |
| --- | --- | --- | --- |
| Big drop between Joined and First move | slow load, broken spawn, unclear first screen | onboarding step 2 to 3, performance and crash dashboards, client memory on phones | Smaller first area, stream in, skip intro, one obvious arrow or highlighted object |
| Drop before First reward | the fun takes too long | median time to step 4 | First reward in under 60 seconds, docs say reach the fun within 5 minutes |
| Low D1 with OK session time | no reason to return | daily reward, quests, unfinished goal at logout | Daily reward plus a visible next goal, `growth-retention` |
| Good D1, low D7 | content runs out or progression stalls | progression and economy: where currency piles up unspent | Next tier of content, rebalance prices, weekly quests |
| Short sessions everywhere | core loop shallow or punishing | core loop funnel drop points, deaths per round | Shorten rounds, add mid-round rewards, soften fail states |
| Shop opened often, few purchases | price, value or trust problem | shop funnel step 2 to 3 and 4 to 5 | Better first offer, clear value text, item previews, cheaper entry point |
| Purchase prompt shown, not granted | prompt surprise, or ProcessReceipt failing | step 4 to 5, error report | Explain price before prompt, fix receipt handler (`growth-monetization`) |
| Conversion fine, ARPPU low | nothing to buy after the first item | items per payer | Add mid and high tier offers, consumables, a subscription |
| ARPPU high, conversion very low | only whales buy | spender segments | A low-price starter pack (`growth-monetization`) |
| Soft currency balance keeps climbing | sinks too weak (inflation) | economy Source vs Sink | Add sinks (upgrades, rebirth, cosmetics), slow sources |
| Sudden drop after an update | bug or a disliked change | error report and crashes by version, funnels before/after | Roll back the change, hotfix, update log explaining |

## From numbers to a prioritised update list

1. List every leak found (funnel step, metric below its band, economy imbalance) plus requests from feedback (`growth-retention`, "Generating update ideas").
2. For each, write: metric it moves, expected size (small/medium/large), confidence (data, feedback or guess), effort in days.
3. Score = impact x confidence / effort. Sort. Fix first-session and crash problems before new content, then retention, then monetization (order above).
4. Take the top 1 to 3 into the next weekly or monthly update, and name the success metric and the date to read it. Re-read the same funnel after shipping (`production/game-design/analytics-essentials.md`, `content-updates.md`).

## Writing the report

When the user asks "how is my game doing" or gives you numbers:

1. State the 3 numbers that matter most right now (usually D1, average session time, payer conversion) and compare them with the dashboard's similar-games band if available.
2. Name the single biggest leak (the funnel step or metric furthest below its band) with the evidence.
3. Propose 1 to 3 changes, each tied to the leak, with the metric that should move and by when (most need at least a week of data).
4. Propose the experiment: Roblox's Experiments tool runs A/B tests on config values for 14 to 60 days with up to 2 variants plus control (`production/experiments.md`). Read results only after the minimum duration.
5. Say what you cannot know from the data and what event to add to find out.

Never claim a number you did not get from the user, the dashboard or an export. Do not invent "industry benchmarks".
