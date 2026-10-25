//! Recent's per-place menu, the one Studio's start page has: Configure
//! Experience, Configure Place, Open Place page, Archive, Remove from list;
//! and the list/icons switch.

use gpui_kit::*;
use rbx_cloud::{ApiKey, Client};

use super::{Dialog, HomeWindow};
use crate::cli::Configure;
use crate::home::{self, RecentPlace};
use crate::settings::Settings;

/// The menu, while it is up, and where it was opened.
pub(crate) struct RecentMenu {
    pub(super) place: RecentPlace,
    pub(super) at: Point<Pixels>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecentAction {
    ConfigureExperience,
    ConfigurePlace,
    OpenPage,
    Archive,
    Remove,
}

impl RecentAction {
    pub(super) const ALL: [RecentAction; 5] = [
        RecentAction::ConfigureExperience,
        RecentAction::ConfigurePlace,
        RecentAction::OpenPage,
        RecentAction::Archive,
        RecentAction::Remove,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            RecentAction::ConfigureExperience => "Configure Experience",
            RecentAction::ConfigurePlace => "Configure Place",
            RecentAction::OpenPage => "Open Place page",
            RecentAction::Archive => "Archive",
            RecentAction::Remove => "Remove from list",
        }
    }

    /// Why the entry is greyed out for `place`, or `None` when it works.
    /// Everything but Remove is about the Roblox place, which a file that
    /// was never linked doesn't have.
    pub(super) fn unavailable(self, place: &RecentPlace, has_key: bool) -> Option<&'static str> {
        let linked = place.universe_id.is_some() && place.place_id.is_some();
        match self {
            RecentAction::Remove => None,
            _ if !linked => Some("Local only: this file isn't linked to a Roblox place"),
            RecentAction::Archive if !has_key => Some("Archiving needs an API key"),
            _ => None,
        }
    }
}

/// `RBX_STUDIO_LAUNCHER_ARCHIVE=ok|<reason>` answers Archive without
/// calling Roblox, for captures and for trying the flow on a real game
/// without archiving it.
const ARCHIVE_VARIABLE: &str = "RBX_STUDIO_LAUNCHER_ARCHIVE";

/// `RBX_STUDIO_LAUNCHER_RECENT=list|icons`, plus `@menu` or `@archive` for
/// the first place's menu or Archive dialog: Recent's opening state for a
/// capture. Nothing is saved.
const RECENT_VARIABLE: &str = "RBX_STUDIO_LAUNCHER_RECENT";

impl HomeWindow {
    pub(super) fn set_recent_icons(&mut self, on: bool, cx: &mut Context<Self>) {
        self.recent_icons = on;
        // The editor saves the settings it booted with, so its copy has to agree.
        if let Some(boot) = self.boot.borrow_mut().as_mut() {
            boot.settings.recent_icons = on;
        }
        let mut settings = Settings::load();
        settings.recent_icons = on;
        if let Err(err) = settings.save() {
            eprintln!("rbxstudio: could not save the Recent view: {err}");
        }
        cx.notify();
    }

    pub(super) fn open_recent_menu(
        &mut self,
        place: RecentPlace,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.recent_menu = Some(RecentMenu { place, at });
        cx.notify();
    }

    pub(super) fn run_recent_action(&mut self, action: RecentAction, cx: &mut Context<Self>) {
        let Some(RecentMenu { place, .. }) = self.recent_menu.take() else {
            return;
        };
        cx.notify();
        if action.unavailable(&place, self.has_key()).is_some() {
            return;
        }
        match action {
            RecentAction::ConfigureExperience => self.configure(&place, Configure::Experience, cx),
            RecentAction::ConfigurePlace => self.configure(&place, Configure::Place, cx),
            RecentAction::OpenPage => {
                if let Some(id) = place.place_id {
                    cx.open_url(&format!("https://www.roblox.com/games/{id}"));
                }
            }
            RecentAction::Archive => {
                self.dialog = Some(Dialog::Archive {
                    place,
                    busy: false,
                    error: None,
                });
            }
            RecentAction::Remove => self.forget_recent(&place),
        }
    }

    /// Game Settings lives in the editor, where the place file's own
    /// settings are, so this opens the place and the dialog on top of it.
    fn configure(&mut self, place: &RecentPlace, page: Configure, cx: &mut Context<Self>) {
        if let Some(boot) = self.boot.borrow_mut().as_mut() {
            boot.launch.configure = Some(page);
        }
        self.open_recent(place, cx);
    }

    fn forget_recent(&mut self, place: &RecentPlace) {
        match home::forget(&place.path) {
            Ok(()) => self.recent.retain(|p| p.path != place.path),
            Err(err) => {
                self.dialog = Some(Dialog::Error {
                    experience: None,
                    title: "Couldn\u{2019}t update Recent".to_string(),
                    status: None,
                    reason: err.to_string(),
                })
            }
        }
    }

    pub(super) fn confirm_archive(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Archive { place, busy, error }) = &mut self.dialog else {
            return;
        };
        let Some(universe_id) = place.universe_id else {
            return;
        };
        let mock = std::env::var(ARCHIVE_VARIABLE).ok();
        let key = ApiKey::from_env_or_config();
        if mock.is_none() && key.is_none() {
            *error = Some("No API key is stored.".to_string());
            cx.notify();
            return;
        }
        *busy = true;
        *error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match mock.as_deref() {
                        Some("ok") => Ok(()),
                        Some(reason) => Err(reason.to_string()),
                        None => Client::new(key)
                            .archive_experience(universe_id)
                            .map_err(|err| err.to_string()),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(Dialog::Archive { busy, error, .. }) = &mut this.dialog else {
                    return;
                };
                match result {
                    Ok(()) => this.dialog = None,
                    Err(reason) => {
                        *busy = false;
                        *error = Some(reason);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn apply_recent_capture(&mut self, window: &Window) {
        let Ok(value) = std::env::var(RECENT_VARIABLE) else {
            return;
        };
        let (view, then) = value.split_once('@').unwrap_or((&value, ""));
        self.recent_icons = view == "icons";
        let Some(place) = self.recent.first().cloned() else {
            return;
        };
        match then {
            "menu" => {
                let size = window.viewport_size();
                self.recent_menu = Some(RecentMenu {
                    place,
                    at: point(size.width * 0.45, size.height * 0.5),
                });
            }
            "archive" => {
                self.dialog = Some(Dialog::Archive {
                    place,
                    busy: false,
                    error: None,
                })
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's own `test` attribute would come along with it.
    use super::{RecentAction, RecentPlace};

    fn place(linked: bool) -> RecentPlace {
        RecentPlace {
            path: "a.rbxl".into(),
            universe_id: linked.then_some(1),
            place_id: linked.then_some(2),
            name: None,
            opened: None,
        }
    }

    #[test]
    fn a_local_file_can_only_be_removed() {
        let local = place(false);
        for action in RecentAction::ALL {
            let off = action.unavailable(&local, true).is_some();
            assert_eq!(off, action != RecentAction::Remove, "{action:?}");
        }
    }

    #[test]
    fn a_linked_place_needs_a_key_only_to_archive() {
        let linked = place(true);
        for action in RecentAction::ALL {
            assert_eq!(action.unavailable(&linked, true), None, "{action:?}");
            let off = action.unavailable(&linked, false).is_some();
            assert_eq!(off, action == RecentAction::Archive, "{action:?}");
        }
    }
}
