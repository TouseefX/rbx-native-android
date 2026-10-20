# Loren (vendored)

[Loren](https://github.com/fabideveloper/Loren-Framework) is a Roblox Luau
game framework (Services, Controllers, dependency-ordered boot, packed
networking with typed specs and middleware) by **fabi**, released under the
MIT licence (`LICENSE` in this folder, kept verbatim). rbx-native offers it as
its optional built-in framework: `rbxstudio loren ...` is a Rust port of
Loren's `loren` CLI, and Home's "Loren project" template starts a game on it.
Nothing in the editor requires it — a project that ignores Loren works the
same as before. If you like it, use fabi's framework, star the repository
and read the [docs](https://fabideveloper.github.io/Loren-Framework/).

## Origin

- Repository: `https://github.com/fabideveloper/Loren-Framework`
- Pinned commit: `650154a286f9235e758002b514ce4170a50de7d6` (`main`), which
  is exactly the tag `v2.0.0-beta.2`. The npm package
  `loren-framework@2.0.0-beta.2` was checked to carry byte-identical
  `project/`, `lib/` and `index.js` to this commit.

## What is here

| Path | Upstream path | Notes |
| :--- | :--- | :--- |
| `LICENSE` | `LICENSE` (same text as `cli/LICENSE`) | verbatim |
| `project/` | `cli/project/` | the runtime (`loren/`), the bundled Promise (`loren_packages/Promise`, with **its own** `LICENSE`: roblox-lua-promise by Eryn L. K., MIT), the premades and the project template the CLI writes |
| `test/runner/` | `test-project/runner/` | Loren's spec runner (`LorenTest.luau`) |
| `crates/rbx_loren/tests/fixtures/` | `cli/test/fixtures/` | the CLI test suite's fixture projects, for the ported tests |
| `test/specs/` | `test-project/specs/{unit,runner,frozen}` | Loren's own unit specs and the frozen wire reference, run by `crates/rbx_loren`'s tests |

The Node CLI itself (`cli/index.js`, `cli/lib/`) is not vendored: it is
ported to Rust in `crates/rbx_loren`, module for module, with its test
suite ported alongside.

## Local changes

None to the files above, except the performance patches listed in
`patches/README.md` (today one, to `loren/shared/Codec.luau`). Each is a
self-contained diff against the pinned commit, kept so it can be offered
upstream. `Version.luau` still says `2.0.0-beta.2`: the patches change no
behaviour, only speed, so a project `rbxstudio loren update` brought to
this runtime is wire-compatible with fabi's.
