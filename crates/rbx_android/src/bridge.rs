//! The JNI surface of `MainActivity` (`kotlin/MainActivity.kt`): the
//! surface, touch, key and IME calls that forward into `gpui-pre-mobile`'s
//! host-driven entry point.
//!
//! Kotlin parses the Android events — `MotionEvent`'s pointers, the key's
//! meta state — and passes plain values across, so nothing here has to call
//! back into Java except turning a `Surface` into the `ANativeWindow` the
//! platform renders through. Every function is called on the Java UI
//! thread; the host's functions are all safe from it (they post commands to
//! the render thread, and `surface_destroyed` blocks until that thread has
//! let the surface go, which is what `SurfaceHolder.Callback` owes the
//! framework before returning).

// The `jni` module of `gpui-pre-mobile` is imported by the functions it
// provides (`with_env`, …), not as a name: it would shadow the `jni` crate
// every signature below types its JNI arguments through.
use gpui_mobile::android::host;
use gpui_mobile::android::jni::{get_string, with_env};
use jni::objects::{JObject, JPrimitiveArray};
use jni::sys::{jboolean, jfloat, jint, jlong, jobject};
use jni::Env;

/// Hand a `Surface` to the render thread, with the density its logical
/// pixels scale by. Call from both `surfaceCreated` and `surfaceChanged`:
/// the host treats a repeat of the window it already holds as a resize.
///
/// # Safety
/// Called by JNI with a valid `android.view.Surface` local reference.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeSurfaceCreated(
    env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    surface: jobject,
    density: jfloat,
) {
    surface_arrived(env, surface, density);
}

/// # Safety
/// Called by JNI with a valid `android.view.Surface` local reference.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeSurfaceChanged(
    env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    surface: jobject,
    density: jfloat,
) {
    surface_arrived(env, surface, density);
}

fn surface_arrived(env: *mut jni::sys::JNIEnv, surface: jobject, density: jfloat) {
    if surface.is_null() {
        log::warn!("rbx_android: null surface handed to the host");
        return;
    }
    // SAFETY: `env`/`surface` are the valid JNI arguments of the entry point
    // this was called from, and the acquired window is handed to the host,
    // which releases it when the surface goes away. The cast is between the
    // two `jni-sys` majors the graph carries (jni 0.22 pins 0.4, ndk 0.9 pins
    // 0.3): both bind the same C `JNIEnv*`/`jobject` values, and
    // `from_surface` only forwards them to `ANativeWindow_fromSurface`.
    match unsafe { ndk::native_window::NativeWindow::from_surface(env.cast(), surface.cast()) } {
        Some(window) => host::surface_created(window, density),
        None => log::error!("rbx_android: ANativeWindow_fromSurface failed"),
    }
}

/// Take the surface back. Blocks — on the Java UI thread — until the render
/// thread has stopped using it, which is the contract
/// `SurfaceHolder.Callback.surfaceDestroyed` owes the framework.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeSurfaceDestroyed(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
) {
    host::surface_destroyed();
}

/// Deliver a whole `MotionEvent`'s pointers. `action` is
/// `getActionMasked()`, `action_index` is `getActionIndex()` (significant
/// only for `POINTER_DOWN`/`POINTER_UP`), and the three arrays are indexed
/// by pointer index — `getPointerId(i)`, `getX(i)`/`getY(i)` in physical
/// pixels relative to the surface, which Kotlin scales by the density.
///
/// Returns `false` when the touch lands on a platform view, so
/// `onTouchEvent` should return `false` too and let the Java view hierarchy
/// handle it.
///
/// # Safety
/// Called by JNI with valid `int[]`/`float[]` local references of equal
/// length.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeMotion(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    action: jint,
    action_index: jint,
    ids: jni::sys::jintArray,
    xs: jni::sys::jfloatArray,
    ys: jni::sys::jfloatArray,
) -> jboolean {
    let forwarded = with_env(|env| {
        let ids = read_int_array(env, ids)?;
        let xs = read_float_array(env, xs)?;
        let ys = read_float_array(env, ys)?;
        if ids.len() != xs.len() || ids.len() != ys.len() {
            return Err("pointer arrays of different lengths".to_string());
        }
        let pointers: Vec<host::Pointer> = (0..ids.len())
            .map(|i| host::Pointer {
                id: ids[i],
                x: xs[i],
                y: ys[i],
            })
            .collect();
        Ok(host::motion_event(
            action as u32,
            action_index.max(0) as usize,
            &pointers,
        ))
    });
    // `jboolean` is Rust's `bool` under jni-sys 0.4, so this is the
    // `Boolean` Kotlin reads back directly.
    match forwarded {
        Ok(handled) => handled,
        Err(error) => {
            log::warn!("rbx_android: motion event dropped: {error}");
            false
        }
    }
}

/// Deliver a key event: `action` is `0` down, `1` up, and `meta_state` is
/// the event's `getMetaState()`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeKey(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    key_code: jint,
    action: jint,
    meta_state: jint,
) {
    host::key(key_code, action, meta_state);
}

/// Forward an IME update from the `InputConnection` of the EditText proxy
/// Kotlin keeps for the software keyboard. `kind` matches the Java side:
/// `0` composing, `1` commit, `2` delete-surrounding, `3`
/// delete-in-code-points, `4` done/dismiss.
///
/// # Safety
/// Called by JNI with a valid Java `String`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeIme(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    session: jlong,
    kind: jint,
    text: jni::sys::jstring,
    start: jint,
    end: jint,
) {
    let _ = with_env(|env| {
        let text = unsafe { JObject::from_raw(env, text as jobject) };
        let text = get_string(env, &text);
        host::ime_event(
            session as u64,
            kind,
            text,
            start.max(0) as usize,
            end.max(0) as usize,
        );
        Ok(())
    });
}

/// `Activity.onResume`, after the super call.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativeResumed(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
) {
    host::resumed();
}

/// `Activity.onPause`, after the super call.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_touseefx_rbxnative_MainActivity_nativePaused(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
) {
    host::paused();
}

/// Copy a Java `int[]` out, empty when it is null.
fn read_int_array(env: &mut Env<'_>, raw: jni::sys::jintArray) -> Result<Vec<i32>, String> {
    if raw.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: `raw` is the local reference Kotlin passed; it belongs to this
    // thread's JNI frame and is only read before the entry point returns.
    let array: JPrimitiveArray<'_, jni::sys::jint> =
        unsafe { JPrimitiveArray::from_raw(env, raw as jni::sys::jarray) };
    let len = array.len(env).map_err(|e| e.to_string())?;
    let mut values = vec![0; len];
    array
        .get_region(env, 0, &mut values)
        .map_err(|e| e.to_string())?;
    Ok(values)
}

/// Copy a Java `float[]` out, empty when it is null.
fn read_float_array(env: &mut Env<'_>, raw: jni::sys::jfloatArray) -> Result<Vec<f32>, String> {
    if raw.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: as `read_int_array`.
    let array: JPrimitiveArray<'_, jni::sys::jfloat> =
        unsafe { JPrimitiveArray::from_raw(env, raw as jni::sys::jarray) };
    let len = array.len(env).map_err(|e| e.to_string())?;
    let mut values = vec![0.; len];
    array
        .get_region(env, 0, &mut values)
        .map_err(|e| e.to_string())?;
    Ok(values)
}
