//! The Open Cloud API key at rest: the OS credential store, the macOS
//! Keychain, Windows' Credential Manager, or the Secret Service (GNOME
//! Keyring, KWallet) on Linux, through GPUI's own credentials API, never a
//! plaintext file.
//!
//! What that buys, honestly: the key is encrypted on disk under the user's
//! login, so a copied disk, a backup, a synced dotfile folder or another
//! user account cannot read it. What it does not: a program already
//! running as this same user in an unlocked session can generally ask the
//! store for it (the Secret Service has no per-app access control; the
//! Keychain prompts). The wizard's IP restriction and expiration steps are
//! what make a stolen key useless elsewhere.
//!
//! The key found here is handed to `rbx_cloud::ApiKey::install`, so every
//! `Client` built afterwards, the viewer's asset fetches included, uses
//! it. `RBX_API_KEY` still wins over it, for scripted launches.

pub(crate) mod accounts;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine as _;
use gpui_kit::{AsyncApp, BackgroundExecutor, FutureExt as _};
use rbx_cloud::ApiKey;

// Every credential-store access in the editor goes through these three. When
// `RBX_NO_KEYRING` (see `rbx_cloud::keyring_disabled`) is set -- Android sets
// it, because the platform layer has no keyring there and a read that never
// answers would freeze the whole pump -- the credentials that would have gone
// to the OS store land in the file store below instead of vanishing. Nothing
// reaches the real keyring either way.

pub(crate) async fn read_credentials(
    url: &str,
    cx: &mut AsyncApp,
) -> anyhow::Result<Option<(String, Vec<u8>)>> {
    if rbx_cloud::keyring_disabled() {
        return Ok(file_read_at(&file_store_path()?, url));
    }
    cx.update(|cx| cx.read_credentials(url)).await
}

pub(crate) async fn write_credentials(
    url: &str,
    account: &str,
    secret: Vec<u8>,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    if rbx_cloud::keyring_disabled() {
        file_write_at(&file_store_path()?, url, account, secret);
        return Ok(());
    }
    cx.update(|cx| cx.write_credentials(url, account, &secret))
        .await
}

pub(crate) async fn delete_credentials(url: &str, cx: &mut AsyncApp) -> anyhow::Result<()> {
    if rbx_cloud::keyring_disabled() {
        file_delete_at(&file_store_path()?, url);
        return Ok(());
    }
    cx.update(|cx| cx.delete_credentials(url)).await
}

/// Where the file store lives: the editor's config directory, which on
/// Android is the app's private `getFilesDir()` -- no other app can read
/// it, and the OS keeps it across restarts and updates. `None` when no
/// config directory is known at all, in which case the old vanishing
/// behavior stands.
fn file_store_path() -> Option<PathBuf> {
    crate::settings::default_config_dir().map(|dir| dir.join("credentials.json"))
}

/// One stored credential: the account name and the secret, base64 so
/// bytes round-trip whatever they are.
#[derive(serde::Deserialize, serde::Serialize)]
struct FileEntry {
    account: String,
    secret: String,
}

/// Serializes the load-modify-save cycles; the calls are rare (startup,
/// sign-in, settings) and none of them await while held.
static FILE_STORE: Mutex<()> = Mutex::new(());

fn file_read_at(path: &Path, url: &str) -> Option<(String, Vec<u8>)> {
    let store = load_file_store_at(path);
    let entry = store.get(url)?;
    let secret = base64::engine::general_purpose::STANDARD
        .decode(&entry.secret)
        .ok()?;
    Some((entry.account.clone(), secret))
}

fn file_write_at(path: &Path, url: &str, account: &str, secret: Vec<u8>) {
    let _guard = FILE_STORE.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_file_store_at(path);
    store.insert(
        url.to_string(),
        FileEntry {
            account: account.to_string(),
            secret: base64::engine::general_purpose::STANDARD.encode(secret),
        },
    );
    save_file_store_at(path, &store);
}

