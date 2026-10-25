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
  clothing keeping its `WrapLayer` and fitted by nearest vertex blend.
  That is a viewer-side approximation, not Roblox's deformer.
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
4. **Real layered-clothing preview**: today it's a nearest-vertex blend. A
   deformer closer to Roblox's, checked against real Studio, is the thing
   that makes try-on trustworthy.
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

1. **Validator** (stage 1). Asset-type table, 4k/2048/size/influence checks
   on an imported mesh, a report window. Smallest piece, useful alone, and
   the import dialog can reuse it.
   - Check in real Studio: import the same mesh, run the AFT and
     Save to Roblox up to the validation step and compare every message and
     number. The size table especially, per accessory type and body scale.
2. **Fitting tool, rigid items** (stage 2). Accessory type, attachment
   placement on the mannequin, scale, writes the `Accessory`.
   - Check: the generated `Accessory` saved to a `.rbxl` and diffed against
     the AFT's output (Handle, attachment name and `CFrame`, `AvatarPartScaleType`).
3. **Fitting tool, layered items plus cage authoring** (stage 3). Inner and
   outer cage creation, `WrapLayer` properties, preview.
   - Check: upload through the API, read back what the importer made, wear
     the item in Studio on Normal, Slender and Classic bodies, compare the
     fit to ours. This is where the deformer gap shows.
4. **Heads** (stage 4). Landmarks, FACS check, face preview.
   - Check: Studio's own dynamic head validation on the same head.
5. **Upload and handoff** (stage 5). Window, fee notice, Model upload,
   Creator Hub link.
   - Check, live: does a Model uploaded by the API show up as something the
     Creator Hub can turn into an avatar item? If not, say so in the window.
   Needs the tester's account, a key with the assets scope, and real Robux
   for the upload fee, so Chee has to be the one pressing it.
6. **Test rig** (stage 6). Wear on Player / My Avatar and the presets, play
   animations, body scales.
   - Check: same item on the same avatar in real Studio, screenshot against
     screenshot.

Every stage needs a real place or item opened in real Studio on an X server
before it counts as done, same as the rest of the project.

## Open questions

- Which accessory types the AFT lists today, and their exact size table
  (the spec page has the numbers by body scale; copy them from the page, not
  from memory, when stage 1 starts).
- Whether the Open Cloud Assets API will ever take avatar item metadata.
- Whether a validator can lean on `rbx_autorig`'s voxeliser for the
  watertight check or needs its own.
