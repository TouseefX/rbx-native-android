//! The JNI surface of the `androidx.games:games-activity` AAR's
//! `GameActivity` base class — the native methods it declares and calls
//! itself, which must all exist for the class to link when it loads
//! `librbx_android.so` in `onCreate`.
//!
//! Only one of them does work: `initializeNativeCode`, which every
//! `GameActivity.onCreate` — an Activity recreation included — calls first.
//! It registers the Activity with `gpui-pre-mobile` (the IME and the
//! platform's Java callbacks find it through that registry) and starts the
//! host-driven render thread with the editor's mobile boot, once per
//! process; on a recreation the thread, the GPUI application and the
//! editor's state all survive, and the new surface re-attaches to them (see
//! `gpui_mobile::android::host`).
//!
//! The rest are no-ops: input and surfaces are driven from Kotlin, through
//! [`super::bridge`], in the currency the host speaks, rather than through
//! the GameActivity glue `android-activity`-based apps use. GameActivity
//! still calls its own `onTouchEventNative`/`onSurfaceCreatedNative`/… —
//! `MainActivity` does not reach them, and were something to, returning
//! `false`/doing nothing is inert.

// As `bridge`: the module's functions are imported directly so the `jni`
// crate name stays free for the types the signatures below use.
use gpui_mobile::android::jni::{get_string, install_panic_hook, set_host_activity, with_env};
use jni::objects::JObject;
use jni::sys::{jboolean, jlong, jobject};

/// `initializeNativeCode(String internalDataPath, String obbPath, String
/// externalDataPath, AssetManager assetMgr, byte[] savedState, Configuration
/// config) -> long`, called from `GameActivity.onCreate` before anything
/// else — every time the Activity is (re)created.
///
/// Returns a non-zero handle (ours is `1`); GameActivity treats `0` as
/// failure and reads `getDlError` for the reason, which we never give it.
///
/// # Safety
/// Called by JNI, with `thiz` the `MainActivity` being created and valid
/// local references for the parameters.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_initializeNativeCode(
    _env: *mut jni::sys::JNIEnv,
    thiz: jobject,
    internal_data_path: jni::sys::jstring,
    _obb_path: jni::sys::jstring,
    _external_data_path: jni::sys::jstring,
    _asset_manager: jobject,
    _saved_state: jni::sys::jbyteArray,
    _config: jobject,
) -> jlong {
    // Rust's `log` → logcat, once per process, before the render thread
    // starts saying anything.
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("rbxstudio")
            .with_max_level(log::LevelFilter::Info),
    );
    // A panic on the render thread would abort the process through the
    // platform's hooks; install them while the JVM is reachable.
    install_panic_hook();

    let path = match with_env(|env| {
        let path = unsafe { JObject::from_raw(env, internal_data_path as jobject) };
        let path = get_string(env, &path);
        // `thiz` is the live Activity: the platform's IME (its
        // `gpuiShowKeyboard` & co.) and Java-side helpers find it through
        // this registry, and a recreated Activity replaces the entry.
        let activity = unsafe { JObject::from_raw(env, thiz as jobject) };
        set_host_activity(env, &activity)?;
        Ok(path)
    }) {
        Ok(path) => path,
        Err(error) => {
            log::error!("rbx_android: could not register the Activity: {error}");
            return 0;
        }
    };

    // The render thread, the GPUI application and the editor's state: once
    // per process, on the first Activity; later calls are no-ops inside.
    super::boot::start(if path.is_empty() {
        None
    } else {
        Some(path.as_str())
    });
    1
}

/// `getDlError() -> String`. Never called — `initializeNativeCode` never
/// reports failure — but declared by the base class.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_getDlError(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
) -> jni::sys::jstring {
    std::ptr::null_mut()
}

/// `terminateNativeCode(long)`, from `GameActivity.onDestroy`. The render
/// thread outlives every Activity by design — a recreation re-attaches its
/// surface to the GPUI window that survived — so there is nothing to
/// terminate; the process going away is what stops the editor.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_terminateNativeCode(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onStartNative(long)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onStartNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onResumeNative(long)`. Resume reaches the host from Kotlin
/// (`MainActivity.onResume` → `nativeResumed`), so this stays inert.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onResumeNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onSaveInstanceStateNative(long) -> byte[]`. The editor keeps no Java-side
/// state; its own persistence is the config directory under
/// `getFilesDir()`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSaveInstanceStateNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) -> jni::sys::jbyteArray {
    std::ptr::null_mut()
}

