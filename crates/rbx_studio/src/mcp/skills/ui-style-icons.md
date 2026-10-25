---
name: ui-style-icons
description: Drawing production-quality SVG icons and item art for Roblox UI that match the chosen style (real silhouettes with bezier curves, outline weight, two-tone cel shading, highlight, rounded vs sharp joins, one grid), with worked cartoony egg, military rifle and sleek duotone examples as a quality bar, pattern tiles and 9-slice frames, then rasterizing to PNG, checking at 32 px and uploading through store_image / upload_image.
---

# Icons and item art that match the style

Text-only buttons and emoji icons are a tell of generic UI. Icons built out of Frames are a worse tell: a rifle made of stacked rectangles or an egg that is a plain oval reads as unfinished at a glance. Every HUD button, currency, tab, shop item and preview needs real art, all drawn the same way. Draw it as SVG yourself, rasterize it, show it in an `ImageLabel`. Pull `ui-style` first so you know the style and the corner decision.

## Hard rules

1. **Never build an icon or item art from Frames.** No UICorner blobs, no rotated bars, no stacked rectangles. Always SVG, rasterized to PNG.
2. **Silhouette first, with real proportions.** Sketch the object's outline as one path with bezier curves (`C`), then add parts. Look up what the object actually looks like and get the proportions right (see the construction notes).
3. **Two-tone cel shading, a highlight and an outline** on every icon of a filled style. Base fill, one shadow shape that follows the form, one hard highlight, the style's outline. No noisy gradients.
4. **Readable at 32 px.** Render each icon at 32 px and look at it. If you cannot tell what it is, simplify the shape and thicken the parts.
5. **One set, one hand.** Same grid, outline weight, light direction, joins and palette on every icon. Joins follow the UI's corner decision: round for rounded UI, miter for sharp UI.
6. **Item art is bigger than an icon and richer.** A shop tile or detail pane shows the item as an illustration (128 to 256 px on screen), with more shading steps and a ground shadow, never a small icon floating in a big box.

## Construction notes

- **Egg**: about 1.3 times taller than wide, widest point 60 to 65 percent of the way down, a narrower rounder dome on top. Never an ellipse. A slight tilt (5 to 10 degrees) adds charm. Spots are irregular blobs clipped to the shell, not perfect circles.
- **Assault rifle, side view, muzzle right**: overall about 3.5 to 1. Stock about 22 percent of the length, tapering toward the receiver with a darker butt pad. Receiver about 25 percent, the thickest part. Pistol grip raked back 15 to 20 degrees. Magazine in front of the grip, curving forward at the bottom. A rail on top with an optic. Handguard about 25 percent, a bit thinner than the receiver, with vent slots. A thin barrel with a gas block, then a muzzle device. Different weapons need different silhouettes (an SMG is short with a straight mag, a shotgun has a pump and a tube, a sniper a long barrel and a big scope), not the same shape recoloured.
- **Coin**: a rim ring at about 80 percent radius, the game's emblem in the middle, the coin's thickness shown as a darker band below.
- **Gem**: a wide table, faceted crown and pavilion (5 to 8 facets), each facet a flat tone.
- **Paw**: one big rounded-triangle pad and four oval toes in an arc, toes about 40 percent of the pad.
- **Line icons**: one stroke width, round caps and joins, corners from one radius, no tiny gaps.

## Layer order (filled styles)

1. Soft ground shadow (an ellipse under the object, black at 0.15 to 0.2).
2. Outline: the silhouette path drawn with a thick stroke behind the fill. Define the path once in `<defs>` and `<use>` it twice, stroke first, fill second, so the outline only grows outward.
3. Base fill in the shadow tone.
4. The lit area: a copy of the silhouette, shifted up-left and clipped to the shape, in the base tone. What is left showing along the bottom-right is the cel shadow, a crescent that follows the form.
5. Details (spots, panels, emblem), each with the same light logic.
6. Rim light: a thin lighter line on the shadow side, clipped.
7. Highlight: one hard white pill top-left at 0.8 to 0.9 opacity, plus one small dot.

Outline colour is the darkest shade of the icon's own hue (deep green on a green egg), not pure black, for cartoony and pastel styles. Military uses one dark neutral on everything.

## Quality bar: cartoony egg (128 grid)

