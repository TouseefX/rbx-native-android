---
name: performance
description: How to measure and fix performance in Roblox experiences: frame budget, MicroProfiler, Developer Console, Script Profiler, memory and leaks, streaming, physics and rendering cost, RunService event choice, remote traffic and Parallel Luau. Pull it before optimizing or when reviewing code that runs every frame.
---

# Performance

Measure first, change one thing, measure again. All numbers below come from Roblox docs unless marked otherwise.

## Frame budget

- 60 FPS is 16.67 ms per frame, 30 FPS is 33.33 ms, 120 FPS is 8.33 ms. Consistent frame times matter as much as the average: one long spike is visible even at a high average (source: https://create.roblox.com/docs/performance-optimization/microprofiler).
- Server heartbeat is capped at 60 FPS; lower means the server is overloaded. The client cap is 60 FPS by default and players can raise it up to 240 on Windows (source: https://create.roblox.com/docs/performance-optimization/identify).
- Profile on phones and tablets when you can. Powerful desktops hide problems, especially with an FPS cap (source: microprofiler page above).
- Performance numbers inside Studio are skewed by the Studio app; confirm on a real client (identify page).

## Tools

| Tool | Open with | Use for |
| --- | --- | --- |
| MicroProfiler | `Ctrl+F6` (Cmd+F6) in Studio or desktop client | per-frame timing, spikes, which task caused them |
| Developer Console | `F9` or `/console` in chat | logs (client and server), memory, network, Script Profiler, Luau heap |
| Script Profiler | Developer Console, ScriptProfiler tab | CPU time per Luau function |
| Scene Analysis | Studio: Window > Performance Summary > Scene Analysis | what is in the scene and what it costs |
| Performance Stats | `Ctrl+Alt+F7` | quick memory, CPU, GPU, network overlay |
| Debug stats | `Ctrl+Shift+F1` to `F5`; `Shift+F5` for the summary (client, and Studio via Window > Performance > Stats) | rendering, physics, network, FPS summary |
| Performance Dashboard | creator dashboard | published-game trends (memory, FPS, crashes) |

(sources: https://create.roblox.com/docs/performance-optimization/identify, https://create.roblox.com/docs/studio/developer-console)

### MicroProfiler

The frame graph at the top shows one bar per frame; click a tall bar to see its timeline. Orange bars: worker-thread jobs (scripts, physics) took longer than rendering. Blue: the render thread took longer. Red: render-bound and GPU wait above 2.5 ms (source: microprofiler page). Focus on frame time, not the colour.

Label your own code so it shows up in the timeline:

```lua
--!strict
local RunService = game:GetService("RunService")

RunService.Heartbeat:Connect(function(dt: number)
	debug.profilebegin("EnemyAI")
	-- work
	debug.profileend()
end)
```

`debug.profilebegin(label)` and `debug.profileend()` must be paired in the same thread (source: https://create.roblox.com/docs/reference/engine/libraries/debug). Useful built-in scopes: `RunService.PreSimulation`, `RunService.PostSimulation`, `RunService.Heartbeat`, `physicsStepped`, `worldStep`, `stepHumanoid`, `stepAnimation`, `Prepare and Perform` (rendering) (source: https://create.roblox.com/docs/performance-optimization/improve).

### Script Profiler

Records CPU time of Luau function calls on Client or Server, at 1 kHz or 10 kHz, for 1, 5 or 10 minutes or until you stop it. It does not record threads that sleep or wait (source: https://create.roblox.com/docs/studio/optimization/scriptprofiler). Start it, reproduce the slow action, stop, sort by time spent.

## Memory

Open Developer Console > Memory (choose Client or Server). Key rows:
- `LuaHeap`: high or growing means a script leak.
- `InstanceCount`: always growing means instances are being kept alive by references.
- `PlaceScriptMemory`: per-script breakdown.
- On the client also check `GraphicsMeshParts`, `GraphicsTexture`, `Sounds`.

(sources: https://create.roblox.com/docs/performance-optimization/improve, https://create.roblox.com/docs/performance-optimization/identify)

Server memory: `6.25 GiB + 100 MiB x largest number of connected players`; keep use under about 50% (identify page). Servers do not shrink when players leave.

### Luau heap

Developer Console > LuauHeap > Create Snapshot (Client or Server). Views: Graph (aggregated tree of what references what), Object Tags (function, table, thread), Memory Categories, Object Classes, and Unique References / Unparented Instances (source: https://create.roblox.com/docs/studio/optimization/memory-usage). Take one snapshot early and one after the suspect behaviour, then compare which tags or scripts grew. You can label allocations with `debug.setmemorycategory(tag)` to see them under their own category (debug library page).

### Scene Analysis

Views include script memory per script, unparented instances held by scripts, instance composition (e.g. count of particle emitters), audio memory, animation memory (a common production leak), and triangle composition (shadows, opaque, transparent, terrain, grass, particles, sky, UI) (source: https://create.roblox.com/docs/performance-optimization/scene-analysis). Use it during a play session to find what is heavy before guessing.

### Common leaks

- Connections never disconnected. The engine never collects events connected to an instance or the values their callbacks capture. Disconnect, or destroy the owning instance (improve page).
- Tables keyed by player or object that never remove entries:

```lua
--!strict
local Players = game:GetService("Players")
local info: { [Player]: { score: number } } = {}
Players.PlayerAdded:Connect(function(p) info[p] = { score = 0 } end)
Players.PlayerRemoving:Connect(function(p) info[p] = nil end) -- the line people forget
```

- Instances that are created and kept (or parented to nil) but never `:Destroy()`ed.
- Player objects and characters after leaving: enable `Workspace.PlayerCharacterDestroyBehavior` or destroy them in `PlayerRemoving` / `CharacterRemoving` (improve page).
- Animation tracks and assets held after the model is gone (Scene Analysis).

## Scripts

- Do not run expensive work every frame. Frame events (`PreAnimation`, `PreRender`, `PreSimulation`, `PostSimulation`, `Heartbeat`) repeat each frame; limit them to work that must be per-frame, like camera updates (improve page).
- Spread large jobs across frames with `task.wait()` between chunks.
- Avoid heavy table operations (deep clone, serialize) on large structures in hot paths.
- Cache references. Do not call `Instance.new`, `FindFirstChild` chains or `GetDescendants()` every frame; look things up once and store them. Use `CollectionService` tags instead of scanning the tree.
- Use events instead of `while true do task.wait() end` polling.
- Server scripts can be compiled to native code with the `--!native` pragma (or `@native` on one function) for hot code (source: https://create.roblox.com/docs/luau/native-code-gen). Measure before and after.

### Which RunService event

(source: https://create.roblox.com/docs/reference/engine/classes/RunService)

| Event | Use |
| --- | --- |
| `PreRender` (client; supersedes `RenderStepped`) | camera and anything that must run just before rendering |
| `PreSimulation` (`Stepped` is older) | set up physics inputs before the physics step |
| `PostSimulation` / `Heartbeat` | game logic after physics, runs on server and client |
| `PreAnimation` | before animations are applied |

`RunService:BindToRenderStep(name, priority, fn)` is client-only and tied to rendering; use it only for camera/character control where ordering matters, and `UnbindFromRenderStep` when done. For everything else use a plain event or a timer.

## Physics

(source: https://create.roblox.com/docs/performance-optimization/improve)
- Anchor parts that do not need simulation.
- Set `CanCollide`, `CanTouch` and `CanQuery` to `false` on parts that do not need them (also saves physics memory).
- `MeshPart.CollisionFidelity`: `Box` has the lowest memory and cost; `Default` and `Precise` cost much more. Use Box for small or non-interactive meshes, Hull for small to medium, and hand-built invisible parts for large complex ones. Toggle Collision fidelity in Studio's Visualization Options to see shapes.
- Keep adaptive physics stepping on; reduce joints and self-collision in ragdolls.
- Humanoids are expensive: disable unused `HumanoidStateType`s on NPCs with `SetStateEnabled`, use `AnimationController` for static NPCs, pool NPCs instead of destroying and recreating, spawn only near players.
- Avoid changing size/scale or avatar hierarchy of live characters; `updateInvalidatedFastClusters` over 4 ms in the MicroProfiler signals avatar modification cost.

## Rendering and assets

(source: https://create.roblox.com/docs/performance-optimization/improve)
- Draw calls: view with Render Stats (`Shift+F2` in the client). Identical meshes with the same texture settings are instanced into one draw call, so reuse the same MeshId and SurfaceAppearance instead of re-uploading copies. Print `MeshId`s with `workspace:GetDescendants()` to find duplicates.
- Reduce object density and triangles in view; very complex scenes cost frame time.
- Shadows are costly: set `BasePart.CastShadow = false` on small or distant parts, disable `Light.Shadows` where not needed, limit light range, use fewer lights.
- Avoid stacking partly transparent surfaces (overdraw).
- `MeshPart.RenderFidelity` set to `Automatic` or `Performance` lets meshes fall back to simpler versions. With streaming enabled, `Model.LevelOfDetail = SLIM` and `Workspace.EnableSLIMAvatars` reduce far-model cost.
- Texture memory depends on pixel count, not file size. Most small images need at most 512x512; reuse one texture and tint with `SurfaceAppearance.Color`.
- Mobile: be stricter with texture sizes, particle counts, shadows and lights; test on real low-end devices (https://create.roblox.com/docs/performance-optimization/test-on-hardware).
- Audio files add memory when all are loaded at once; load per area.

## Streaming

`Workspace.StreamingEnabled` loads and unloads 3D content near the player; it is on for new places and cannot be set from a script (source: https://create.roblox.com/docs/workspace/streaming). Benefits: faster joins, lower memory, better frame rate on servers and clients. It changes replication, so code that assumes everything exists must be tested.

Properties set on Workspace in Studio:
- `StreamingMinRadius` and `StreamingTargetRadius` (target must be larger than min).
- `StreamOutBehavior` (default `LowMemory`), `ModelStreamingBehavior` (`Improved` recommended), `StreamingIntegrityMode` (`PauseOutsideLoadedArea` recommended; also `Disabled`, `MinimumRadiusPause`), `PredictiveStreamingMode`.

Model streaming mode (`Model.ModelStreamingMode`): `Nonatomic` (default), `Atomic` (the whole model streams together, so scripts can rely on its initial descendants), `Persistent` (always present after join, never streams out; use rarely, it occupies memory), `PersistentPerPlayer`.

Script patterns under streaming (source: https://create.roblox.com/docs/workspace/streaming/techniques):
- On the client, `WaitForChild` for anything in Workspace that may not be streamed yet, with a timeout; it can otherwise yield forever.
- Group things a script needs together in an Atomic model.
- On the server, call `player:RequestStreamAroundAsync(position, timeOut)` before teleporting or before firing a remote that references far-away instances. It yields.
- Streamed-out instances are parented to nil, so `ChildRemoved`-style signals will not tell you reliably; reconnect when they stream back in.
- Keep Persistent models and extra replication foci (`Player:AddReplicationFocus`) minimal.

## Networking

(source: https://create.roblox.com/docs/performance-optimization/improve)
- Do not send RemoteEvents every frame or on every input without throttling. Send only what changed (the purchased item, not the whole inventory).
- Batch small updates into one remote per interval.
- Creating or destroying large instance trees at runtime on the server replicates all of it; clone/parent in chunks or use streaming.
- Keep ReplicatedStorage and ReplicatedFirst lean, they replicate before the player can play. Use `PreloadAsync` only for assets that must be ready (loading screen, menu images, spawn area).

## Parallel Luau

(source: https://create.roblox.com/docs/scripting/multithreading)
- Put scripts under `Actor` instances; scripts in one actor run serially with each other, so use several actors for real parallelism.
- Enter the parallel phase with `task.desynchronize()`, or use `RBXScriptSignal:ConnectParallel`. Return to serial with `task.synchronize()` before writing to instances.
- Not all APIs are thread-safe (Unsafe, Read Parallel, Local Safe, Safe in the reference); unlisted members are Unsafe. `require()` does not work in a desynchronized phase.
- Good fits: NPC AI computation, raycast validation, procedural generation. Do not parallelize small work; the overhead dominates.

## Checklist

1. Reproduce and measure (MicroProfiler for spikes, Developer Console for memory, Script Profiler for functions).
2. Identify the category: scripts, physics, rendering, memory, network, load time.
3. Apply the cheapest documented fix from the matching section above.
4. Re-measure on the same scenario; keep the change only if it helped.
