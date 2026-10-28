//! What `GameActivity.onCreate` starts, and where the editor's state lives
//! on Android.

use gpui_mobile::android::host;

/// Start the editor's render thread. Called once from the GameActivity glue
/// (`initializeNativeCode`, see `game_activity`) on the Java UI thread; the
/// launch closure below runs on the render thread once the first surface
/// arrives, with the GPUI application already built and its window already
/// open over that surface — the host owns the window, so the editor never
/// opens one itself.
///
/// `internal_data_path` is the Activity's `getFilesDir()`, the app's private
/// storage: it becomes `XDG_CONFIG_HOME`, because `rbx_studio`'s settings,
/// Recent list and packs resolve their directory through it (see
/// `rbx_studio::settings::default_config_dir`) and Android sets no `HOME`
/// for the process. Without it those all fall back to their defaults and
/// nothing the editor saves survives a restart. `cache_path` is the
/// Activity's `getCacheDir()`: `rbx_assets`' asset cache — every download
/// the editor makes — resolves to `$XDG_CACHE_HOME/rbx-native` and answers
/// `NoCacheDir` without it, so nothing downloads at all.
pub fn start(internal_data_path: Option<&str>, cache_path: Option<&str>) {
    let files = internal_data_path.filter(|path| !path.is_empty());
    if let Some(path) = files {
        std::env::set_var("XDG_CONFIG_HOME", format!("{path}/.config"));
        std::env::set_var("XDG_DATA_HOME", format!("{path}/.local/share"));
    }
    // Prefer the system cache dir — Android can clear it when storage runs
    // low — falling back to a folder beside the config one when the
    // Activity could not be asked.
    let cache = match cache_path.filter(|path| !path.is_empty()) {
        Some(cache) => Some(cache.to_string()),
        None => files.map(|path| format!("{path}/cache")),
    };
    match cache {
        Some(cache) => std::env::set_var("XDG_CACHE_HOME", cache),
        None => log::warn!(
            "rbx_android: no storage paths from the Activity — downloads \
             will have nowhere to go"
        ),
    }
    // Android has no OS keyring behind GPUI's credentials API, and the
    // platform read never answers there: Home's startup `load_session`
    // awaits it with no timeout, on the render thread, inside the task
    // queue the host loop is draining — one call and the whole pump is
    // frozen (no frames, no input, ever again). The desktop boot wraps
    // the same read in a five-second timeout for exactly this failure;
    // the session path has none. `RBX_NO_KEYRING` is the switch
    // `rbx_cloud` already provides for keyring-less environments: every
    // keyring read and write then answers "empty" instantly and Home
    // stays interactive. Revisit when the platform layer grows a real
    // Android Keystore delegate.
    std::env::set_var("RBX_NO_KEYRING", "1");

    host::start_with_assets(gpui_kit::assets::AllAssets, |cx| {
        rbx_studio::boot_mobile(cx);
        // The host loop presents only the frames GPUI asks for, and the
        // first one needs asking for from outside the launch closure:
        // `App::open_window`'s initial draw builds the scene without
        // presenting it and leaves the window clean, so the frame its
        // birth-dirty waker scheduled arrives to nothing and never re-arms —
        // a black screen forever after. (The android-activity entry point
        // kicks a frame right after its launch callback; the host-driven one
        // has no such kick.) `AsyncApp::refresh` is the purpose-built ask:
        // it runs inside an update scope — a bare `refresh_windows` out here
        // would queue an effect nothing flushes, as the host's launch
        // closure runs without an App update scope — and the task lands on
        // the foreground executor, which the host loop drains the moment the
        // boot returns.
        cx.spawn(async move |cx| {
            // rbx-native diagnostics (temporary): proves the host loop
            // drains the main-thread queue at all.
            log::info!("rbx_android: kick task is polling");
            cx.refresh();
            log::info!("rbx_android: first-frame refresh asked for");
        })
        .detach();
    });
}