fn file_delete_at(path: &Path, url: &str) {
    let _guard = FILE_STORE.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load_file_store_at(path);
    if store.remove(url).is_some() {
        save_file_store_at(path, &store);
    }
}

/// A credentials file that cannot be parsed reads as empty: a corrupted
/// document must not brick sign-in forever, and `forget` can still clear it.
fn load_file_store_at(path: &Path) -> BTreeMap<String, FileEntry> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            eprintln!("rbxstudio: the credentials file could not be parsed: {err}");
            BTreeMap::new()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(err) => {
            eprintln!("rbxstudio: the credentials file could not be read: {err}");
            BTreeMap::new()
        }
    }
}

fn save_file_store_at(path: &Path, store: &BTreeMap<String, FileEntry>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(store) {
        Ok(json) => {
            if let Err(err) = std::fs::write(path, json) {
                eprintln!("rbxstudio: the credentials file could not be written: {err}");
            }
        }
        Err(err) => eprintln!("rbxstudio: the credentials could not be serialized: {err}"),
    }
}

/// The store's lookup key: one entry for this editor.
const URL: &str = "https://apis.roblox.com/rbx-native";
const ACCOUNT: &str = "open-cloud-api-key";

/// How long startup waits on the credential store. A Secret Service that
/// never answers (a locked collection whose prompt cannot show, a bus with
/// no keyring behind it) would otherwise keep the splash up forever.
const RESTORE_TIMEOUT: Duration = Duration::from_secs(5);

/// Set when [`restore_at_startup`] gave up on the store, so Home can say
/// why no key was loaded instead of claiming there is none.
static STALLED: AtomicBool = AtomicBool::new(false);

/// [`restore`] with [`RESTORE_TIMEOUT`]: `None` when the store did not
/// answer in time, and the launch goes on without a key.
pub(crate) async fn restore_at_startup(cx: &mut AsyncApp) -> Option<bool> {
    let executor = cx.background_executor().clone();
    let restored = within(restore(cx), RESTORE_TIMEOUT, &executor).await;
    if restored.is_none() {
        eprintln!(
            "rbxstudio: the OS credential store did not answer within {}s; starting without the saved key",
            RESTORE_TIMEOUT.as_secs()
        );
        STALLED.store(true, Ordering::Relaxed);
    }
    restored
}

async fn within<T>(
    future: impl std::future::Future<Output = T>,
    timeout: Duration,
    executor: &BackgroundExecutor,
) -> Option<T> {
    future.with_timeout(timeout, executor).await.ok()
}

/// Whether startup gave up waiting on the credential store.
pub(crate) fn stalled() -> bool {
    STALLED.load(Ordering::Relaxed)
}

/// Loads the stored key into `rbx_cloud` at startup, moving a key still in
/// the old plaintext `api_key` file into the store first. Returns whether
/// any key is available now, `false` sends a launch to the setup wizard.
async fn restore(cx: &mut AsyncApp) -> bool {
    match read(cx).await {
        Ok(Some(key)) => {
            let known = accounts()
                .find(&accounts::fingerprint(key.expose_secret()))
                .is_some();
            if !known {
                record_active(key.expose_secret(), cx).await;
            }
            ApiKey::install(Some(key));
        }
        Ok(None) => migrate_plaintext(cx).await,
        Err(err) => {
            eprintln!("rbxstudio: the OS credential store could not be read: {err}");
            // Still usable this session, just not moved anywhere safer.
            ApiKey::install(ApiKey::from_plaintext_file());
        }
    }
    ApiKey::from_env_or_config().is_some()
}

/// Stores `key`, replacing any key already there, and makes it the one
/// every later `Client` uses. The wizard calls this only once
/// `rbx_cloud::check_scopes` has passed on it.
pub(crate) async fn save(key: ApiKey, cx: &mut AsyncApp) -> anyhow::Result<()> {
    let secret = key.expose_secret().as_bytes().to_vec();
    write_credentials(URL, ACCOUNT, secret, cx).await?;
    record_active(key.expose_secret(), cx).await;
    ApiKey::install(Some(key));
    Ok(())
}

