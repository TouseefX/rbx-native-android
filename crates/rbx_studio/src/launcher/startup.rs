//! The launch-time credential check.

use gpui_kit::{App, AsyncApp};
use rbx_cloud::expiry::{check_session, key_health, Health};

use super::setup::Cred;

fn from_env(name: &str) -> Cred {
    match std::env::var(name).as_deref() {
        Ok("missing") => Cred::Missing,
        Ok("expired") => Cred::Expired,
        _ => Cred::Present,
    }
}

fn from_health(health: Health) -> Cred {
    match health {
        Health::Expired => Cred::Expired,
        Health::Valid | Health::Unknown => Cred::Present,
    }
}

/// The key and session states of this launch. The capture env hooks stand in
/// for the keyring and the network when set. A credential Roblox refuses is
/// deleted so it is not tried again.
pub(crate) async fn credentials(has_key: bool, cx: &mut AsyncApp) -> (Cred, Cred) {
    if std::env::var_os("RBX_STUDIO_LAUNCHER_KEYSTATE").is_some()
        || std::env::var_os("RBX_STUDIO_LAUNCHER_SESSION").is_some()
    {
        return (
            from_env("RBX_STUDIO_LAUNCHER_KEYSTATE"),
            from_env("RBX_STUDIO_LAUNCHER_SESSION"),
        );
    }
    let cookie = crate::key_store::read_session(cx).await.ok().flatten();
    let (key_result, session_result) = cx
        .background_executor()
        .spawn(async move {
            let key = has_key.then(|| {
                let client = rbx_cloud::Client::new(rbx_cloud::ApiKey::from_env_or_config());
                key_health(&client.introspect())
            });
            let session = cookie.as_ref().map(check_session);
            (key, session)
        })
        .await;
    let key = key_result.map_or(Cred::Missing, from_health);
    let session = session_result.map_or(Cred::Missing, from_health);
    if key == Cred::Expired {
        if let Err(err) = crate::key_store::forget(cx).await {
            eprintln!("rbxstudio: the expired API key could not be removed: {err}");
        }
    }
    if session == Cred::Expired {
        if let Err(err) = crate::key_store::forget_session(cx).await {
            eprintln!("rbxstudio: the expired sign-in could not be removed: {err}");
        }
    }
    (key, session)
}

/// The same check with the editor already open: an expired credential is
/// reported so the editor shows its notice.
pub(crate) fn check_in_background(has_key: bool, cx: &mut App) {
    cx.spawn(async move |cx| {
        let (key, session) = credentials(has_key, cx).await;
        if key == Cred::Expired {
            rbx_cloud::expiry::report_key_refused();
        }
        if session == Cred::Expired {
            rbx_cloud::expiry::report_session_refused();
        }
    })
    .detach();
}