/// `onPauseNative(long)`. Pausing reaches the host from Kotlin
/// (`MainActivity.onPause` → `nativePaused`).
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onPauseNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onStopNative(long)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onStopNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onConfigurationChangedNative(long, Configuration)`. Density changes
/// arrive with the new surface, which the host already scales by.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onConfigurationChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _config: jobject,
) {
}

/// `onTrimMemoryNative(long, int)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onTrimMemoryNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _level: jni::sys::jint,
) {
}

/// `onWindowFocusChangedNative(long, boolean)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onWindowFocusChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _focused: jboolean,
) {
}

/// `onSurfaceCreatedNative(long, Surface)`. The surface reaches the host
/// from Kotlin's own `SurfaceHolder.Callback` (`nativeSurfaceCreated`),
/// which runs beside GameActivity's, so this stays inert.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSurfaceCreatedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _surface: jobject,
) {
}

/// `onSurfaceChangedNative(long, Surface, int, int, int)`. As above.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSurfaceChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _surface: jobject,
    _format: jni::sys::jint,
    _width: jni::sys::jint,
    _height: jni::sys::jint,
) {
}

/// `onSurfaceRedrawNeededNative(long, Surface)`. GPUI redraws on its own
/// frame clock.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSurfaceRedrawNeededNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _surface: jobject,
) {
}

/// `onSurfaceDestroyedNative(long)`. The blocking hand-back runs in Kotlin's
/// callback (`nativeSurfaceDestroyed`).
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSurfaceDestroyedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onTouchEventNative(long, MotionEvent, …) -> boolean`. Touch reaches the
/// host from Kotlin (`nativeMotion`), which intercepts before the base
/// class sees the event; if the base class ever called this itself,
/// returning `false` merely lets its default handling run.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onTouchEventNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _motion_event: jobject,
    _pointer_count: jni::sys::jint,
    _history_size: jni::sys::jint,
    _device_id: jni::sys::jint,
    _source: jni::sys::jint,
    _action: jni::sys::jint,
    _event_time: jlong,
    _down_time: jlong,
    _flags: jni::sys::jint,
    _meta_state: jni::sys::jint,
    _action_button: jni::sys::jint,
    _button_state: jni::sys::jint,
    _classification: jni::sys::jint,
    _edge_flags: jni::sys::jint,
    _precision_x: jni::sys::jfloat,
    _precision_y: jni::sys::jfloat,
) -> jboolean {
    false
}

/// `onKeyDownNative(long, KeyEvent) -> boolean`. Keys reach the host from
/// Kotlin (`nativeKey`), after the super call so the system's own handling
/// (Back, volume) still runs.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onKeyDownNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _key_event: jobject,
) -> jboolean {
    false
}

/// `onKeyUpNative(long, KeyEvent) -> boolean`. As above.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onKeyUpNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _key_event: jobject,
) -> jboolean {
    false
}

/// `onTextInputEventNative(long, State)`. Text input is the EditText proxy
/// Kotlin runs (`nativeIme`), not GameTextInput.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onTextInputEventNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _soft_keyboard_event: jobject,
) {
}

/// `setInputConnectionNative(long, InputConnection)`. See
/// `onTextInputEventNative`: the proxy's connection is the one the IME
/// talks to.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_setInputConnectionNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _input_connection: jobject,
) {
}

/// `onWindowInsetsChangedNative(long)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onWindowInsetsChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
) {
}

/// `onContentRectChangedNative(long, int, int, int, int)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onContentRectChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _x: jni::sys::jint,
    _y: jni::sys::jint,
    _w: jni::sys::jint,
    _h: jni::sys::jint,
) {
}

/// `onSoftwareKeyboardVisibilityChangedNative(long, boolean)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onSoftwareKeyboardVisibilityChangedNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _visible: jboolean,
) {
}

/// `onEditorActionNative(long, int)`.
///
/// # Safety
/// Called by JNI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_google_androidgamesdk_GameActivity_onEditorActionNative(
    _env: *mut jni::sys::JNIEnv,
    _thiz: jobject,
    _handle: jlong,
    _action: jni::sys::jint,
) {
}

/// The one symbol `android-activity`'s native-activity glue — compiled into
/// `gpui-pre-mobile`, though the host-driven path this crate drives never
/// runs it — requires of the application: Android's `dlopen` refuses a
/// library that leaves it unresolved, so `MainActivity`'s
/// `System.loadLibrary` died with an `UnsatisfiedLinkError` before this
/// existed. Nothing ever calls it — GameActivity is not a `NativeActivity`,
/// and the editor's real entry points are the JNI functions above and in
/// [`super::bridge`] — so it returns immediately.
#[unsafe(no_mangle)]
fn android_main(_app: android_activity::AndroidApp) {}