/// Removes the stored key and the active account's slot; the next launch
/// opens the wizard again.
pub(crate) async fn forget(cx: &mut AsyncApp) -> anyhow::Result<()> {
    delete_credentials(URL, cx).await?;
    let mut index = accounts();
    if let Some(id) = index.active {
        if let Err(err) = delete_credentials(&accounts::slot_url(id), cx).await {
            eprintln!("rbxstudio: the account's keychain entry could not be deleted: {err}");
        }
        index.remove(id);
        save_index(&index);
    }
    ApiKey::install(None);
    Ok(())
}

fn index_path() -> Option<std::path::PathBuf> {
    crate::settings::default_config_dir().map(|dir| dir.join("accounts.json"))
}

fn save_index(index: &accounts::Accounts) {
    let Some(path) = index_path() else {
        return;
    };
    if let Err(err) = index.save(&path) {
        eprintln!("rbxstudio: the accounts index could not be saved: {err}");
    }
}

/// The index of stored accounts; empty when none exists or the config
/// directory is unknown.
pub(crate) fn accounts() -> accounts::Accounts {
    index_path()
        .map(|path| accounts::Accounts::load(&path))
        .unwrap_or_default()
}

/// Names the active account after a successful key check, writing the
/// index only when something changed.
pub(crate) fn label_active(owner: &str, key_name: &str) {
    let before = accounts();
    let mut index = before.clone();
    if index.label_active(owner, key_name) && index != before {
        save_index(&index);
    }
}

/// Makes `secret` the active account, adding it (and its credential-store
/// slot) when new. Failures are logged: the main key must keep working.
async fn record_active(secret: &str, cx: &mut AsyncApp) {
    let mut index = accounts();
    let (id, is_new) = index.add_active(&accounts::fingerprint(secret));
    if is_new {
        let bytes = secret.as_bytes().to_vec();
        let url = accounts::slot_url(id);
        if let Err(err) = write_credentials(&url, ACCOUNT, bytes, cx).await {
            eprintln!("rbxstudio: the account's key could not be stored: {err}");
            return;
        }
    }
    save_index(&index);
}

/// Makes a stored account's key the one in use.
pub(crate) async fn switch(id: u32, cx: &mut AsyncApp) -> anyhow::Result<()> {
    let url = accounts::slot_url(id);
    let stored = read_credentials(&url, cx).await?;
    let secret = stored
        .and_then(|(_, secret)| String::from_utf8(secret).ok())
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("That account's key is no longer in the system keychain.")
        })?;
    save(ApiKey::new(secret), cx).await?;
    let mut index = accounts();
    if index.set_active(id) {
        save_index(&index);
    }
    Ok(())
}

/// Deletes an account's stored key; removing the active one also forgets
/// the main entry.
pub(crate) async fn remove(id: u32, cx: &mut AsyncApp) -> anyhow::Result<()> {
    let url = accounts::slot_url(id);
    delete_credentials(&url, cx).await?;
    let mut index = accounts();
    let was_active = index.active == Some(id);
    index.remove(id);
    save_index(&index);
    if was_active {
        forget(cx).await?;
    }
    Ok(())
}

async fn read(cx: &mut AsyncApp) -> anyhow::Result<Option<ApiKey>> {
    let stored = read_credentials(URL, cx).await?;
    Ok(stored
        .and_then(|(_, secret)| String::from_utf8(secret).ok())
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
        .map(ApiKey::new))
}

/// Moves a plaintext `api_key` file into the store, deleting the file only
/// once the store reads the same key back. Anything short of that leaves
/// the file in place and uses it as before.
async fn migrate_plaintext(cx: &mut AsyncApp) {
    let Some(key) = ApiKey::from_plaintext_file() else {
        return;
    };
    let stored = save(key.clone(), cx).await.is_ok()
        && read(cx)
            .await
            .ok()
            .flatten()
            .is_some_and(|back| back.expose_secret() == key.expose_secret());
    let Some(path) = ApiKey::plaintext_file_path().filter(|_| stored) else {
        ApiKey::install(Some(key));
        return;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => eprintln!(
            "rbxstudio: moved the API key from {} into the OS credential store",
            path.display()
        ),
        Err(err) => eprintln!(
            "rbxstudio: the API key is now in the OS credential store, but {} \
             could not be deleted: {err}",
            path.display()
        ),
    }
}

