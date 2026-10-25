---
name: ui-style-icons
description: Drawing your own SVG icons that match the chosen Roblox UI style (outline weight, two-tone shading, highlight, rounded vs sharp joins, one grid), with example cartoony and sleek SVGs, pattern tiles and 9-slice frames, then rasterizing to PNG and uploading through the store_image / upload_image tools.
---

# Icons that match the style

Text-only buttons and emoji icons are a tell of generic UI. Every HUD button, currency, shop item and tab needs an icon, all drawn the same way. When no fitting asset exists, draw it as SVG yourself, rasterize it, upload it. Pull `ui-style` first so you know which style you are drawing for.

## What good icons look like

Look-only references for the quality bar: the packs on gfxcomet.com/category/packs and the free icon pack at builtbybit.com/resources/free-icon-pack.54420. Study them, never copy a shape. What they get right:

- **Consistent stroke**: every icon in a pack has the same outline weight and colour, so a coin, a gem and a gear sit together.
- **Two-tone shading**: a base fill plus one darker shade of the same hue on the side away from the light, and one highlight. No noisy gradients.
- **Readable silhouette**: you can tell what it is at 32 px as a solid shape. Big simple forms, a few bold inner lines, no hairline detail.
- **Tier variants**: the same icon recoloured (bronze, silver, gold, rainbow) for rarity, instead of a new drawing per tier.

Then give the set this game's identity (`ui-style` section 4): one shape trait shared by every icon (a soft squircle backing, a notched corner, a tilted angle) or the game's emblem worked into the currency icon. A set that could ship in any game is a stock pack.

## Rules for a consistent set

1. **One grid**: draw every icon on the same canvas (`viewBox="0 0 128 128"` for cartoony/trendy, `0 0 24 24` for sleek line icons). Keep 6 to 8 percent padding so outlines do not touch the edge.
2. **One outline weight**: cartoony/trendy 8 px on 128 (about 6 percent), sleek 1.5 px on 24, military 0 (flat silhouette) or 1 px hairline.
3. **One light direction**: top-left. Highlights go top-left, shadows bottom-right, on every icon.
4. **Joins**: cartoony, trendy, pastel use `stroke-linejoin="round"` and `stroke-linecap="round"`. Military, racing, striking headers use `miter` and sharp shapes.
5. **Shading** (cartoony/trendy): base fill, a darker shade of the same hue for the bottom/right third (no black shading), a white highlight blob at 0.5 to 0.7 opacity top-left, then the dark outline on top of everything.
6. **Outline colour**: near-black `#1A1A1A` or the darkest shade of the icon's own hue. Same choice across the set.
7. **Silhouette first**: the icon must read at 32 px as a filled shape. Avoid thin interior details.
8. **Colour meaning**: coins gold, gems cyan/blue, cash green, close red, settings grey, shop orange. Match the UI palette's hex values.

## Cartoony examples (128 grid)

Gold coin:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <circle cx="64" cy="68" r="52" fill="#C98A00"/>
  <circle cx="64" cy="60" r="52" fill="#FFC21A"/>
  <circle cx="64" cy="60" r="36" fill="#F2A900"/>
  <path d="M64 36 L70 54 L88 54 L74 65 L79 83 L64 72 L49 83 L54 65 L40 54 L58 54 Z" fill="#FFE27A"/>
  <ellipse cx="44" cy="34" rx="14" ry="8" fill="#FFFFFF" opacity="0.6" transform="rotate(-30 44 34)"/>
  <circle cx="64" cy="64" r="54" fill="none" stroke="#1A1A1A" stroke-width="8"/>
</svg>
```

Red close button (square, trendy):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <rect x="8" y="14" width="112" height="106" rx="18" fill="#A3121C"/>
  <rect x="8" y="8" width="112" height="100" rx="18" fill="#F2343F"/>
  <rect x="18" y="14" width="92" height="30" rx="14" fill="#FFFFFF" opacity="0.3"/>
  <path d="M42 36 L86 80 M86 36 L42 80" stroke="#1A1A1A" stroke-width="22" stroke-linecap="round"/>
  <path d="M42 36 L86 80 M86 36 L42 80" stroke="#FFFFFF" stroke-width="12" stroke-linecap="round"/>
  <rect x="8" y="8" width="112" height="112" rx="18" fill="none" stroke="#1A1A1A" stroke-width="8"/>
</svg>
```

