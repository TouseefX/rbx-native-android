---
name: debugging
description: How to find and fix bugs in Roblox experiences: Output and logging, the Studio script debugger, Developer Console, test modes for client/server bugs, common Roblox bug classes, and a step-by-step checklist for debugging through MCP tools. Pull it when something errors, behaves wrongly, or only fails in some conditions.
---

# Debugging

## Read the evidence first

1. Read the Output window (Studio: View > Output). Errors show the script, line and a stack trace. Fix the first error, not the last; later ones are often consequences.
2. In a playtest, the Output shows client and server messages; use the Developer Console (`F9` or `/console`) to switch between the Client and Server logs, and filter by Output, Information, Warning and Error. Server logs in a live game are visible only to the owner or group members with permission (source: https://create.roblox.com/docs/studio/developer-console).
3. Log files on disk: Windows `%LOCALAPPDATA%\Roblox\logs`, Mac `~/Library/Logs/Roblox` (source: https://create.roblox.com/docs/studio/debugging).

## Logging that helps

- Put context in every message: which function, which player, which values.

```lua
--!strict
print(string.format("[Shop] purchase player=%s item=%s price=%d", player.Name, itemId, price))
warn("[Shop] unknown item", itemId, debug.traceback())
```

- `print` for normal flow, `warn` for recoverable problems, `error` for failures that must stop the thread. `warn` and `error` are highlighted in the Output and the Developer Console filters.
- `debug.traceback(message, level)` returns a string with the call stack; use it in `warn` to see how you got there (source: https://create.roblox.com/docs/reference/engine/libraries/debug). `debug.info` can read function names and lines.
- Prefer a logpoint (below) over adding and removing `print` lines.
- Remove noisy logs once fixed. Print inside frame events spams the Output and slows the game.

### Capture errors in code

`ScriptContext.Error(message, stackTrace, script)` fires for uncaught script errors, so you can log or report them (source: https://create.roblox.com/docs/reference/engine/classes/ScriptContext).

```lua
--!strict
local ScriptContext = game:GetService("ScriptContext")
ScriptContext.Error:Connect(function(message: string, stackTrace: string, script: Instance?)
	-- send to your logging; do not error in here
end)
```

`LogService:GetLogHistory()` returns a table of past log entries (message string, message type and more) and `LogService.MessageOut` fires on each new message. Use them to read what already happened or to forward logs (source: https://create.roblox.com/docs/reference/engine/classes/LogService). `LogService:ClearOutput()` also clears that history.

## The Script Debugger

Studio supports breakpoints in the Script Editor (source: https://create.roblox.com/docs/studio/debugging):

- Standard breakpoint: click the gutter beside the line number. Execution pauses there, with a yellow arrow on the next line to run.
- Conditional breakpoint: right-click the gutter > Insert Conditional Breakpoint, enter a condition such as `i == 10`; it only pauses when true. Use it inside loops or per-player code.
- Logpoint: Insert Logpoint, enter a message such as `"value:", var`; it prints to the Output without pausing.
- Temporary breakpoint: removes itself after the first playtest session.
- Disable instead of delete when you want to keep the condition or message.

When paused:
- **Watch** window: Variables tab shows variables in scope; My Watches lets you enter expressions.
- **Call Stack** window shows how execution reached the current line.
- **Breakpoints** window lists all breakpoints; each shows contexts (Client, Server, Edit) so you can enable one side only.
- Step Into `F11`, Step Over `F10`, Step Out; Resume Scripts to continue; Stop to end.
- Hover a variable in the editor to see its value, including table contents.
- The Command Bar (Studio, or the Developer Console on the server view) runs Luau against the running game for quick experiments.

## Reproduce the right environment

(source: https://create.roblox.com/docs/studio/testing-modes)
- **Test (F5)**: runs the game with your avatar at a spawn. In solo mode you can toggle between Client and Server views.
- **Test Here**: same, but inserts your avatar at the camera position.
- **Run (F8)**: simulates without inserting your avatar; good for server-only logic and NPCs.
- **Server & Clients**: launches a separate server and 1 to 8 client sessions (start with 1 to 2). Use this for anything involving more than one player, replication, or a bug that appears only on a client.
- Network simulation (latency, jitter, packet loss) is available in Studio; use it for timing-dependent bugs.
- Studio performance and exact timing differ from a published client; confirm important fixes in a published place too.

If a bug only reproduces on one side, put breakpoints or logs on both and compare. A value correct on the server and wrong on the client points to replication or a client-only script.

## Common Roblox bug classes

| Symptom | Likely cause | Check |
| --- | --- | --- |
| "X is not a valid member of Y", or indexing nil, intermittently | Client script ran before the instance replicated, or it streamed out | Use `WaitForChild` with a timeout; with StreamingEnabled see the `performance` skill streaming section (https://create.roblox.com/docs/workspace/streaming/techniques) |
| Works once, breaks after death | Script cached the character, humanoid or parts at start | Re-fetch on every `CharacterAdded`; disconnect old connections |
| Infinite yield possible warning | `WaitForChild` on a name that does not exist or is spelled differently | Check the exact name, add a timeout, handle `nil` |
| Remote handler sees wrong data | Arguments change in transit: instance keys in tables become strings, functions arrive as nil, metatables are lost, mixed numeric and string keys and nil holes break tables | Pass plain dictionaries or arrays of simple values (source: https://create.roblox.com/docs/scripting/events/remote); validate types on the server |
| Touched fires many times | It fires for each part that contacts another, repeatedly as parts move in and out of contact (source: https://create.roblox.com/docs/reference/engine/classes/BasePart) | Add a debounce or per-player cooldown (https://create.roblox.com/docs/scripting/debounce); filter by `GetPlayerFromCharacter` |
| Event never fires | Wrong side (server listening to a client-only event), or script in the wrong container, or `RunContext` mismatch | Check script type, location, `RunContext` (https://create.roblox.com/docs/scripting/locations) |
| Server saves nothing at shutdown | `BindToClose` callbacks get 30 seconds then the server exits | Keep the callback short, save in parallel (source: https://create.roblox.com/docs/reference/engine/classes/DataModel) |
| Purchases granted twice or lost | `ProcessReceipt` must return a `ProductPurchaseDecision`; `NotProcessedYet` means it will be called again | Make processing idempotent and return `PurchaseGranted` only after the grant succeeded (https://create.roblox.com/docs/reference/engine/classes/MarketplaceService) |
| Memory/lag grows over time | Leaked connections, tables, instances | See the `performance` skill |
| Animation doesn't play | Animator not loaded yet, asset not owned by the experience, wrong priority, track garbage collected | Check Output for asset errors and priority (see the `animation` skill) |

Note on yielding: a handler that yields (`task.wait`, `WaitForChild`, a DataStore call) blocks only its own thread, but `BindToClose` has a 30 second budget and a `ProcessReceipt` that never returns blocks receipt processing, so avoid unbounded yields in those two.

## Find the cause

- **Binary search**: disable half the scripts (or comment out half of a function), see if the bug remains, narrow down. Works for conflicts between plugin or free-model scripts too; inspect scripts that generate extra instances or look obfuscated (source: https://create.roblox.com/docs/performance-optimization/identify).
- **Minimal repro**: make a new empty place with only the failing logic.
- **Check assumptions with output**: print the type and value of what you think you have, `typeof(x)`, `x == nil`, `x:GetFullName()` for instances.
- **Stack traces first**: the traceback names the script and line; open exactly that line.
- Turn on `--!strict`; many nil and typo bugs become type errors before runtime (see the `luau-scripting` skill).

## Checklist for debugging through MCP tools

1. State the symptom in one sentence: what happened, what you expected, on which side (server/client), and when.
2. Read the Output (or console log history) and copy the exact error text and the script and line it names. Do this before touching code.
3. Read the script that errors. Then grep the project for the symbols and remote names involved to find the other side (who fires, who listens, where it is required).
4. Check the script's type and location (Script/LocalScript/ModuleScript, `RunContext`, container). Wrong container is a frequent cause of "nothing happens".
5. Form one hypothesis. Say what output would prove or disprove it.
6. Add one log or logpoint to test it, run the smallest test that reproduces it (Test, Run or Server & Clients), read the Output again.
7. Fix the cause, not the symptom: wrapping in `pcall` or an `if x then` hides the bug unless `x == nil` is truly expected.
8. Re-run the original reproduction and confirm the original error is gone and no new warnings appear. Remove temporary logs.
9. If you cannot reproduce, widen the environment (multiple clients, network simulation, respawn, streaming) rather than guessing.
