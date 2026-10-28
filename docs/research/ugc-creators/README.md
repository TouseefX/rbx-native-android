# UGC creators: what rbx-native has, what's missing

Goal: someone who makes avatar items (rigid accessories, layered clothing,
bodies, dynamic heads) can build, check, test and upload them from
rbx-native without opening real Studio. Written 2026-10-25. Docs read are
the `creator-docs` repo pages linked below.

## What real Studio gives a UGC creator

- **Accessory Fitting Tool (AFT)**, Avatar tab > Accessory. Pick a
  `MeshPart`/`Model`, pick Clothing (layered) or Accessory (rigid), pick the
  `AssetType` and body scale, then try it on preset bodies, animations and
  other accessories. It converts the mesh into the real `Accessory` object.
  <https://create.roblox.com/docs/avatar/accessory-fitting-tool>
- **Avatar Setup / Auto-Setup**. Takes a plain mesh and adds the R15 rig,
  skinning, facial animation (FACS), cages, the 15-part split and the
  attachments. Needs a specific input shape (head components, naming).
  <https://create.roblox.com/docs/avatar-setup>,
  <https://create.roblox.com/docs/avatar-setup/auto-setup-requirements>
- **Layered clothing**: `WrapLayer` on the item (inner cage = how it fits the
  body, outer cage = how other layers fit over it), `WrapTarget` on bodies.
  Must be skinned to the R15 armature, max 4 bone influences per vertex.
  <https://create.roblox.com/docs/avatar/layered-accessories/specifications>
- **Rigid accessories**: one `Accessory` with a `Handle` and an attachment,
  fitted by body scale.
  <https://create.roblox.com/docs/avatar/rigid-accessories/specifications>
- **Bodies and dynamic heads**: 15 watertight parts, outer cage on every
  part, bounding box limits per part, and for heads at least 17 FACS poses,
  eye/mouth landmarks in the cage, and 5 facial actions that must deform.
  <https://create.roblox.com/docs/avatar/character-bodies/specifications>,
  <https://create.roblox.com/docs/avatar/dynamic-heads/specifications>