Gem:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <path d="M32 20 H96 L120 52 L64 116 L8 52 Z" fill="#1E9BE0"/>
  <path d="M8 52 H120 L64 116 Z" fill="#147AB8"/>
  <path d="M32 20 L48 52 L64 20 Z M64 20 L80 52 L96 20 Z" fill="#7FD6FF"/>
  <path d="M24 46 L36 28" stroke="#FFFFFF" stroke-width="7" stroke-linecap="round" opacity="0.8"/>
  <path d="M32 20 H96 L120 52 L64 116 L8 52 Z" fill="none" stroke="#1A1A1A" stroke-width="8" stroke-linejoin="round"/>
</svg>
```

Four-point sparkle (striking style):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <path d="M64 4 C70 46 82 58 124 64 C82 70 70 82 64 124 C58 82 46 70 4 64 C46 58 58 46 64 4 Z" fill="#FFFFFF"/>
</svg>
```

## Sleek line examples (24 grid)

All share `fill="none" stroke="#F5F7FA" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"`:

```svg
<!-- close -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#F5F7FA" stroke-width="1.5" stroke-linecap="round"><path d="M6 6 L18 18 M18 6 L6 18"/></svg>
<!-- bag (shop) -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#F5F7FA" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5 8 H19 L18 20 H6 Z"/><path d="M9 8 V6.5 A3 3 0 0 1 15 6.5 V8"/></svg>
<!-- coin -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#F5F7FA" stroke-width="1.5"><circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="5"/></svg>
<!-- bell (notify) -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#F5F7FA" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 16 V11 A6 6 0 0 1 18 11 V16 L19.5 17.5 H4.5 Z"/><path d="M10 20 A2 2 0 0 0 14 20"/></svg>
```

Draw sleek icons white and tint them in Roblox with `ImageColor3`, so one upload serves every colour. Cartoony icons are multi-colour and must be uploaded as drawn.

## Military examples

Flat silhouettes, one colour (`#E7E9E2`), sharp corners, no outline. Draw a magazine as a slightly slanted rectangle with a notch, a grenade as a circle with a lever rectangle; tint with `ImageColor3`.

## Pattern tiles and frames

- **Halftone tile** (cartoony fills): 64x64, white dots on transparent, dot radius shrinking along a diagonal; tile with `ScaleType.Tile` at 0.75 to 0.9 `ImageTransparency`.
- **Stud tile** (trendy panels): 48x48, a rounded-square stud with a light top-left edge and dark bottom-right edge, on transparent.
- **9-slice frame** (RPG, horror, ornate panels): 256x256 with all decoration in the outer 48 px and a flat centre; use `ScaleType.Slice` with `SliceCenter = Rect.new(48, 48, 208, 208)`. Keep corners square to the grid so slicing does not cut through an ornament.
- **Light rays** (striking cards): 256x256 white wedges radiating from the centre with a radial fade, used at 0.6 to 0.8 transparency behind item art.

## Rasterize and upload

1. Write each SVG to a file (one icon per file, named after its use: `coin.svg`, `close.svg`).
2. Rasterize to PNG with a transparent background, at 2x the largest on-screen size (256 px for HUD icons, 128 px for small badges). Any of these works where installed:
   - `resvg coin.svg coin.png -w 256`
   - `rsvg-convert -w 256 coin.svg -o coin.png`
   - `magick -background none -density 384 coin.svg -resize 256x256 coin.png`
   - `inkscape coin.svg --export-type=png --export-width=256`
3. Look at the PNG before uploading. Check the outline is not clipped at the edge and the icon reads at 32 px.
4. Upload with the MCP `store_image` tool (`file_path`: the PNG). It uploads the image as an asset owned by the user and returns the image URI (`rbxassetid://<id>`). For images already on the web, `upload_image` takes `image_urls` (up to 10 per call) and returns a map from each URL to its asset id.
5. Set `ImageLabel.Image` (or `ImageButton.Image`) to the returned URI. Keep a table of name to URI in one ModuleScript (`Icons.Coin`, `Icons.Close`) so the whole game uses the same uploads.
6. Uploaded images go through Roblox moderation and may show blank for a short while. Do not upload the same icon twice; reuse the URI.

```lua
-- ReplicatedStorage.UI.Icons
return {
	Coin = "rbxassetid://0000000000", -- from store_image
	Close = "rbxassetid://0000000000",
}
```

## What not to do

- Do not mix a cartoony coin with a sleek line bag in one UI.
- Do not use emoji or Unicode symbols as icons.
- Do not copy icons from paid packs, free packs or other games, and never download or upload someone else's asset. Draw your own.
- Do not upload huge PNGs (over 1024 px) for small icons; it wastes memory.