Study the construction, then draw your own egg for your game. Do not reuse these numbers.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <defs>
    <path id="egg" d="M64 8 C88 8 106 50 106 80 C106 106 88 120 64 120 C40 120 22 106 22 80 C22 50 40 8 64 8 Z"/>
    <clipPath id="in"><use href="#egg"/></clipPath>
  </defs>
  <ellipse cx="64" cy="121" rx="34" ry="5" fill="#000" opacity="0.18"/>
  <use href="#egg" fill="none" stroke="#2F5E2A" stroke-width="10" stroke-linejoin="round"/>
  <use href="#egg" fill="#5DB043"/>
  <g clip-path="url(#in)">
    <path d="M58 2 C80 4 96 48 95 76 C94 100 80 112 56 113 C38 112 20 100 18 80 C16 52 36 0 58 2 Z" fill="#9BE15D"/>
    <path d="M38 60 C46 54 58 58 56 68 C54 78 40 78 36 72 C33 68 34 63 38 60 Z" fill="#FFF3B0"/>
    <path d="M70 34 C77 31 84 36 82 43 C80 49 71 49 68 45 C65 41 66 36 70 34 Z" fill="#FFF3B0"/>
    <path d="M72 82 C82 78 94 84 92 95 C90 105 76 106 70 99 C66 94 66 86 72 82 Z" fill="#E3D27A"/>
    <path d="M103 64 C108 84 104 108 80 117" fill="none" stroke="#C8F59A" stroke-width="3" stroke-linecap="round" opacity="0.7"/>
  </g>
  <ellipse cx="47" cy="38" rx="7" ry="15" fill="#FFFFFF" opacity="0.85" transform="rotate(24 47 38)"/>
  <circle cx="56" cy="22" r="3.5" fill="#FFFFFF" opacity="0.85"/>
</svg>
```

What makes it work: the egg shape (narrow dome, low belly), the lit copy shifted up-left so the shadow is a crescent, the spot in the shadow area drawn in a darker tone, a rim light on the shadow side, a hard highlight, an outline in the egg's own darkest green, a ground shadow.

## Quality bar: military rifle (256 x 96 grid)

Sharp joins, a light metal tone with a hard three-step gradient (lit top edge, body, shaded underside), darker polymer parts, a dark hairline, a few panel lines, one small accent.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 96">
  <defs>
    <linearGradient id="m" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#EEF0E9"/><stop offset="0.24" stop-color="#EEF0E9"/>
      <stop offset="0.24" stop-color="#C9CCC2"/><stop offset="0.72" stop-color="#C9CCC2"/>
      <stop offset="0.72" stop-color="#9A9E94"/><stop offset="1" stop-color="#9A9E94"/>
    </linearGradient>
    <linearGradient id="d" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#8C9086"/><stop offset="0.3" stop-color="#8C9086"/>
      <stop offset="0.3" stop-color="#6A6E65"/><stop offset="1" stop-color="#6A6E65"/>
    </linearGradient>
  </defs>
  <g stroke="#1E211C" stroke-width="2.5" stroke-linejoin="miter">
    <path d="M110 54 L126 54 C127 66 131 76 138 85 L123 90 C116 80 112 68 110 54 Z" fill="url(#d)"/>
    <path d="M90 54 L106 54 L101 79 C100 84 97 86 92 86 L83 86 Z" fill="url(#d)"/>
    <path d="M106 54 L118 54 C118 62 114 65 104 65" fill="none"/>
    <path d="M10 38 C22 35 44 35 62 36 L62 53 C48 55 30 59 16 65 L10 65 Z" fill="url(#m)"/>
    <path d="M6 37 L12 37 L12 66 L6 66 Z" fill="url(#d)"/>
    <path d="M186 42 L232 42 L232 47 L186 47 Z" fill="url(#m)"/>
    <path d="M196 39 L206 39 L206 50 L196 50 Z" fill="url(#d)"/>
    <path d="M232 39 L250 39 L250 50 L232 50 Z" fill="url(#d)"/>
    <path d="M124 37 L188 38 L188 51 L124 53 Z" fill="url(#m)"/>
    <path d="M60 33 L124 33 L126 38 L126 55 L60 55 Z" fill="url(#m)"/>
    <path d="M64 28 L156 28 L156 33 L64 33 Z" fill="url(#d)"/>
    <path d="M82 16 L112 16 L117 21 L117 28 L79 28 L79 21 Z" fill="url(#d)"/>
  </g>
  <path d="M68 30.5 H152" stroke="#3A3D36" stroke-width="2" stroke-dasharray="4 4"/>
  <path d="M136 43 H146 M152 43 H162 M168 43 H178" stroke="#5C6058" stroke-width="3"/>
  <path d="M80 41 H100" stroke="#5C6058" stroke-width="3"/>
  <path d="M236 42.5 V46.5 M241 42.5 V46.5 M246 42.5 V46.5" stroke="#2A2D27" stroke-width="2"/>
  <rect x="110" y="19" width="4" height="6" fill="#E8A33D"/>
</svg>
```

What makes it work: a tapered stock with a curved underside, a raked grip, a curved magazine, a rail with teeth and an optic, a handguard with vents, a gas block and a slotted muzzle. Each part has its own shape, so it reads as a rifle even at 32 px. Draw every weapon in the set this way, each with its own silhouette.

## Quality bar: sleek duotone line icon (48 grid)

