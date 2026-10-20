# Contributing to rbx-native

Thanks for considering it. This project is young and moves fast, so the
short version: read [ROADMAP.md](ROADMAP.md) to see what's actually needed,
keep changes small and verified, and ask before doing something big.

## Before you start

- Skim [README.md](README.md) for what this project is and how to build it.
- Read [GUIDELINES.md](GUIDELINES.md) and [SPECS.md](SPECS.md), the Rust
  style and architecture rules every change here follows. They're short;
  read them once rather than getting review comments about them repeatedly.
- Check [ROADMAP.md](ROADMAP.md) for what's already done, what's in
  progress, and what's out of scope on purpose. It's where the project's
  direction lives, and it's more up to date than anything here.
- For anything non-trivial (a new feature, a change to the architecture, a
  new dependency), open an issue or a discussion first. Saves everyone a
  round-trip if the direction turns out to be wrong.
- **On Windows?** CI builds and tests it there on every change, but very
  few people run the editor on it day to day, see the "Platform support"
  section of [README.md](README.md). Reporting exactly what breaks on your
  machine is one of the most useful things you can contribute, even without
  fixing anything yourself.

## Setting up

Fork the repository, clone your fork and add this one as `upstream`,
[README.md](README.md#contributing) walks through it, along with keeping
your fork in sync and opening the pull request. Then:

```sh
cargo build --release
./scripts/check.sh
```

See [README.md](README.md#building) for platform-specific build
dependencies. `./scripts/check.sh` is the gate every change is held to,
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, and the full test suite. Run it before opening a PR; CI runs the
same script.

Some tests need real Roblox files this repository intentionally does not
ship (see [README.md](README.md#assets--fixtures)); they're `#[ignore]`d and
read a path from an environment variable, so they're skipped cleanly if you
don't have one, you don't need Roblox assets on hand to contribute.

## Making a change

1. **Every change gets its own branch off `dev`**: a feature, a bug fix,
   a docs tweak, all of it. `dev` is where every pull request targets;
   `main` tracks releases and only ever receives `dev`, merged by the
   maintainer alone (a ruleset blocks everyone else, collaborators
   included), never a feature branch directly. Never commit straight to
   either. A short, descriptive name is enough:
   `fix/beam-texture-orientation`, `feat/surfacegui-text`,
   `docs/windows-build-steps`.
2. Keep the change scoped to one thing. A bug fix doesn't need a
   refactor riding along with it; a new feature doesn't need to touch
   unrelated code on the way. Small, reviewable PRs get merged faster than
   large ones.
3. Match the existing code's style and the conventions in
   [GUIDELINES.md](GUIDELINES.md), naming, error handling, module layout,
   and especially comments: explain *why* a decision was made, not what the
   code already says it does.
4. Add or update tests for what you changed. If you're touching rendering,
   lighting, or anything visual, a passing test suite alone isn't proof of
   a correct fix, build the relevant binary and actually look at the
   result (a screenshot, a real place file) before calling it done.
5. Run `./scripts/check.sh`. It must be green.
6. Write a commit message that explains *why*, not just what changed, the
   diff already shows what changed.

## Testing (no exceptions)

This is the one rule I won't bend on. A PR doesn't get merged until all
three of these are done, for everything it touches:

1. **Unit tests.** Every new behaviour and every bug fix comes with tests
   that would fail without the change. `./scripts/check.sh` has to be green.
2. **End to end.** Build `rbxstudio` (or the tool you changed) and actually
   use the feature on a real place: click through it, undo it, save, reopen.
   A nested X server like Xephyr is fine for this. Screenshots or a short
   recording go in the PR.
3. **Against real Roblox Studio.** If it copies something Studio does, open
   the same thing in real Studio and compare them side by side. If it saves,
   exports or publishes, open the result in real Studio and check it loads
   and looks right. If it talks to Roblox (Open Cloud, publishing, Play),
   run it for real at least once, not only against a mock. Put both
   screenshots in the PR.

And nothing gets left for later:

- "Not tested", "not verified", "should work", "follow-up" or "TODO" in a PR
  means it isn't ready. Finish it or take it out of the PR.
- If a limit is there on purpose (Roblox doesn't publish a formula, a
  feature needs Roblox's own servers), say what it is and why in the PR.
  That's a design choice, not unfinished work.
- If part of it only runs on a platform you don't have, say so, and it waits
  until someone has tested it there.

PRs that skip any of this get sent back, no matter how small they are.

## Opening a pull request

Opening one fills in a template (`.github/PULL_REQUEST_TEMPLATE.md`), fill
it in rather than deleting it, it covers exactly what's below.

- Describe what the change does and why, and how you verified it (test
  output, a screenshot, a specific file you checked it against).
- If it touches something visual, include a before/after screenshot where
  practical, it's the fastest way for a reviewer to confirm the change
  actually does what it claims.
- Reference the relevant `ROADMAP.md` line or an open issue if there is one.
- Expect review comments. This is a young project with strong opinions on
  style and architecture (see `GUIDELINES.md`), they're enforced
  consistently, not personally.

## Reporting a bug

Open an issue, the bug report template walks you through it: what you did,
what you expected, what happened instead, and, if it's a rendering issue,
a screenshot and, ideally, the exact camera/place setup that reproduces it.
"It looks wrong" is much harder to act on than "this part's specular
highlight is missing, here's the file and
the angle."

## A note on assets and legality

This project does not ship, embed, or commit any Roblox-owned asset (place
files, meshes, textures, icon sheets, or anything else extracted from the
Roblox client). If your change would need one, it should either fetch it
anonymously from Roblox's public CDN at runtime/test time, or read it from a
path a contributor supplies themselves via an environment variable, never
from a file added to this repository. If you're not sure whether something
you want to add is safe to ship, ask before opening the PR.

## Using AI tools

Fine by me, I use them too. Same bar as any other change though: you're the
one opening the PR, so read what it wrote, run `./scripts/check.sh`, and
test it against real Studio before you ask for a review. A few extra rules
that come up a lot:

- Keep files under about 400 lines. Split into submodules instead of growing
  one file (see `GUIDELINES.md` §6).
- Comments describe the code as it is now. No "fixed in task 12" or "as
  discussed", nobody reading the code later knows what that means.
- No new dependency without a reason, and prefer one that's already in
  `Cargo.lock`.
- Don't trust Roblox behaviour from memory. Roblox's docs are on GitHub
  ([Roblox/creator-docs](https://github.com/Roblox/creator-docs)), check
  them, and if they don't cover something, say so in a comment instead of
  guessing.
