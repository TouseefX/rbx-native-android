//! Which linked games are archived: Recent's menu offers Restore instead
//! of Archive for those, and both pages mark them. Public data, so asked
//! with or without an API key, and kept in the games cache when there is one.

use gpui_kit::*;

use super::HomeWindow;
use crate::home::RecentPlace;

impl HomeWindow {
    pub(super) fn refresh_archived(&mut self, mut ids: Vec<u64>, cx: &mut Context<Self>) {
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(archived) = cx
                .background_spawn(async move { rbx_cloud::archived_experiences(&ids) })
                .await
            else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.cache.archived.extend(archived);
                this.save_cache();
                cx.notify();
            });
        })
        .detach();
    }

    /// After Archive or Restore went through, so the row changes at once.
    pub(super) fn set_archived(&mut self, universe_id: u64, archived: bool) {
        self.cache.archived.insert(universe_id, archived);
        self.save_cache();
    }

    fn save_cache(&self) {
        if let Some(user_id) = self.user_id {
            self.cache.save(user_id);
        }
    }

    pub(super) fn is_archived(&self, universe_id: u64) -> bool {
        self.cache.archived.get(&universe_id) == Some(&true)
    }

    pub(super) fn place_archived(&self, place: &RecentPlace) -> bool {
        place.universe_id.is_some_and(|id| self.is_archived(id))
    }
}
