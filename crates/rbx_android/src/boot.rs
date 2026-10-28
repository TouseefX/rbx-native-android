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
/// nothing the editor saves survives a restart.
pub fn start(internal_data_path: Option<&str>) {
    if let Some(path) = internal_data_path.filter(|path| !path.is_empty()) {
        std::env::set_var("XDG_CONFIG_HOME", format!("{path}/.config"));
    }

    host::start_with_assets(gpui_kit::assets::AllAssets, |cx| {
        rbx_studio::boot_mobile(cx);
    });
}
