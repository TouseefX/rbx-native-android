---
name: growth-retention
description: Retention and live-ops design for Roblox games: the first 60 seconds and first session, daily rewards and streaks, quests, events and update cadence, social hooks (friend boost, invites, co-play), Experience Notifications, generating update ideas from data and the genre's top games, and a content roadmap.
---

# Retention and live ops

You are the game's live-ops designer. Players who come back are what Roblox's recommendations reward (`discovery.md` ranks play days per user, playtime per user and co-play days among its key signals) and what Creator Rewards pay for. Pull `growth-analytics` to find the leak first, `growth-monetization` for offers, `growth-promotion` for getting new players.

## The first 60 seconds

D1 is driven by the core loop, the first-time experience and performance; the docs say get players to the fun within 5 minutes and avoid long tutorials (`production/analytics/retention.md`, `engagement.md`, `production/game-design/onboarding.md`). Aim for 60 seconds.

- **0 to 10 s**: the player spawns facing the action, not a wall or a menu. No intro cutscene, no settings popup, no shop popup. Load the spawn area first (streaming) so phones get in fast.
- **10 to 30 s**: one obvious thing to do, shown not told: a glowing object, an arrow, a beam to the first target, a single short hint line. Teach by doing.
- **30 to 60 s**: the first reward. Currency pops, a sound, a number going up, a new item. Then immediately show the next goal ("Hatch your first egg: 0/100 coins") as a progress bar.
- **First 5 minutes**: the first "wow" (first rare pet, first upgrade that visibly changes play, first win), the first meaningful choice, and a goal that takes longer than this session so there is a reason to return.
- Log each of these as onboarding funnel steps (`growth-analytics`) and watch where players drop.

Techniques from `production/game-design/onboarding-techniques.md`: contextual tips that appear only when needed, guided first objective, progressive unlock of UI (do not show 12 HUD buttons on join; reveal them as features unlock).

## Reasons to come back

| Hook | How | Watch out |
| --- | --- | --- |
| Daily reward | Claim once per UTC day, rewards grow over a 7-day cycle with a big day 7 | Missing a day should reset gently (step back a day or two, not to zero), or kids feel punished |
| Streak | Count consecutive days, show it on the HUD | Offer a free "streak freeze" earned in play, not only sold |
| Quests | 3 daily quests (5 to 10 minutes each), 3 to 5 weekly quests, tied to the core loop | Quests that force grinding or spending feel like chores |
| Timers | Things that finish while away (crops growing, eggs incubating, offline earnings capped at a few hours) | Do not make timers so long that play stops |
| Collection | Index of pets/items with completion rewards | Show what is missing, with silhouettes |
| Progression | Rebirth/prestige, levels, ranks with visible next milestone | Rebirth should feel like a gain, show before/after |
| Social | Friends online, play together bonus, co-op goals | Never pressure kids to recruit or pay |
| Events | Limited-time content with real dates | Do not reuse "limited" for things that come back unannounced |

