---
name: docs-search
description: How to look things up in Roblox's official documentation with the http_get tool (Engine API reference, creator guides, Cloud API, the llms.txt indexes) instead of guessing an API from memory. Pull it before using an API you are not sure of.
---

# Searching the Roblox documentation

Roblox APIs change: members get deprecated, new classes appear, limits move.
Before you write code against an API you are not certain about, read its page
with `http_get`. A wrong method name costs the user a debugging session; a
lookup costs one call.

## Where things are

`create.roblox.com/docs` serves every page as Markdown when `.md` is added to
its URL, and lists its pages in `llms.txt` indexes.

| What | URL |
|---|---|
| A class | `https://create.roblox.com/docs/reference/engine/classes/<ClassName>.md` |
| A datatype (CFrame, UDim2, TweenInfo) | `https://create.roblox.com/docs/reference/engine/datatypes/<Name>.md` |
| An enum | `https://create.roblox.com/docs/reference/engine/enums/<EnumName>.md` |
| Globals (`task`, `math`, `string`) | `https://create.roblox.com/docs/reference/engine/libraries/<lib>.md` and `.../globals/<Name>.md` |
| Everything deprecated, with replacements | `https://create.roblox.com/docs/reference/engine/deprecated.md` |
| Index of the Engine API | `https://create.roblox.com/docs/reference/engine/llms.txt` |
| A guide | `https://create.roblox.com/docs/<path>.md`, for example `.../docs/ui/position-and-size.md` |
| Index of every guide | `https://create.roblox.com/docs/llms.txt` |
| Index of the Open Cloud API | `https://create.roblox.com/docs/cloud/llms.txt` |

Names in reference URLs are the API's exact PascalCase name: `TweenService`,
not `tweenservice` or `Tween_Service`. Inherited members live on the class
that declares them: `Anchored` is on `BasePart.md`, not `Part.md`.

Roblox's creator-docs repository on GitHub (the source of the same pages) is
also reachable, for example
`https://raw.githubusercontent.com/Roblox/creator-docs/main/content/en-us/ui/index.md`.
No other site is: a URL outside these is refused.

## Use `query` first

`http_get` takes a `query`. With it, only the lines holding the keyword
(ignoring case) come back, each with a few lines around it and its line
number in the page; with no match, a single line says so. Class pages run to
hundreds of lines, so check with a query before taking the whole page.

```
http_get(url: "https://create.roblox.com/docs/reference/engine/classes/TweenService.md", query: "GetValue")
```

- `context_lines` widens each excerpt (default 3).
- `return_full: true` returns the whole page when the query matches, in one call.
- Without `query`, the whole page comes back.

## A lookup, step by step

1. You know the class: fetch it with a `query` for the member.
2. You know the domain but not the class: query the Engine index for a domain
   word (`http_get(url: ".../reference/engine/llms.txt", query: "tween")`),
   then fetch the class it names. The index lists classes, datatypes, enums
   and libraries with a one-line summary; it does not list members.
3. You need a workflow (streaming setup, data store limits, avatar specs):
   query the guide index (`.../docs/llms.txt`) for the topic, then fetch the
   guide.
4. A class page says "see the guide on X": follow that link with `http_get`.
   Links in the Markdown are site-relative (`/docs/...md`); put
   `https://create.roblox.com` in front.
5. Code from the user or from memory uses something old (`wait`, `spawn`,
   `Humanoid:LoadAnimation`, `BodyVelocity`): check `deprecated.md` for the
   replacement before you keep or copy it.

## Reading a class page

The top of each page is a front matter block (`inherits`, `tags` such as
`NotCreatable` or `Service`, `memory_category`), then the description and
code samples, then members grouped as properties, methods, events and
callbacks. Look for:

- **Deprecated** in a member's section: do not use it in new code.
- **Yields** on a method: it waits, so it cannot run inside a callback that
  must not yield, and it needs `pcall` when it calls a web service.
- **Security** (`PluginSecurity`, `RobloxScriptSecurity`): a game script
  cannot call it.
- **Thread safety**: what may run in parallel (Actors).
- Hidden or not-browsable members: they exist but may change; prefer the
  documented alternative.

## Open Cloud

The Cloud API index (`https://create.roblox.com/docs/cloud/llms.txt`) lists
every Open Cloud endpoint and links to its reference page. Read the page for
the endpoint's path, the API key scope it needs and its rate limits before
writing a request. Game servers reach Open Cloud through `HttpService` with a
key kept in the experience's secrets (`HttpService:GetSecret`), never a key
written into a script (source:
https://create.roblox.com/docs/cloud-services/secrets).

## When a fetch fails

- `URL not allowed`: the URL is outside the documentation sites. Rewrite it
  as a `create.roblox.com/docs` URL.
- `HTTP 404`: the name is wrong or the page moved. Check the casing, then
  query the right `llms.txt` index for the name.
- `No match for "..."`: the keyword is not on that page. Try a shorter word,
  the inherited class, or the index.

## Rules

- Do not guess an API name. If a query and the index both miss, tell the user
  you could not find it rather than inventing one.
- Quote what the page says (limits, yields, security level) instead of
  paraphrasing from memory.
- Prefer a `query` when checking several pages; fetch whole pages only when
  you need them.
