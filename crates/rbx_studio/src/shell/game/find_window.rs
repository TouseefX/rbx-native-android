//! Find in Game: one text search over every script of every place, grouped
//! place › script › line, with Replace's preview and its write.
//!
//! A loaded place is searched in its editor's tree; one not opened yet is
//! read from its file (once, then kept until the file changes) without
//! building a scene for it. A click on a line opens that place and that
//! script with the match selected. Replace All writes each place as one
//! undo step of that place, loading the ones not opened yet, since only an
//! open place has an undo history (and a Save) to put the change in.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use rbx_dom::{Ref, WeakDom};
use rbx_reflection::ReflectionDatabase;

use crate::tokens;

use super::super::Shell;
use super::search::{self, Preview, ScriptHits};
use super::workspace::GameWorkspace;

mod view;

/// Line rows across every place before the list stops growing.
const MAX_ROWS: usize = 2000;

/// One script's rows: its hits, and the Replace preview of each line.
pub(super) struct ScriptRows {
    pub(super) hits: ScriptHits,
    pub(super) previews: Vec<Preview>,
}

/// One place's share of the results.
pub(super) struct PlaceRows {
    pub(super) slot: usize,
    pub(super) label: String,
    /// Searched from its file: refs are that parse's, not the editor's.
    pub(super) from_file: bool,
    pub(super) scripts: Vec<ScriptRows>,
}

pub(crate) struct FindWindow {
    game: WeakEntity<GameWorkspace>,
    main: AnyWindowHandle,
    database: ReflectionDatabase,
    pub(super) query: Entity<InputState>,
    pub(super) replacement: Entity<InputState>,
    pub(super) results: Vec<PlaceRows>,
    pub(super) truncated: bool,
    /// What the last Replace All did, or why a place could not be searched.
    pub(super) notice: Option<String>,
    files: HashMap<PathBuf, (Option<SystemTime>, Rc<WeakDom>)>,
    pub(super) scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl GameWorkspace {
    /// Raises the Find in Game window, opening it if need be.
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.find_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
        }
        let Some(database) = self
            .active_shell()
            .map(|shell| shell.read(cx).database.clone())
        else {
            return;
        };
        let game = cx.weak_entity();
        let main = window.window_handle();
        let title = format!("Find in Game \u{2014} {}", self.manifest.name);
        // Deferred: the new window's first render reads this game.
        cx.defer(move |cx| {
            let opened = super::secondary_window(title, (820., 640.), cx, {
                let game = game.clone();
                move |window, cx| FindWindow::new(game, main, database, window, cx)
            });
            let _ = game.update(cx, |game, _| game.find_window = opened);
        });
    }

    /// Place `index`'s file, parsed: kept until the file changes, since a
    /// place nobody opened is searched again on every keystroke.
    fn file_dom(
        &self,
        index: usize,
        files: &mut HashMap<PathBuf, (Option<SystemTime>, Rc<WeakDom>)>,
    ) -> Result<Rc<WeakDom>, String> {
        let path = &self.slots[index].path;
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((seen, dom)) = files.get(path) {
            if *seen == modified {
                return Ok(dom.clone());
            }
        }
        let dom = Rc::new(rbx_viewer::read_place(path)?);
        files.insert(path.clone(), (modified, dom.clone()));
        Ok(dom)
    }
}

impl FindWindow {
    fn new(
        game: WeakEntity<GameWorkspace>,
        main: AnyWindowHandle,
        database: ReflectionDatabase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in every script"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
        let changed =
            |this: &mut Self, _: Entity<InputState>, event: &InputEvent, cx: &mut Context<Self>| {
                if matches!(event, InputEvent::Change) {
                    this.notice = None;
                    this.refresh(cx);
                }
            };
        let subscriptions = vec![
            cx.subscribe(&query, changed),
            cx.subscribe(&replacement, changed),
        ];
        query.update(cx, |state, cx| state.focus(window, cx));
        if let Ok(seed) = std::env::var(QUERY_VARIABLE) {
            let (find, replace) = seed.split_once('\u{1f}').unwrap_or((&seed, ""));
            query.update(cx, |state, cx| state.set_value(find.to_owned(), window, cx));
            replacement.update(cx, |state, cx| {
                state.set_value(replace.to_owned(), window, cx)
            });
        }
        let mut this = FindWindow {
            game,
            main,
            database,
            query,
            replacement,
            results: Vec::new(),
            truncated: false,
            notice: None,
            files: HashMap::new(),
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        this.refresh(cx);
        this
    }

    pub(super) fn query_text(&self, cx: &App) -> String {
        self.query.read(cx).value().to_string()
    }

    pub(super) fn replacement_text(&self, cx: &App) -> String {
        self.replacement.read(cx).value().to_string()
    }

    /// Runs the search again over every place.
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query_text(cx);
        let replacement = self.replacement_text(cx);
        self.results.clear();
        self.truncated = false;
        let Some(game) = self.game.upgrade() else {
            return;
        };
        let mut budget = MAX_ROWS;
        let mut failures = Vec::new();
        let game = game.read(cx);
        for index in 0..game.slots.len() {
            let rows = |dom: &WeakDom, budget: &mut usize| -> Vec<ScriptRows> {
                search::search(dom, &self.database, &query, budget)
                    .into_iter()
                    .map(|hits| {
                        let source = crate::script_editor::source::read(dom, hits.script)
                            .unwrap_or_default();
                        ScriptRows {
                            previews: search::preview(&source, &query, &replacement),
                            hits,
                        }
                    })
                    .collect()
            };
            let from_file = game.shell(index).is_none();
            let scripts = match game.shell(index) {
                Some(shell) => rows(&shell.read(cx).dom, &mut budget),
                None => match game.file_dom(index, &mut self.files) {
                    Ok(dom) => rows(&dom, &mut budget),
                    Err(err) => {
                        failures.push(format!("{}: {err}", game.slots[index].label));
                        continue;
                    }
                },
            };
            if !scripts.is_empty() {
                self.results.push(PlaceRows {
                    slot: index,
                    label: game.slots[index].label.clone(),
                    from_file,
                    scripts,
                });
            }
            if budget == 0 {
                self.truncated = true;
                break;
            }
        }
        if !failures.is_empty() {
            self.notice = Some(format!("Couldn\u{2019}t search {}", failures.join("; ")));
        }
        cx.notify();
    }