Sleek icons are line art, but a production set adds one accent fill so they are not bare wireframes:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" stroke-linecap="round" stroke-linejoin="round">
  <path d="M9 28 V24 C9 15.7 15.7 9 24 9 C32.3 9 39 15.7 39 24 V28" stroke="#F5F7FA" stroke-width="2.5"/>
  <path d="M8 27 H13 C14.1 27 15 27.9 15 29 V37 C15 38.1 14.1 39 13 39 H11 C9.3 39 8 37.7 8 36 Z" fill="#FF9F6B" stroke="#F5F7FA" stroke-width="2.5"/>
  <path d="M40 27 H35 C33.9 27 33 27.9 33 29 V37 C33 38.1 33.9 39 35 39 H37 C38.7 39 40 37.7 40 36 Z" fill="#FF9F6B" stroke="#F5F7FA" stroke-width="2.5"/>
</svg>
```

For sleek item art in a shop (not the nav icons), go richer: soft filled shapes in two or three tones of the palette with a gentle highlight, no outline, the same accent colour.

## Rules for a consistent set

1. **One grid**: `viewBox="0 0 128 128"` for cartoony, trendy and pastel, `0 0 48 48` for sleek line icons, a wide box like `0 0 256 96` for weapons. Keep 6 to 8 percent padding so outlines are not clipped.
2. **One outline weight**: cartoony and trendy 8 to 10 on 128, pastel 4 to 6 in a darker pastel, sleek 2 to 2.5 on 48, military 2 to 3 hairline on 256.
3. **One light direction**: top-left, on every icon.
4. **Minimum feature size**: 4 units on a 128 grid (1 px at 32 px). Gaps between parts at least 4 units. Anything smaller disappears; remove it.
5. **Colour meaning**: coins gold, gems cyan/blue, cash green, close red, settings grey, shop orange, matching the UI palette's hex values.
6. **Identity**: one shape trait shared by every icon (a soft squircle backing, a notched corner, a tilt) or the game's emblem worked into the currency icon. A set that could ship in any game is a stock pack.
7. **Tier variants**: recolour the same drawing for rarity (bronze, silver, gold, rainbow) instead of drawing a new one per tier.

Look-only references for the bar: the packs on gfxcomet.com/category/packs and builtbybit.com/resources/free-icon-pack.54420. Study them, never copy a shape.

## Pattern tiles and frames

- **Halftone tile** (cartoony fills): 64x64, white dots on transparent, dot radius shrinking along a diagonal; tile with `ScaleType.Tile` at 0.75 to 0.9 `ImageTransparency`.
- **Stud tile** (trendy panels): 48x48, a rounded-square stud with a light top-left edge and dark bottom-right edge, on transparent.
- **9-slice frame** (RPG, horror, ornate panels): 256x256 with all decoration in the outer 48 px and a flat centre; use `ScaleType.Slice` with `SliceCenter = Rect.new(48, 48, 208, 208)`.
- **Light rays** (striking cards, featured offers): 256x256 white wedges radiating from the centre with a radial fade, used at 0.6 to 0.8 transparency behind item art.

## Rasterize, check, upload

1. Write each SVG to a file named after its use (`coin.svg`, `egg_meadow.svg`).
2. Rasterize to PNG with a transparent background at 2x the largest on-screen size (256 px for HUD icons and tiles, 512 px for hero art). Use whichever is installed: `rsvg-convert -h 256 coin.svg -o coin.png`, `resvg -h 256 coin.svg coin.png`, `inkscape coin.svg --export-type=png --export-height=256`, or `magick -background none -density 384 coin.svg -resize 256x256 coin.png`. librsvg renders `<use href>`, gradients, `clipPath` and an `feGaussianBlur` shadow (give the filter an explicit region, or the blur clips).
3. Look at every PNG, and at a 32 px render on the UI's background colour. Fix clipped outlines, muddy shapes and anything that does not read.
4. Upload with the MCP `store_image` tool (`file_path`: the PNG). It returns the image URI (`rbxassetid://<id>`). For images already on the web, `upload_image` takes `image_urls` (up to 10 per call) and returns a map from each URL to its asset id.
5. Keep a table of name to URI in one ModuleScript so the whole game uses the same uploads. Uploaded images go through moderation and may show blank for a short while. Never upload the same icon twice.

```lua
-- ReplicatedStorage.UI.Icons
return {
	Coin = "rbxassetid://0000000000", -- from store_image
	EggMeadow = "rbxassetid://0000000000",
}
```

Show multi-colour art with `ImageColor3` white. Single-colour line icons can be drawn white and tinted with `ImageColor3`.

## What not to do

- Do not build icons from Frames, and do not draw an egg as an ellipse or a weapon as rectangles.
- Do not mix a cartoony coin with a sleek line bag in one UI.
- Do not use emoji or Unicode symbols as icons.
- Do not copy icons from paid packs, free packs or other games, and never download or upload someone else's asset. Draw your own.
- Do not upload huge PNGs (over 1024 px) for small icons.