- **Validation rules** (the ones that matter for a checker):
  - 4k triangles max for rigid and layered accessories, 20k per mesh in
    general (`art/modeling/specifications`)
  - Marketplace textures 2048x2048 max
  - max size per accessory type in studs, centred on the attachment
  - cages: never delete vertices or touch UVs, inner and outer cage named
    after the item
  - bodies watertight, each limb inside its bounding box
  - Roblox also ships a Blender/Maya "Layered Accessory Validation Tool"
    (<https://create.roblox.com/docs/art/accessories/validation-tool>)
- **Upload and publish**. Explorer > right click > Save to Roblox, Content
  Type "Avatar Item", pick title, asset category, creator (user or group).
  Studio validates, then you pay an upload fee (80 Robux per submission, 500
  if it uses an emissive mask) and it goes into moderation (up to 24h). After
  that you publish it on the Marketplace, which takes a refundable
  publishing advance. Creator must be ID verified or have a parental link.
  The fees page: <https://create.roblox.com/docs/marketplace/marketplace-fees-and-commissions>,
  the rigid walkthrough:
  <https://create.roblox.com/docs/art/accessories/creating-rigid/publishing>
- **Testing**: the AFT's preview bodies, and "test" docs for bodies and heads
  (<https://create.roblox.com/docs/avatar/character-bodies/test-studio>).

## What the Open Cloud Assets API can and can't do

From <https://create.roblox.com/docs/cloud/guides/usage-assets>: it creates
Animation, Audio, Decal/Image, Model (`.fbx`, `.gltf`, `.glb`, `.rbxm`,
`.rbxmx`, imported as a Model with MeshParts) and Video, one asset per call,
20 MB max. Mesh create is Roblox-only. Updating content only works for
`.fbx`. The page says nothing about avatar items or Marketplace listing.

So this is the hard wall: **the upload fee, moderation submission and
Marketplace publish are not in Open Cloud as far as the docs say.** What we
can do is upload the Model and hand the user to the Creator Hub for the
last step. Nobody has checked if a Model uploaded through the API can be
turned into an avatar item there; that needs a live try (stage 5).

## What rbx-native has today

Checked against `ROADMAP.md` "What's been implemented", `CHANGELOG.md` and
the code on `origin/dev`.

- **3D import** (`crates/rbx_import`, `shell/import3d.rs`): `.fbx`, `.obj`,
  `.gltf`, `.glb`, per-mesh triangle count against the **20k** general limit
  (`TRIANGLE_LIMIT`), uploads the self-contained file through
  `rbx_cloud::create_asset`. There is no 4k accessory budget and no
  asset-type awareness.
- **Auto-rigger** (`crates/rbx_autorig`): humanoid mesh to the 15 R15 parts,
  joints, bone-heat skinning (4 influences, sum 255), fit panel, draggable
  joints, accessory attachments from the fit, and the Mannequin's outer
  cages bent onto the fit (`fit_cages`), uploaded as `<Part>_OuterCage`.
  Live upload confirmed Roblox's importer makes one `WrapTarget` per part
  and a layered jacket and pants follow the published body. This is the body
  side of Avatar Setup, minus facial animation and head work.
- **Rig Builder, Player / My Avatar**: builds R6/R15 rigs, loads a real
  player's avatar with accessories welded at the right attachment, layered
  clothing keeping its `WrapLayer` and fitted cage to cage (see stage 3
  below). Close to Roblox's fit, not its exact deformer.
- **Properties, Explorer, viewer**: `Accessory`, `Attachment`, `WrapLayer`,
  `WrapTarget` and `FaceControls` load, show and edit as normal instances.
- **Save/Publish to Roblox** uploads places. Nothing there uploads an
  avatar item.
- **Skinned mesh rendering** (mesh v4 to v7) so a rig bends in the viewport.

## Gap analysis

### Have
Import and upload of a Model, auto-rig plus body cages, rig and avatar
loading, instance editing for all the avatar classes, skinned rendering.

### Missing (we can build it)
1. **Accessory Fitting Tool**: pick a mesh, pick Clothing or Accessory, pick
   the asset type and scale, preview on preset bodies, animations and other
   items, write out the real `Accessory` (`Handle`, attachment, `WrapLayer`).
2. **Validation panel** with the real numbers above: 4k tris, 2048 textures,
   size per asset type, max 4 influences, cage vertex and UV integrity,
   watertight bodies, bounding boxes, 17 FACS poses. Run before any upload.
3. **Cage authoring for items**: auto-make an inner/outer cage for a
   layered accessory (the auto-rigger only does body cages), show the cage
   as an overlay, and a way to nudge it.
4. **Real layered-clothing preview**: was a nearest-vertex blend, now a
   cage-to-cage fit checked against real Studio (stage 3).
5. **Head side**: dynamic head cage landmarks, FACS pose check, facial
   animation preview. `FaceControls` already loads.
6. **Avatar Setup for items**, not just bodies: attachments and skin for a
   rigid or layered item from a plain mesh.
7. **Upload flow**: a window that validates, uploads the Model, then opens
   the Creator Hub item page, with the fee spelled out before anything is
   sent.
8. **Test**: wear the item on the Player / My Avatar rigs and the preset
   bodies, play animations, swap body scale.

### Blocked by Roblox
- **Upload fee, moderation submit, Marketplace publish and the publishing
  advance** have no documented Open Cloud endpoint. We can't do the last
  step from here, so the plan ends in a handoff to the Creator Hub.
- **Mesh create** in the Assets API only takes content from Asset Delivery.
- **Roblox's actual deformer and server-side validator** aren't available.
  We can mirror the documented rules, not the checker itself.
- Avatar Schema changes (procedural bones, higher mesh resolution, more FACS,
  silhouette-preserving fit) are announced for 2027 and not in the API dump
  yet. See the Creator Roadmap section of `ROADMAP.md`.

## Staged plan

1. **Validator** (stage 1). Done: `crates/rbx_ugc` (no UI; `validate(&Item)`
   returns findings with rule, severity, measured value, limit and the part)
   and Avatar › Check Item. Import 3D can check a file as an avatar item.
   - Checked in real Studio 2026-10-28 on generated items (1k, 5k and 21k
     triangle spheres, a 4-stud hat, a 4096 texture, a shirt with no
     cages): Save to Roblox gave the same verdicts as ours, with the same
     numbers (5000 over 4000, 4096x4096 over 2048x2048, the Hat box
     [3, 4, 3]).
   - What Studio taught us that no page says: the size box comes from the
     handle's `AvatarPartScaleType` value and an accessory without one is
     sized as Classic (so an unmarked hat gets 3 x 4 x 3, not Normal's
     1.87 x 2.5 x 1.87); a rigid handle needs a `TextureID` or a
     `SurfaceAppearance`, a layered one doesn't. Both are mirrored.
   - Not mirrored: Studio's "Missing HSR data" error on a layered item
     (Studio generates that data itself) and its schema check (stray
     instances under the handle). The Studio importer decimates a mesh over
     20k triangles instead of refusing it; our importer still refuses.
   - The AFT never got past its Select step on these items in our run, so
     its own messages were not compared.
   - No page gives a size box for shoes, a vertex/UV count for Roblox's
     template cages, or a publishing advance for eyebrows and eyelashes;
     those rules are skipped. The Slender hair depth row adds up wrong on
     the page (1.892 + 1.189 for 2.08); 0.892 front is used.
2. **Fitting tool, rigid items** (stage 2). Done: Avatar › Accessory opens
   the Accessory Fitting Tool. Pick a `MeshPart` or `Model`, Accessory or
   Clothing, the asset type (Studio's list, Shoulder with its five
   attachments) and the body scale, place it on the test body, Generate.
   - Checked against Studio's AFT run on the bow (6874004992), its output
     saved and dumped: the `Accessory` is named after the source with
     `AccessoryType` Unknown, the source part becomes `Handle` and keeps
     its mesh, size and children, then a `StringValue`
     `AvatarPartScaleType` (Classic, ProportionsNormal, ProportionsSlender)
     and an `Attachment` named for the type whose `CFrame` is the handle's
     inverse times the body attachment's world `CFrame` (identity for an
     untouched fit, `(0.783, 0, 0)` after a 0.783 stud move). `Anchored`
     and `Massless` off, rigid `CanCollide` on, layered off. Ours writes
     the same; the generator's tests are built from that dump.
3. **Fitting tool, layered items plus cage authoring** (stage 3). Done:
   Clothing makes `<item>_InnerCage` and `<item>_OuterCage` from the
   Mannequin's body cages (`rbx_autorig::item_cages`: the template welded,
   pushed out along its normals until it clears the item, smoothed, every
   UV kept; each bit of the item pushes only the cage vertex nearest it),
   shows them as an overlay with a nudge by region, and writes a
   `WrapLayer` the way Studio's tool does (renamed `HandleWrapLayer`,
   Order 1, Puffiness 1, ShrinkFactor 0, `AutoSkin` kept).
   - The deformer is new (`rbx_viewer` `wrap/cage_map.rs`): body cages and
     every item cage share one template, and the template UV is the
     pairing key (checked: all 705 UVs of the Mannequin's UpperTorso cage
     are in the Denim Jacket's inner cage). Each garment vertex is bound to
     the closest spot on its inner cage and rebuilt at the same spot of the
     body cage with the same clearance; each layer's outer cage becomes the
     surface the next layer is worn over.
   - Checked in real Studio on the catalog Denim Jacket (8516609675) and
     Cargo Pants (6984740059) on the default blocky body at Normal,
     Slender and Classic, same camera: garment-pixel XOR over union went
     from 0.81 / 0.75 / 0.76 with the old nearest-vertex fit to 0.11 /
     0.13 / 0.12 (whole frame 0.78 to 0.12). What's left is a few spots
     between the legs where Studio hides the body under the cloth (its HSR
     data) and we don't.
   - `WrapLayer` tuning, measured in Studio on the same jacket (Normal
     body, one camera per setting): `BindOffset` moves the fitted garment
     as a whole, about the character's `HumanoidRootPart`. `(0, 0.5, 0)`
     raises it half a stud (21 px in Studio, 20 px in ours, same camera),
     `CFrame.Angles(0, 0, 0.3)` turns it about the root part, not the
     handle (checked on a 1.5x tall body where the two sit 0.58 studs
     apart). Ours does the same. `Puffiness` (-1, 0, 0.5, 1,
     and 2, which Studio clamps to 1) and `ShrinkFactor` (-1, 0.5, 1) draw
     pixel for pixel the same as the defaults in Studio, alone or over a
     second layer; the API page marks both deprecated. So they're written
     but not applied, and the tool clamps them to -1..1 like Studio. On
     the Mannequin our jacket is still slimmer than Studio's (garment
     XOR/union 0.31 at the defaults on that body, the same before and
     after this change).
   - Eyebrows and eyelashes: Studio's AFT lists them with the rigid types,
     but Save to Roblox on a rigid eyebrow says "Could not find required
     WrapLayer" and "Asset type 'EyebrowAccessory' is not a rigid
     accessory category. It can only be used with layered clothing." The
     tool keeps them in the Accessory list and fits them layered on the
     Head, and Check Item keeps asking for their cages.
   - Not checked: whether Roblox's own
     validator accepts cages built from the Mannequin template once
     uploaded, since that takes a paid upload.
