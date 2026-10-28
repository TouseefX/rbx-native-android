# rbx_android — the Android host

The editor as an Android app: the same `rbx_studio` library the desktop
`rbxstudio` binary runs, on a GPUI platform, inside a
[GameActivity](https://developer.android.com/games/sdk/game-activity)
surface — built and packaged by [xbuild](https://github.com/rust-mobile/xbuild).

```sh
# from this directory (xbuild picks the rbx_android package of the workspace)
x build --release --platform android --arch arm64 --format apk
```

CI builds it for every change (`.github/workflows/android.yml`): this tree
is far beyond what a laptop wants to rebuild per commit, and the
cross-compile needs an Android SDK/NDK to exist at all. The workflow
installs the NDK, xbuild and Gradle, builds and signs the APK, and uploads
it as an artifact — see its comments for the three adjustments it makes to
xbuild's generated Gradle project (a current NDK, and current AGP/Kotlin
plugin versions).

## What is where

| Path | What |
|---|---|
| `src/lib.rs` | The module map and the architecture doc comment. |
| `src/game_activity.rs` | The JNI surface of the AAR's `GameActivity` base class itself — `initializeNativeCode` starts the host, the rest are the no-ops the class needs to link — plus the inert `android_main` the native-activity glue `gpui-pre-mobile` links in requires (Android's `dlopen` refuses the library without it, `UnsatisfiedLinkError` and all). |
| `src/bridge.rs` | The JNI surface of `MainActivity` — surfaces, touch, keys, IME, resumed/paused, forwarded to `gpui_mobile::android::host`. |
| `src/boot.rs` | What `initializeNativeCode` starts: the render thread, the asset source, and `XDG_CONFIG_HOME` so the editor's settings persist. |
| `kotlin/MainActivity.kt` | The Activity: GameActivity subclass, surface callback, input parsing, and the invisible-`EditText` IME proxy. |
| `manifest.yaml` | xbuild's project manifest: the package id, SDK levels, permissions, the AAR dependencies, `gradle: true`. |

xbuild copies every file in `kotlin/` into the Gradle project it generates
under `target/x/release/android/gradle/` (the *workspace root's* `target/`),
resolves the Maven dependencies from `manifest.yaml`, packs
`librbx_android.so` (the cargo package name is the library name — and the
`android.app.lib_name` metadata it writes, which is why the crate is called
`rbx_android`) and runs `gradle assemble`.

## How it fits together

```
GameActivity (androidx.games AAR)          ┌─ the app's Activity ─────────┐
  └─ MainActivity (Kotlin, ours)            │ loads librbx_android.so,     │
       ├─ surface → nativeSurfaceCreated    │ owns the InputEnabled-       │
       ├─ touch    → nativeMotion           │ SurfaceView                  │
       ├─ keys    → nativeKey               └──────────────────────────────┘
       ├─ IME     → nativeIme (InputProxy: an invisible EditText with a
       │             real InputConnection — composing text, commits,
       │             surrounding-text deletion)
       └─ resume/pause → nativeResumed/nativePaused
                    │  (src/bridge.rs)
                    ▼
       gpui_mobile::android::host   (gpui-pre-mobile's host-driven entry
                    │                point: a process-lived render thread)
                    ▼
       rbx_studio::boot_mobile  →  the Home screen in GPUI's window,
                                    opened over the surface the host holds
```

- **Why GameActivity**: it is an `AppCompatActivity` hosting a
  `SurfaceView` with a real `InputConnection` wired for text input, which
  is what a software keyboard needs. Its native glue expects the
  `android-activity` crate's `game-activity` backend though, and
  `gpui-pre-mobile`'s host-driven path is none of that — so
  `src/game_activity.rs` implements the ~two dozen native methods
  `GameActivity` declares (it fails to link without them), with
  `initializeNativeCode` doing the real work and the rest inert.
- **Why the host-driven path** (`host`, not the `android_main` one in
  `jni`): it keeps the render thread — and therefore the GPUI
  application and the editor's state — alive across Activity
  recreation (rotation, dark-mode flips, the recents carousel), where a
  fresh native thread per Activity would lose them.
- **The window** is not opened by the editor: the host opens GPUI's
  window over the surface it was handed, then runs the launch closure
  (`rbx_studio::boot_mobile`) with it already alive.

## The version pairing

The whole tree shares one GPUI snapshot, pinned in three places that must
move together (each says so in a comment next to the pin):

1. `crates/rbx_studio`'s `gpui-kit = "=0.6.2"` — the release that made the
   toolkit hostable on mobile (`gpui_kit::is_mobile()`, the desktop
   platform selector compiled out on Android), built against gpui-pre
   0.3.5;
2. `vendor/gpui-pre` — gpui-pre 0.3.5 (vendored for the hover-easing
   patch, see `vendor/README.md`);
3. this crate's `gpui-mobile` git pin — the `gpui-pre-mobile` revision
   that pins gpui-pre `=0.3.5` and carries the host-driven entry point
   the GameActivity glue drives.

A later Kit moves to a later snapshot; a mismatch puts two `gpui-pre`s in
the tree and the types stop lining up.

## Installing and running

```sh
adb install -r dist/rbx-studio.apk
adb shell am start -n com.touseefx.rbxnative/.MainActivity
```

Logs — the editor's render thread, the platform, and any panic — go to
logcat under the tag `rbxstudio`:

```sh
adb logcat -s rbxstudio AndroidRuntime DEBUG
```

The APK from CI is signed with the runner's debug key: it installs, but it
is not for store distribution.