/// The Roblox web session from the code sign-in (`shell::roblox_login`),
/// kept beside the API key under its own entry. It is a full account
/// credential, so it only ever lives in the store and in memory.
const SESSION_URL: &str = "https://www.roblox.com/rbx-native";
const SESSION_ACCOUNT: &str = "roblosecurity";

pub(crate) async fn save_session(
    cookie: &rbx_cloud::heartbeats::SessionCookie,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    let secret = cookie.expose_secret().as_bytes().to_vec();
    write_credentials(SESSION_URL, SESSION_ACCOUNT, secret, cx).await
}

pub(crate) async fn read_session(
    cx: &mut AsyncApp,
) -> anyhow::Result<Option<rbx_cloud::heartbeats::SessionCookie>> {
    let stored = read_credentials(SESSION_URL, cx).await?;
    Ok(stored
        .and_then(|(_, secret)| String::from_utf8(secret).ok())
        .filter(|secret| !secret.trim().is_empty())
        .map(|secret| rbx_cloud::heartbeats::SessionCookie::new(&secret)))
}

pub(crate) async fn forget_session(cx: &mut AsyncApp) -> anyhow::Result<()> {
    delete_credentials(SESSION_URL, cx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_store_roundtrips_and_deletes_every_credential() {
        let dir = std::env::temp_dir().join(format!("rbx-keyring-test-{}", std::process::id()));
        let path = dir.join("credentials.json");
        let _ = std::fs::remove_file(&path);

        assert_eq!(file_read_at(&path, "https://a"), None);

        file_write_at(&path, "https://a", "open-cloud-api-key", b"the-key".to_vec());
        assert_eq!(
            file_read_at(&path, "https://a"),
            Some(("open-cloud-api-key".to_string(), b"the-key".to_vec()))
        );

        // A second entry survives beside the first, and bytes that are
        // not UTF-8 round-trip too (the cookie and key are, the store
        // does not get to assume).
        file_write_at(&path, "https://b", "roblosecurity", vec![0, 255, 7]);
        assert_eq!(
            file_read_at(&path, "https://b").map(|(_, s)| s),
            Some(vec![0, 255, 7])
        );
        assert!(file_read_at(&path, "https://a").is_some());

        file_delete_at(&path, "https://a");
        assert_eq!(file_read_at(&path, "https://a"), None);
        assert!(file_read_at(&path, "https://b").is_some());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unparseable_credentials_file_reads_as_empty() {
        let dir = std::env::temp_dir().join(format!("rbx-keyring-bad-{}", std::process::id()));
        let path = dir.join("credentials.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(file_read_at(&path, "https://a"), None);
        // And writing over it heals the document.
        file_write_at(&path, "https://a", "acct", b"secret".to_vec());
        assert_eq!(
            file_read_at(&path, "https://a"),
            Some(("acct".to_string(), b"secret".to_vec()))
        );
        let _ = std::fs::remove_file(&path);
    }

    #[gpui_kit::test]
    async fn a_store_that_never_answers_is_given_up_on(cx: &mut gpui_kit::TestAppContext) {
        let executor = cx.executor();
        let hung = executor.spawn({
            let executor = executor.clone();
            async move { within(std::future::pending::<bool>(), RESTORE_TIMEOUT, &executor).await }
        });
        executor.run_until_parked();
        executor.advance_clock(RESTORE_TIMEOUT);
        assert_eq!(hung.await, None);
        assert_eq!(
            within(async { true }, RESTORE_TIMEOUT, &executor).await,
            Some(true)
        );
    }
}