4. **Heads** (stage 4). Landmarks, FACS check, face preview.
   - Check: Studio's own dynamic head validation on the same head.
5. **Upload and handoff** (stage 5). Done: Avatar › Upload Item (type,
   title, creator, the check, the fees, Model upload through Open Cloud,
   Creator Hub link). Tested against a mock Open Cloud server.
   - Still to check, live: does a Model uploaded by the API show up as
     something the Creator Hub can turn into an avatar item? The window
     says it isn't confirmed. Needs the tester's account, a key with the
     assets scope, and real Robux for the upload fee, so Chee has to be
     the one pressing it.
6. **Test rig** (stage 6). Done, inside the fitting tool: Player / My
   Avatar bodies and the Normal, Slender and Classic presets, the default
   animations, body scale swaps and other items layered on top.
   - Check: the same jacket and pants on the same three bodies in real
     Studio, screenshot against screenshot (the stage 3 numbers above).

Every stage needs a real place or item opened in real Studio on an X server
before it counts as done, same as the rest of the project.

## Open questions

- Which accessory types the AFT lists today. Save to Roblox's Asset
  Category list (2026-10-28): Body, Head, Shoes, Avatar Animations, Back,
  Dress Skirt, Emote, Eyebrow, Eyelash, Eye Makeup, Face, Face Makeup,
  Front, Hair, Hat, Jacket, Lip Makeup, Neck, Pants, Shirt, Shorts,
  Shoulder, Sweater, T-Shirt, Waist.
- Whether the Open Cloud Assets API will ever take avatar item metadata.
- Whether a validator can lean on `rbx_autorig`'s voxeliser for the
  watertight check or needs its own.
