//! The Android host of the editor: the bridge between a GameActivity app
//! (see `kotlin/MainActivity.kt`, built by xbuild's Gradle project) and the
//! same `rbx_studio` library the desktop `rbxstudio` binary runs.
//!
//! # The pieces
//!
//! - [`game_activity`]: the JNI surface the `androidx.games:games-activity`
//!   AAR's `GameActivity` base class calls into. `GameActivity` loads
//!   `librbx_android.so` and calls `initializeNativeCode` from `onCreate`
//!   before anything else; the methods it declares must all exist or the
//!   class fails to link, so the rest are no-ops — the Kotlin subclass and
//!   [`bridge`] handle input and surfaces, in the currency
//!   `gpui-pre-mobile`'s host-driven entry point speaks.
//! - [`bridge`]: the JNI surface of `MainActivity` itself — the surface,
//!   touch, key and IME calls that forward into
//!   `gpui_mobile::android::host`.
//! - [`boot`]: what `initializeNativeCode` starts — the render thread
//!   `gpui-pre-mobile` owns, with `rbx_studio`'s mobile boot as its launch
//!   closure and GPUI Kit's assets as its asset source.
//!
//! # How the editor ends up on screen
//!
//! `GameActivity`'s `initializeNativeCode` (Rust, [`game_activity`]) starts
//! `gpui-pre-mobile`'s host-driven path: a process-lived render thread that
//! builds GPUI's `Application` the first time a surface arrives. Kotlin's
//! `SurfaceHolder.Callback` forwards each `Surface` (as an `ANativeWindow`)
//! and its density to [`bridge`], which hands them to the host; the host
//! opens GPUI's window over it and runs the launch closure —
//! `rbx_studio::boot_mobile`, which initializes GPUI Kit and opens the
//! studio's Home screen. Touch, key and IME events take the same road in.
//!
//! The crate compiles to nothing off Android: every module is
//! `cfg(target_os = "android")`, and the `rlib` target only exists so
//! desktop `cargo check`/`clippy` still typecheck the file set.

#[cfg(target_os = "android")]
mod boot;
#[cfg(target_os = "android")]
mod bridge;
#[cfg(target_os = "android")]
mod game_activity;