Daily reward server logic (store the last claim day and streak in the player's saved data):

```lua
local function today() return math.floor(os.time() / 86400) end -- UTC day number

local function tryClaim(data)
	local day = today()
	if data.lastClaimDay == day then return nil end -- already claimed
	if data.lastClaimDay == day - 1 then
		data.streak += 1
	else
		data.streak = math.max(1, (data.streak or 0) - 2) -- gentle reset
	end
	data.lastClaimDay = day
	local cycleDay = ((data.streak - 1) % 7) + 1
	return REWARDS[cycleDay] -- table of 7 rewards, day 7 the best
end
```

Log the grant as an economy `Source` with `transactionType = TimedReward` and the streak as a custom event value.

## Social hooks

- **Friend boost**: a visible "+10% per friend in server" bonus (cap it, for example +50%). It drives co-play, which `discovery.md` counts as a signal (intentional co-play days per user).
- **Invites**: use the platform prompt, from a clear "Invite" button, never forced:

```lua
-- LocalScript
local SocialService = game:GetService("SocialService")
local player = game:GetService("Players").LocalPlayer
inviteButton.Activated:Connect(function()
	local ok, canInvite = pcall(SocialService.CanSendGameInviteAsync, SocialService, player)
	if ok and canInvite then
		SocialService:PromptGameInvite(player)
	end
end)
```

  (`production/promotion/invite-prompts.md`; `PromptGameInvite` also takes an `ExperienceInviteOptions` to customise the invite.) Reward the inviter only when the friend actually plays, and with in-game items, never Robux.
- **Party and co-op**: shared goals (server-wide boss, community progress bar), trading where allowed (`IsPaidItemTradingAllowed`), private servers for groups.
- **Referral system** and share links: `production/promotion/referral-system.md`, `share-links.md`.

## Experience Notifications

Bring players back with a message about something that matters to them (`production/promotion/experience-notifications.md`, `reference/engine/classes/ExperienceNotificationService.yaml`):

- Players opt in through `ExperienceNotificationService:PromptOptIn()`, called from a LocalScript after `CanPromptOptInAsync()` returns true (wrap both in `pcall`). Ask at a moment that makes sense ("Want a ping when your eggs hatch?"), never on join.
- Notifications are sent from the server side through Open Cloud's user notification API, using a notification string you create in the Creator Dashboard. One notification per user per day per experience; recipients must be 13+ and opted in; the experience needs at least 100 visits.
- Content must be personalised and actionable ("Your crops are ready to harvest!", "You're 2 races from finishing the weekly challenge"), not generic ads ("New cars dropped, check them out!") (`includes/experience-notifications/guidelines.md`).

```lua
-- LocalScript, after the player's first harvest
local ENS = game:GetService("ExperienceNotificationService")
local ok, canPrompt = pcall(ENS.CanPromptOptInAsync, ENS)
if ok and canPrompt then
	ENS:PromptOptIn()
end
```

## Events and update cadence

From `production/game-design/liveops-essentials.md` and `liveops-planning.md`: updates on a steady rhythm (weekly to monthly), limited-time and seasonal events, and larger updates every few months. A workable rhythm for a small team:

- **Weekly**: a small update (new item, new quest set, balance fixes) on the same weekday, announced in game.
- **Every 2 to 4 weeks**: an event (limited currency, event shop, event map or boss, leaderboard), 1 to 2 weeks long. Use Experience Events (`production/promotion/experience-events.md`) so it shows on the game page with its own thumbnail and players can opt in to a reminder.
- **Every 2 to 3 months**: a major update (new world, new system), ideally lined up with a season and with a battle pass or season pass (`production/game-design/season-pass-design.md`).
- Roblox's discovery docs note updates can bring a wave of new users from recommendations ("explore" phase), so ship polished updates, and fix the first session before big pushes.

In-game "What's new" board or popup on first join after an update: 3 bullet lines max with images, and a button that takes the player to the new thing.

## Generating update ideas

Base ideas on data, then on the genre:

1. From `growth-analytics`: where do players stop? Content runs out at level 30 → add the next world. Currency piles up → add a sink worth saving for. Session length short → add a mid-session goal.
2. From the genre's top games: list what the top 5 games in the genre update with (new worlds, pets, codes, events, rebirth tiers, trading, clans). Do not copy their content; take the type of feature and fit it to this game.
3. From players: group feedback, DevForum, comments; repeat requests are cheap wins.
4. Score each idea: impact on the target metric x confidence / effort. Pick the top 1 to 3 for the next update.

## Content roadmap template

Write it for the user as a table: week, update name, contents, metric it targets, event or not. Example for a pet simulator:

| Week | Update | Contents | Target |
| --- | --- | --- | --- |
| 1 | Fixes + quests | Daily/weekly quests, onboarding fixes from funnel data | D1 |
| 2 | World 2 | New area, 6 pets, new egg (with odds) | D7, session |
| 3 | Friends week | Friend boost, co-op egg, invite button | Co-play |
| 4 | Event: Lava Rush | Event currency, event shop, leaderboard, 10 days | D7, revenue |
| 6 | Rebirth 2.0 | Rebirth tiers with before/after UI | D30 |

## Don'ts

- No dark patterns to force returns: punishing streak resets, FOMO countdowns that lie, energy walls (see `growth-monetization`).
- No notification spam or generic ads in notifications.
- No events without the content to back them; a reskinned shop is not an event.
- Do not launch a big update and an ad campaign on top of a broken first session.
