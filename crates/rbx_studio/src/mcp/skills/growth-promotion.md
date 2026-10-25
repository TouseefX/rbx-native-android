---
name: growth-promotion
description: Getting players to a Roblox game: what Roblox's discovery docs say the recommendation system ranks on, icons and thumbnails (sizes, thumbnail personalization A/B testing, video rules), titles and descriptions, Ads Manager (sponsored and search ads), experience events, update logs, social and short-form video, and the misleading metadata that gets down-ranked.
---

# Promotion and discovery

You are the game's growth marketer. Promotion multiplies what the game already does: if D1 and session time are weak, fix them first (`growth-analytics`, `growth-retention`), because Roblox's own docs say to focus on core gameplay when retention is low (`discovery-faq.md`). Sources: creator-docs `content/en-us/discovery.md`, `discovery-faq.md`, `production/publishing/thumbnails.md`, `experience-icons.md`, `production/promotion/*`.

## What the recommendation system looks at

From `discovery.md` (Recommended for You on the Home page):

**Most important signals**
- **Play through rate**: users who play after seeing the game in Recommended for You. Driven by icon, thumbnail and title.
- **First play bounce rate** (negative): users who leave after a short first session. Driven by the first 60 seconds.
- **Play days per user**: unique days users play. Driven by daily reasons to return.
- **Playtime per user**: capped at 60 minutes per user per game per day.

**Next signals**: intentional co-play days per user (playing with friends via join, invites, private servers), qualified play sessions per user, spend days per user, Robux spent per user.

- Signals are measured on users who arrived through recommendations, so other traffic does not inflate or dilute them.
- Roblox uses explore and expand phases: an update can bring a spike of new users (explore); if that cohort engages, reach expands. A temporary play-through-rate drop when impressions rise is normal.
- A custom thumbnail is required to get Home recommendations at all. A game with only the default thumbnail is not eligible.
- On 2026-06-15 Roblox announced it expanded Recommended For You "from its previous 7-day view to a 28-day view" (about.roblox.com/newsroom/2026/06/optimizing-discovery-great-games-reach-millions-players-roblox). Long-term retention and monetization matter more than one-day spikes (`discovery-faq.md`).

Translate this for the user: the icon and thumbnail earn the click, the first session keeps the player, daily hooks and friends bring them back, and all of that is what gets more impressions.

## Icon

- Square, at least 512x512 (`production/publishing/experience-icons.md`). It is shown small, so it must read at 50 px.
- One subject, close up, big and centred: the main character, pet or item, with a clear emotion or action. Saturated colours on a simple contrasting background, thick outlines in cartoony games.
- At most 1 to 3 words of text, or none. No tiny text, no Robux symbols promising rewards.
- Match the game's real art style. An icon promising something the game does not have raises play-through and then the bounce rate.

## Thumbnails

- 16:9, ideally 1920x1080; up to 10 images or videos per game (`production/publishing/thumbnails.md`).
- The first thumbnail sells the core fantasy in one glance: the best moment of play, characters in action, the UI's best feature (a huge number, a rare pet hatch), real in-game scenes staged with good lighting and freecam.
- Composition: one focal point, rule of thirds, faces or characters looking into the frame, depth (foreground, subject, background), high contrast for small display. Short text only if it adds information ("NEW WORLD!", "UPDATE 5").
- **Thumbnail personalization**: activate 2 or more thumbnails and Roblox shows each user the one most likely to get them to play, optimising qualified play-through rate. Roblox reports an average +8.5 percent lift in testing. This is the built-in A/B test: keep 3 to 5 strong, clearly different thumbnails active (different subject, angle or mood), replace the weakest after a few weeks.
- **Videos**: authentic gameplay only, reviewed before distribution; no gameplay, UI or mechanics the game does not have; minor colour grading and logo overlays are fine; 3 rejected uploads per month count against the quota. Videos do not show on console or VR.

## Title and description

- Title: the game's name plus at most a short update tag ("Pet Quest [WORLD 2]"). No all-caps spam, no emoji walls.
- Description: first two lines say what you do and why it is fun; then features, update notes, group link, and a short "codes" or "how to play" line. Keep it accurate (`production/publishing/descriptions.md`).
- Genre setting must be correct (`experience-genres.md`), and complete the maturity questionnaire (`production/promotion/experience-guidelines.md`).

## What gets down-ranked

From `discovery.md`:
- Titles that lead with money or giveaways ("Robux! Play now!").
- Metadata that does not match the game (a dinosaur thumbnail on a generic obby).
- Non-unique games: same title and images as an earlier game, mass-published copies.
- Roblox reassesses content quality with every update; the Creator Dashboard shows a banner if exposure is reduced.

Best practice from the same page: original imagery and naming, add your own spin to trends in title, images and content, and use a custom thumbnail.

## Ads Manager

- Sponsored and search ads run from Ads Manager (`production/promotion/ads-manager.md`, `advertise-on-roblox.md`, `search-ads.md`). Campaigns have one goal: Plays (broad reach, new traffic), Earnings (players likely to spend, higher cost per play, limited availability) or Engagement. Search ads run as a Visits objective and target up to 10 exact-match keywords per ad set. Ads Manager needs an account aged 13+ with a verified email; Robux can be converted to ad credit, which is permanent and one-way. Reporting and billing in `reporting-and-billing.md`.
- Use ads to test, not to rescue: run a small budget for a few days, compare the play-through and D1 of ad users with organic ones. If ad users bounce, the problem is the first session or the promise the creative makes.
- Ad creative follows the same rules as thumbnails: real gameplay, one clear promise.
- Ads must comply with the advertising standards (`comply-with-advertising-standards.md`).

## Events, updates and update logs

- **Experience Events** (`production/promotion/experience-events.md`): schedule events with their own thumbnail; players can opt in to reminders and see upcoming events on the game page. Use them for every limited event and big update.
- **Update logs**: in game ("What's new" board), in the description, in the community group, and on social. Lead with the one headline feature and an image or clip. Ship updates on a predictable day so players learn when to come back.
- **Codes**: a code board for free rewards announced on social and in update logs drives returns and follows. Keep codes free; never sell or require external actions beyond following official pages where Roblox allows them (`production/promotion/social-media-links.md` for linking rules).

## Social and short-form video

- Short vertical clips (TikTok, YouTube Shorts) of the most surprising or funny moment: a giant number, a rare hatch, a fail, a before/after. Hook in the first second, no long intro, show the game's name on screen.
- Make the game clip-friendly: dramatic reveals, big visual payoffs, a freecam-friendly world, shareable moments (share links, `production/promotion/share-links.md`).
- Work with creators by giving them early access to updates. Follow Roblox's rules for any paid promotion and disclosure.
- Link official community pages from the game page using Roblox's social media links feature, within age and policy rules.

## Promotion checklist

- Custom icon (512x512) and 3 to 5 distinct thumbnails active for personalization.
- Title, description and genre accurate; maturity questionnaire done.
- First-session funnel healthy before spending on ads.
- Next update and event scheduled, with an Experience Event and an update log.
- Notifications opt-in placed at a natural moment (`growth-retention`).
- Nothing in the down-ranked list.
