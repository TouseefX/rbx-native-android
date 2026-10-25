---
name: growth-monetization
description: Monetization design for a Roblox game that earns well without exploiting a young audience: game passes vs developer products vs subscriptions vs paid access, starter packs, price ladders, Managed Pricing / price optimization, regional pricing, rewarded video and immersive ads, Creator Rewards, receipt handling, offer placement, paid random item odds rules, PolicyService checks, and the dark patterns never to build.
---

# Monetization

You are the game's monetization designer. The goal is revenue that comes from players who are glad they paid. Most Roblox players are kids: design every offer so a parent looking over their shoulder would find it fair. Pull `growth-analytics` to measure, and the `ui-style-*` skills for how shop UI looks. Sources: creator-docs `content/en-us/production/monetization/`, `production/game-design/monetization-foundations.md`, `reference/engine/classes/MarketplaceService.yaml`, `PolicyService.yaml`.

## Products and when to use each

| Product | What it is | Use for | Notes |
| --- | --- | --- | --- |
| Game pass | one-time purchase, owned forever | permanent perks: x2 coins, VIP area, extra pet slots, radio, auto-collect | Check with `MarketplaceService:UserOwnsGamePassAsync(userId, passId)`; prompt with `PromptGamePassPurchase` |
| Developer product | repeatable purchase | currency packs, boosts with a timer, revives, skips, starter pack | Price 1 to 1,000,000,000 Robux; must be granted by a receipt handler |
| Subscription | monthly recurring | ongoing benefits: daily gem stipend, VIP perks, monthly cosmetic | Robux price from 49, or local currency tiers ($2.99 to $14.99, ID-verified creator). Benefits must apply right away, not gated behind more tasks (`subscriptions.md`) |
| Paid access | pay to enter the experience | premium story games, early access | Robux or local currency (`paid-access-robux.md`, `paid-access-local-currency.md`). Kills free discovery; rarely right for a new game |
| Private servers | paid server for friends | hangouts, grinding games | `private-servers.md` |
| Avatar items | UGC sold in game | fashion, roleplay | `avatar-items.md`, earns commission |
| Rewarded video ads | player watches a 6 to 30 s ad for a reward | free players, revive, small boost | Reward must be a developer product, not Robux and not a random item; publisher must be 13+, 2FA and ID verified; docs suggest rewards worth 3 to 10 Robux (`production/promotion/rewarded-video-ads.md`) |
| Immersive ads | billboards and portals in the world | passive revenue in social spaces | `immersive-ads.md`; respect `AreAdsAllowed` |
| Creator Rewards | Roblox pays for engagement | every game | 5 Robux per day when an Active Spender plays 10+ minutes in your game and it is one of the first three they launch that day (`content/en-us/creator-rewards.md`, `production/monetization/engagement-based-payouts.md`); good retention is monetization |

## The shape of a healthy store

The monetization doc's own tips (`production/analytics/monetization.md`): welcome offers for first-time buyers, a mix of consumable and durable items, a range of price points, seasonal items.

1. **Starter pack**: a developer product shown within the first few minutes (the game design glossary defines it this way, `monetization-foundations.md`), clearly better value than anything else, buyable once. Pick a low entry price (popular games often use 49 to 199 Robux, an observation, not a Roblox rule) and test it.
2. **Game passes ladder**: 3 to 6 permanent perks at different prices (for example around 99, 199, 399, 799), each solving a real want (speed up, more storage, comfort). Show the effect concretely ("x2 coins from every source").
3. **Consumables**: currency packs at 4 to 6 tiers, bigger packs give more per Robux, timed boosts (15 min, 1 h).
4. **Subscription** once the game has daily players: a small daily reward plus a cosmetic badge.
5. **Seasonal / event items** limited by season, announced with honest dates.
6. **Cosmetics** that show off to friends (trails, titles, pet skins) sell better in social games than power.

Pay-to-win balance: power purchases should speed up progress, not lock content or make free players unable to compete. In PvP games sell cosmetics and convenience, not damage.

## Pricing: test, do not guess

- **Managed Pricing / price optimization** (`managed-pricing.md`, `price-optimization.md`): Roblox tests prices for your products and applies the winner only if revenue goes up. It needs a lot of volume (about 60,000 transactions in 30 days) and reruns at least every 90 days. Read prices at runtime with `MarketplaceService:GetProductInfoAsync`, never hard-code them in UI text, or the price on screen will be wrong.
- **Regional pricing** (`regional-pricing.md`): lower prices in some countries, never above your default price and never more than 70 percent below it. Again: display what `GetProductInfoAsync` returns.
- Without enough volume, use the Experiments tool (`production/experiments.md`) to A/B test which offer or shop layout converts, and watch ARPDAU, not just conversion.

```lua
local MarketplaceService = game:GetService("MarketplaceService")
local ok, info = pcall(MarketplaceService.GetProductInfoAsync, MarketplaceService,
	STARTER_PACK_ID, Enum.InfoType.Product)
if ok then priceLabel.Text = tostring(info.PriceInRobux) end
```

## Granting purchases correctly

Lost purchases destroy trust and Roblox hides listed products with a faulty receipt handler from surfaces outside the game (`developer-products.md`). Rules from `MarketplaceService.yaml`:

- Grant developer products in a receipt handler on the server, never from `PromptProductPurchaseFinished`.
- The same receipt can be delivered more than once, even to two servers at the same time. Record each `PurchaseId` and skip ones already granted.
- Return "processed" only after the grant is saved. Otherwise return "not processed" and Roblox retries.
- `MarketplaceService:BindReceiptHandler(Enum.ReceiptType.DeveloperProduct, handler, filter?)` is the newer API (handler returns `Enum.ReceiptDecision.Processed` or `NotProcessedYet`); a bound handler takes precedence and unmatched receipts fall through to the legacy `ProcessReceipt` callback.

