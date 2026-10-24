---
name: monetization
description: Developer products, game passes and subscriptions with MarketplaceService: correct ProcessReceipt, pass ownership checks, showing prices, and the paid-random-items (loot box) policy. Pull it before writing any code that sells something or grants a purchase.
---

# Monetization

## Which product type

| Type | Use for | Server check |
| --- | --- | --- |
| Developer product | Repeatable purchases: currency, ammo, consumables | `ProcessReceipt` (or `BindReceiptHandler`) |
| Pass | One-time permanent access or power-up | `UserOwnsGamePassAsync` |
| Subscription | Recurring benefits for a monthly fee, benefits end when payment stops | `GetUserSubscriptionStatusAsync` |

(source: https://create.roblox.com/docs/en-us/production/monetization/developer-products , https://create.roblox.com/docs/en-us/production/monetization/passes , https://create.roblox.com/docs/en-us/production/monetization/subscriptions)

Facts worth knowing: developer product price is 1 Robux minimum, 1 billion maximum. Subscriptions are auto-renewing, priced in Robux (49 or more) or one of five local-currency tiers, and a game can have up to 50 of them. Deleting a local-currency subscription requires refunding subscribers; Robux subscriptions cannot be refunded, so take it off sale first.

## Prompting

```lua
--!strict
local MarketplaceService = game:GetService("MarketplaceService")
-- server or client; the player argument is the buyer
MarketplaceService:PromptProductPurchase(player, productId)
MarketplaceService:PromptGamePassPurchase(player, gamePassId)
MarketplaceService:PromptSubscriptionPurchase(player, subscriptionId) -- string id
```

(source: https://create.roblox.com/docs/reference/engine/classes/MarketplaceService)

Prompts are requests, not grants. The prompt closing, or `PromptProductPurchaseFinished` firing, proves nothing. Roblox says to use `ProcessReceipt` and not `PromptProductPurchaseFinished` for developer products (source: developer-products page above). Only prompt in response to a player action (button, vendor), never on join or on a loop. Do not prompt for something the player already owns.

## ProcessReceipt done right

Rules from the reference (source: https://create.roblox.com/docs/reference/engine/classes/MarketplaceService):

- Assign `MarketplaceService.ProcessReceipt` exactly once, in one server Script. It must handle every developer product you sell.
- It is called for unresolved purchases when a purchase completes and when the user joins a server. With no callback, receipts are auto-acknowledged and lost.
- Return `Enum.ProductPurchaseDecision.PurchaseGranted` only after the grant is recorded. Return `NotProcessedYet` if you could not grant (player left, data not loaded, save failed). A `NotProcessedYet` receipt is retried only on the same server when the user starts another purchase, or when they rejoin any server in the experience. There is no timer-based retry.
- The callback can yield as long as the server runs and the result is still accepted, but do not yield on anything that might never return. Wrap data store calls in `pcall` with a bounded retry.
- Order across pending purchases is not deterministic. The same receipt can run on two servers at once. A `PurchaseGranted` can still fail to record on Roblox's side, and then it comes back, so granting must be idempotent.
- Key idempotency on `receiptInfo.PurchaseId` (a string). `receiptInfo` has `PlayerId`, `ProductId`, `CurrencySpent`, `CurrencyType`, `PlaceIdWherePurchased`.

The simplest correct design: store processed `PurchaseId`s inside the player's own data record, so the grant and the "already processed" mark save in one write, under session locking (see the data-persistence skill). Roblox's player-data page describes exactly this (source: https://create.roblox.com/docs/en-us/cloud-services/data-stores/player-data-purchasing). A separate `PurchaseHistory` store works too (Roblox's reference sample does that, using `UpdateAsync`), but there is an unavoidable gap where the grant succeeded and recording failed.

```lua
--!strict
local MarketplaceService = game:GetService("MarketplaceService")
local Players = game:GetService("Players")

type Profile = { coins: number, receipts: { [string]: boolean } }
local profiles: { [Player]: Profile } = {}           -- filled by your session-locked loader
local grants: { [number]: (Player, Profile) -> boolean } = {
	[1234567] = function(_, p) p.coins += 100; return true end, -- must not yield
}

local function savePlayer(player: Player): boolean
	-- write profiles[player] through your data layer, pcall + retry, return success
	return true
end

MarketplaceService.ProcessReceipt = function(info)
	local player = Players:GetPlayerByUserId(info.PlayerId)
	if not player then return Enum.ProductPurchaseDecision.NotProcessedYet end

	local profile = profiles[player]
	if not profile then return Enum.ProductPurchaseDecision.NotProcessedYet end -- data not loaded

	if profile.receipts[info.PurchaseId] then
		return Enum.ProductPurchaseDecision.PurchaseGranted -- already granted and saved earlier
	end

	local grant = grants[info.ProductId]
	if not grant then return Enum.ProductPurchaseDecision.NotProcessedYet end -- unknown product, do not ack
	if not grant(player, profile) then return Enum.ProductPurchaseDecision.NotProcessedYet end

	profile.receipts[info.PurchaseId] = true
	if not savePlayer(player) then
		profile.receipts[info.PurchaseId] = nil -- roll back the in-memory grant too in real code
		return Enum.ProductPurchaseDecision.NotProcessedYet
	end
	return Enum.ProductPurchaseDecision.PurchaseGranted
end
```

Do not: return `PurchaseGranted` before the save, set the callback in several scripts (the last wins), grant from a remote the client fires, or return `PurchaseGranted` for products you do not recognise.

Newer alternative: `MarketplaceService:BindReceiptHandler(ReceiptType.DeveloperProduct, handler, filterIds?)` routes receipts by product id; handlers return `Enum.ReceiptDecision.Processed` or `NotProcessedYet`. A bound handler takes precedence and unmatched receipts fall through to `ProcessReceipt`, so you can adopt it incrementally (source: MarketplaceService reference above). The same idempotency rules apply.

Listed developer products are sold outside the game (Store tab, search, homepage). Roblox checks your `ProcessReceipt` implementation, and a product with a faulty one stops showing outside the game until fixed (source: developer-products page).

## Game passes

- `MarketplaceService:UserOwnsGamePassAsync(userId, gamePassId)` works on client and server. The pass id is not an asset id. Results are cached, and the cache updates when `PromptGamePassPurchaseFinished` fires. A pass bought outside the game while in the same session can take several minutes to show. On a fresh join after buying, it returns true (source: MarketplaceService reference above).
- Check on the server before granting anything with gameplay effect. Cache the result per player on join in a table; do not call it every frame.
- Grant on `PromptGamePassPurchaseFinished(player, passId, wasPurchased)` for immediate feedback, but do the real entitlement check with `UserOwnsGamePassAsync` on the server (source: passes page). Client-fired "I own it" signals prove nothing.
- Concurrent `UserOwnsGamePassAsync` calls via `task.spawn` are batched by the engine, so checking several passes at join in parallel is cheap.

```lua
--!strict
local MarketplaceService = game:GetService("MarketplaceService")
local owned: { [Player]: { [number]: boolean } } = {}

local function ownsPass(player: Player, passId: number): boolean
	local cache = owned[player]
	if not cache then cache = {}; owned[player] = cache end
	local hit = cache[passId]
	if hit ~= nil then return hit end
	local ok, res = pcall(MarketplaceService.UserOwnsGamePassAsync, MarketplaceService, player.UserId, passId)
	if not ok then return false end
	cache[passId] = res
	return res
end

MarketplaceService.PromptGamePassPurchaseFinished:Connect(function(player, passId, purchased)
	if purchased then
		(owned[player] or {})[passId] = true
		-- unlock perk; re-verified server side on next use
	end
end)
```

Clear the table on `PlayerRemoving`.

## Subscriptions

`GetUserSubscriptionStatusAsync(player, subscriptionId)` returns `IsSubscribed` (true only once payment succeeded) and `IsRenewing`. Benefits depend on the user continuing to pay, so re-check on join and at long intervals. Studio provides test subscription ids `EXP-0` (active, renewing), `EXP-1` (returning) and `EXP-2` (refunded) (source: MarketplaceService reference above). Use clear names and show price and duration when merchandising (source: subscriptions page).

## Showing price and name

Use `MarketplaceService:GetProductInfoAsync(id, Enum.InfoType.Product)` for products or `Enum.InfoType.GamePass` for passes, and read `PriceInRobux`, `Name`, `Description`. It throws if the item does not exist, so `pcall`. Do not hardcode prices in UI. `GetProductInfo` is deprecated in favour of `GetProductInfoAsync`. If you use price optimization, run the pricing script on the client so users see personalized prices; `receiptInfo.CurrencySpent` holds the real amount paid (source: passes page, developer-products page, MarketplaceService reference).

## Paid random items (loot boxes, gacha, spin wheels)

Anything random that costs Robux directly or via Robux-bought currency counts: egg hatching, prize wheels, chests, luck boosts, pity systems (source: https://create.roblox.com/docs/en-us/production/monetization/paid-random-items).

- Disclose all possible outcomes and real numerical odds before purchase, as percentages summing to exactly 100%. Odds must update when boosts are active and when only one copy of an outcome can be obtained.
- Applies to indirect purchases (keys, tickets, gems bought with Robux). Free earnable randomness does not need odds.
- Per player, call `PolicyService:GetPolicyInfoForPlayerAsync(player)` on the server. If `ArePaidRandomItemsRestricted` is true, the player must not use the generator. Apply one treatment: give a free earnable path, pre-determined disclosed order, sell outcomes directly, hide the feature, block with a message, or move them away. If `IsPaidItemTradingAllowed` is false, do not let them trade items obtained that way (source: https://create.roblox.com/docs/reference/engine/classes/PolicyService).
- Enforce on the server, not only in UI. Wrap in `pcall` and fail closed (treat errors as restricted).

```lua
--!strict
local PolicyService = game:GetService("PolicyService")
local function canUseLootBox(player: Player): boolean
	local ok, info = pcall(PolicyService.GetPolicyInfoForPlayerAsync, PolicyService, player)
	return ok and not info.ArePaidRandomItemsRestricted
end
```

## Honest prompts

Prompt only when the player asks for it, name what they get, and show the price from `GetProductInfoAsync`. Do not auto-open purchase prompts repeatedly or disguise a purchase button as something else. Grant exactly what was shown.

## Testing

Roblox's docs do not describe a dedicated test-purchase mode that I could confirm. Practical approach:

- Keep the grant logic in a pure function (`grantProduct(profile, productId)`) and the receipt handler thin, so you can unit test them with a fake receipt table `{ PurchaseId = "x", PlayerId = id, ProductId = n, CurrencySpent = 0 }` and call it twice to prove idempotency.
- Test failure paths: player nil, profile not loaded, save fails.
- Use test subscription ids above for subscription state.
- Test real prompts on a published test place with a throwaway account, and confirm in the Creator Hub that sales and receipts reconcile.