    /// Opens place `slot` on script number `script` of its results, with
    /// `range` selected.
    pub(super) fn reveal(
        &mut self,
        slot: usize,
        script: usize,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let Some(place) = self.results.iter().find(|place| place.slot == slot) else {
            return;
        };
        let (reference, from_file) = (place.scripts[script].hits.script, place.from_file);
        let query = self.query_text(cx);
        let database = self.database.clone();
        let game = self.game.clone();
        let opened = self.main.update(cx, |_, window, cx| {
            window.activate_window();
            game.update(cx, |game, cx| {
                game.activate(slot, window, cx);
                let shell = game.shell(slot)?.clone();
                shell.update(cx, |shell, cx| {
                    // Refs from a file's parse are not the editor's: the same
                    // walk over the same file finds the same script by order.
                    let reference = if from_file {
                        let mut budget = usize::MAX;
                        search::search(&shell.dom, &database, &query, &mut budget)
                            .get(script)?
                            .script
                    } else {
                        reference
                    };
                    shell.reveal_script(reference, range, window, cx);
                    Some(())
                })
            })
        });
        if matches!(opened, Ok(Ok(Some(())))) && from_file {
            self.refresh(cx);
        }
    }

    /// Replace All: every place with a match, one undo step each.
    pub(super) fn replace_all(&mut self, cx: &mut Context<Self>) {
        let query = self.query_text(cx);
        let replacement = self.replacement_text(cx);
        if query.is_empty() {
            return;
        }
        let slots: Vec<usize> = self.results.iter().map(|place| place.slot).collect();
        let game = self.game.clone();
        let outcome = self.main.update(cx, |_, window, cx| {
            game.update(cx, |game, cx| {
                let mut scripts = 0;
                let mut places = 0;
                let mut failures = Vec::new();
                for slot in slots {
                    match game.load(slot, window, cx) {
                        Ok(shell) => {
                            let changed = shell.update(cx, |shell, cx| {
                                shell.replace_in_scripts(&query, &replacement, window, cx)
                            });
                            scripts += changed;
                            places += usize::from(changed > 0);
                        }
                        Err(err) => failures.push(err),
                    }
                }
                cx.notify();
                (scripts, places, failures)
            })
        });
        self.notice = Some(match outcome {
            Ok(Ok((scripts, places, failures))) => {
                let mut line = format!(
                    "Replaced in {scripts} script{} across {places} place{}. Ctrl+Z in a place undoes its part.",
                    if scripts == 1 { "" } else { "s" },
                    if places == 1 { "" } else { "s" },
                );
                if !failures.is_empty() {
                    line.push_str(&format!(" Couldn\u{2019}t open {}", failures.join("; ")));
                }
                line
            }
            _ => "The game window is gone.".to_owned(),
        });
        self.refresh(cx);
    }
}

/// `RBX_STUDIO_FIND_IN_GAME=<find>[\x1f<replace>]`: opens Find in Game
/// seeded with that query once the game window is up, for captures.
pub(crate) const QUERY_VARIABLE: &str = "RBX_STUDIO_FIND_IN_GAME";

impl Shell {
    /// Opens `reference` in the script editor with `range` selected.
    fn reveal_script(
        &mut self,
        reference: Ref,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_place_script(reference, window, cx);
        if let Some(open) = self.scripts.open.get(&reference) {
            open.state.update(cx, |state, cx| {
                state.set_selected_range(range, cx);
                state.focus(window, cx);
            });
        }
    }

    /// Replaces `query` by `replacement` in every script of this place as one
    /// undo step; returns how many scripts changed.
    fn replace_in_scripts(
        &mut self,
        query: &str,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        // Typing still on the debounce is part of what is being replaced.
        self.flush_script_edits(cx);
        self.dom.take_changes();
        let before = self.dom.clone();
        let changed = search::replace(&mut self.dom, &self.database, query, replacement);
        if changed.is_empty() {
            return 0;
        }
        self.push_history_snapshot(before);
        self.properties.dom_changed(&[]);
        let changes = self.dom.take_changes();
        self.record_history_change(changes);
        self.resync_scripts(window, cx);
        cx.notify();
        changed.len()
    }
}

impl Render for FindWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::component::v_flex()
            .id("find-in-game")
            .size_full()
            .bg(tokens::dock())
            .font_family(tokens::font_ui())
            .text_color(tokens::text())
            .child(super::super::chrome::window_topbar(
                "Find in Game".into(),
                true,
                |window, cx| window.defer(cx, |window, _| window.remove_window()),
            ))
            .child(self.controls(cx))
            .child(self.results_view(cx))
    }
}