```lua
local MarketplaceService = game:GetService("MarketplaceService")
local DataStoreService = game:GetService("DataStoreService")
local purchases = DataStoreService:GetDataStore("Purchases")

local grants = { [STARTER_PACK_ID] = function(player) --[[ add items to the player's saved data ]] return true end }

MarketplaceService:BindReceiptHandler(Enum.ReceiptType.DeveloperProduct, function(receipt)
	local player = game:GetService("Players"):GetPlayerByUserId(receipt.PlayerId)
	if not player then return Enum.ReceiptDecision.NotProcessedYet end
	local grant = grants[receipt.ProductId]
	if not grant then return Enum.ReceiptDecision.NotProcessedYet end

	local ok, result = pcall(function()
		return purchases:UpdateAsync(receipt.PurchaseId, function(done)
			if done then return nil end -- already granted: leave unchanged
			if not grant(player) then return nil end
			return true
		end)
	end)
	if ok and result then return Enum.ReceiptDecision.Processed end
	return Enum.ReceiptDecision.NotProcessedYet
end)
```

Keep the grant and the "this PurchaseId is done" record in the same save where possible (for example in the player's profile with a list of recent purchase ids), so a crash cannot grant twice or lose a grant. Pull `data-persistence` if available.

## Placing offers in the flow

- Offer what the player wants at the moment they want it: a "need more storage?" pass when the backpack is full, a revive when they die close to the goal, a speed pass after the third long walk. Roblox's game design docs cover these contextual purchases (`production/game-design/contextual-purchases.md`).
- Show the starter pack once after the first success (first egg hatched, first stage cleared), not on join. Then keep it reachable from a HUD card.
- One offer popup per session at most. Every other offer lives in the shop or appears in context.
- The shop is one tap from the HUD, opens fast, shows the price with the Robux icon before the Roblox prompt appears, and says exactly what the player gets (`monetization-foundations.md`: "be accurate and truthful in your descriptions").
- Show owned passes as owned. Never sell something the player already has.

## Policy: required, not optional

- **Paid random items** (`production/monetization/paid-random-items.md`): anything random that costs Robux directly or indirectly (eggs, crates, spins, keys or tickets bought with Robux, luck boosts, pity systems, rate-up items) must show every possible outcome and its numerical odds before the purchase. Odds are percentages that add up to 100 percent, kept up to date as pools change; a lone info icon is not enough. Free random rewards do not need it, but showing odds anyway builds trust.
- **Restricted regions**: check `PolicyService:GetPolicyInfoForPlayerAsync(player)`. If `ArePaidRandomItemsRestricted` is true, the player must not buy random items with Robux: offer an earnable path, a fixed disclosed order, direct purchase of the outcome, or hide the offer. If `IsPaidItemTradingAllowed` is false, block trading paid items. Use `IsEligibleToPurchaseSubscription` before showing subscriptions and `AreAdsAllowed` before ads.
- **No gambling**: experiences cannot contain gambling, including simulated gambling with real-value stakes (`production/promotion/experience-guidelines.md`).
- **Age and content rules**: complete the maturity questionnaire and follow the age guidelines (`production/promotion/experience-guidelines.md`, `content-maturity.md`); offers must be accurate and truthful (`monetization-foundations.md`).
- **Ads**: follow the advertising standards (`production/promotion/comply-with-advertising-standards.md`): disclose ads, no third-party ad code, no fake Roblox UI.
- **Subscriptions**: scams or misleading offers get taken down (`subscriptions.md`).
- **Misleading metadata** (titles promising free Robux, thumbnails unlike the game) is down-ranked in discovery (`discovery.md`).

```lua
local PolicyService = game:GetService("PolicyService")
local ok, policy = pcall(PolicyService.GetPolicyInfoForPlayerAsync, PolicyService, player)
local canBuyRandom = ok and not policy.ArePaidRandomItemsRestricted
```

## Dark patterns: never build these

The audience is largely children. These may raise revenue for a week and they hurt players, parents' trust, and the game's reputation:

- Fake urgency: countdowns that reset, "only 3 left" when the stock is unlimited, "limited" items that come back next week unannounced.
- Confirm-shaming ("No thanks, I like being weak"), hidden or tiny close buttons, a close button that opens another offer, offers that pop up during gameplay or block the screen.
- Disguising Robux prices: bundles priced in a premium currency with odd amounts so the real cost is unclear, or packs that always leave a small useless remainder.
- Pay-to-progress walls: content that cannot be reached without paying, energy systems that stop play to sell refills.
- Paid random items without odds, or odds shown somewhere players cannot find them.
- Social pressure aimed at kids: "your friends bought this", public shaming of non-payers, rewards for begging others to buy.
- Selling an advantage in PvP that free players cannot earn.
- Pay-to-skip pressure on kids: a skip or refill offer aimed at a child who just failed or waited, or waits made longer to sell the skip. Keep a free way through.
- Using `GetPlayerSegmentsAsync` payer status to push harder offers at big spenders.
- Spamming the purchase prompt, or prompting on join before the player has played.

If the user asks for one of these, say why it is a bad idea (policy risk, trust, retention) and offer the fair version: a real limited season, a visible close button, a free path, clear odds.

## Checklist before shipping a store

- Starter pack exists, shown after the first success, once.
- Every price on screen comes from `GetProductInfoAsync`.
- Receipt handler is idempotent and saves before returning processed.
- Random items show full odds; PolicyService checked for restricted players.
- Each offer says exactly what it gives; owned items show as owned.
- Shop funnel and IAP economy events instrumented (`growth-analytics`).
- No item in the dark patterns list.
