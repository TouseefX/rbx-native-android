//! Undo/redo: a bounded stack of whole-DOM snapshots, taken right before each
//! mutation (see `shell::history`, which calls [`History::push`] at every
//! call site `shell::command`, `shell::edit` and `shell::keys` already use to
//! take `Shell::dom` out, mutate it, and put it back), each paired with the
//! `Change` log that mutation went on to produce (see [`History::record_changes`]).
//!
//! Keeping a full [`WeakDom`] clone per entry rather than a diff is the same
//! trade `shell::command` already makes for a script run: a diff format would
//! have to track every mutation path (script, property edit, insert, delete)
//! separately instead of once, here. The `Change` log carried alongside each
//! snapshot is that same log, not a second one: every mutation path already
//! writes to `WeakDom`'s own change log (`shell::command`'s `reflect_changes`
//! hands it to the viewport), so recording it here costs nothing beyond
//! draining it at the right two moments.
//!
//! Whole-place clones are only cheap while the place is small or the machine
//! is roomy, and neither holds on a phone: fifty snapshots of a joined game
//! is gigabytes, and the kernel's low-memory killer ends the process with no
//! panic and no dialog — the script editor's silent crash on the device. So
//! the stack is bounded twice over: by steps ([`DEFAULT_CAP`]) and by bytes
//! ([`RETAINED_BUDGET`], an over-estimate via `WeakDom::retained_bytes`), and
//! script typing, the one mutation path that fires per pause in typing,
//! coalesces into one step per streak (see [`History::continues_streak`])
//! instead of minting a snapshot each.

use std::time::{Duration, Instant};

use gpui_kit::Modifiers;
use rbx_dom::{Change, Ref, WeakDom};

/// How many undo steps are kept before the oldest is dropped, bounds memory
/// rather than growing the stack for the length of a whole editing session.
pub(crate) const DEFAULT_CAP: usize = 50;

/// How much each stack may retain, in `WeakDom::retained_bytes`'s estimate.
/// A step is always kept no matter what it cost, so undo of the last action
/// exists; past that, the oldest snapshots go first once the bytes are over.
/// Small places never reach it and keep their full [`DEFAULT_CAP`] steps;
/// large ones trade depth for the process staying alive.
const RETAINED_BUDGET: usize = 128 * 1024 * 1024;

/// What Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z do, the window-level shortcuts this
/// module owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Undo,
    Redo,
}

/// Maps one keystroke to an undo/redo action. Mirrors `save::action_for`'s
/// pure-function shape: the mapping is testable without a window, and
/// `shell::history` only has to call it. Both `Ctrl+Y` and `Ctrl+Shift+Z` redo
/// Studio uses the former, most other editors the latter.
pub(crate) fn action_for(key: &str, modifiers: Modifiers) -> Option<Action> {
    let keystroke = gpui_kit::Keystroke {
        key: key.into(),
        modifiers,
        key_char: None,
    };
    if crate::shortcuts::pressed(crate::shortcuts::Command::Redo, &keystroke) {
        Some(Action::Redo)
    } else if crate::shortcuts::pressed(crate::shortcuts::Command::Undo, &keystroke) {
        Some(Action::Undo)
    } else {
        None
    }
}

/// One snapshot on either stack: the DOM as it stood at that point, plus the
/// `Change` log the mutation right after it went on to produce. Handed by
/// `shell::history::install` to `shell::command`'s `reflect_changes`: the
/// same path an ordinary edit's viewport reflection takes, so the viewport
/// patches exactly the instances the step touched, without re-diffing two
/// `WeakDom` trees to find them.
///
/// `changes` starts empty and is filled in once, after the fact, by
/// [`History::record_changes`], `push` runs *before* the mutation it
/// snapshots, so the log it produces isn't known yet. An entry whose
/// mutation never actually changed anything (a Properties-panel commit that
/// failed validation, say) simply keeps the empty log `push` left it with,
/// and reflecting an empty log is a no-op.
struct Entry {
    dom: WeakDom,
    changes: Vec<Change>,
    /// When the entry was last written to: pushed, or continued by
    /// [`History::record_continued_change`]. Read by
    /// [`History::continues_streak`] to decide whether a script's next
    /// debounced write still belongs to this step.
    at: Instant,
    /// `dom.retained_bytes()`, taken once at construction so the budget
    /// never re-walks a tree to know what it is holding.
    bytes: usize,
}

/// A bounded stack of DOM snapshots either side of the current state: `undo`
/// holds what came before, `redo` holds what an undo just stepped back from.
pub(crate) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    cap: usize,
    /// See [`RETAINED_BUDGET`]; the bytes each stack currently holds.
    budget: usize,
    undo_bytes: usize,
    redo_bytes: usize,
    /// Moves on every push, undo and redo, every way the DOM changes goes
    /// through one of the three, so a reader that only needs to know
    /// *whether* the tree moved since it last looked (`luau-lsp`'s copy of
    /// the place) can compare one number instead of two trees.
    revision: u64,
}

impl History {
    pub(crate) fn new(cap: usize) -> Self {
        History::with_limits(cap, RETAINED_BUDGET)
    }

    /// [`History::new`] with the budget the tests can reach: real places
    /// would need gigabytes to cross [`RETAINED_BUDGET`], and the tests have
    /// neither the time nor the memory to build one.
    pub(crate) fn with_limits(cap: usize, budget: usize) -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            cap,
            budget,
            undo_bytes: 0,
            redo_bytes: 0,
            revision: 0,
        }
    }

    /// Snapshots `dom` onto the undo stack right before a mutation is
    /// applied to it, and clears the redo stack: a new action invalidates
    /// whatever could have been redone. Drops the oldest snapshot once the
    /// stack exceeds `cap` steps or the budget's bytes, so the stack never
    /// grows past either.
    pub(crate) fn push(&mut self, dom: WeakDom) {
        let bytes = dom.retained_bytes();
        self.undo_bytes += bytes;
        self.undo.push(Entry {
            dom,
            changes: Vec::new(),
            at: Instant::now(),
            bytes,
        });
        self.redo.clear();
        self.redo_bytes = 0;
        self.trim();
        self.revision += 1;
    }

    /// Drops the oldest undo snapshots over the limits. One entry always
    /// stays: whatever a step cost, undo of the last action must exist.
    fn trim(&mut self) {
        while self.undo.len() > 1 && (self.undo.len() > self.cap || self.undo_bytes > self.budget) {
            self.undo_bytes -= self.undo[0].bytes;
            self.undo.remove(0);
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Attaches `changes` to the entry `push` most recently added, see
    /// [`Entry`]'s doc comment. A no-op if nothing has been pushed yet
    /// (should not happen given `Shell::push_history`'s own call sites, but
    /// costs nothing to guard).
    pub(crate) fn record_changes(&mut self, changes: Vec<Change>) {
        if let Some(entry) = self.undo.last_mut() {
            entry.changes = changes;
        }
    }

    /// [`History::record_changes`] for a write that continued the top
    /// entry's step rather than starting one of its own (a typing streak,
    /// see [`History::continues_streak`]): the log is re-recorded against
    /// the snapshot already there, the tree as it stood before the streak
    /// began, and `revision` moves, because the tree changed without a
    /// `push`.
    pub(crate) fn record_continued_change(&mut self, changes: Vec<Change>) {
        if let Some(entry) = self.undo.last_mut() {
            entry.changes = changes;
            entry.at = Instant::now();
        }
        self.revision += 1;
    }

    /// Whether the entry on top of the undo stack is `script`'s typing
    /// streak: its whole log is one write of `property` on `script`, and the
    /// last write to it was within `within`. `shell::scripts`'s debounced
    /// write asks before taking a snapshot: a commit that continues the
    /// streak re-records the log against the snapshot already there instead
    /// of minting another whole-place copy, which is what typing a pause at
    /// a time used to do to the memory of a phone-sized machine.
    pub(crate) fn continues_streak(&self, script: Ref, property: &str, within: Duration) -> bool {
        self.continues_streak_at(script, property, within, Instant::now())
    }

    /// [`History::continues_streak`] with the clock handed in, so a test can
    /// let a streak expire without sleeping.
    fn continues_streak_at(
        &self,
        script: Ref,
        property: &str,
        within: Duration,
        now: Instant,
    ) -> bool {
        let Some(entry) = self.undo.last() else {
            return false;
        };
        now.duration_since(entry.at) <= within
            && matches!(
                entry.changes.as_slice(),
                [Change::Property { referent, name }] if *referent == script && name.as_str() == property
            )
    }

    /// Pops the last snapshot, pushes `current` onto the redo stack, paired
    /// with the same `Change` log, since redoing this step reapplies exactly
    /// the mutation undoing it just reverted, and returns the popped DOM
    /// plus that log, for the caller to classify and install. `None` with
    /// nothing to undo, a no-op that leaves both stacks untouched.
    pub(crate) fn undo(&mut self, current: WeakDom) -> Option<(WeakDom, Vec<Change>)> {
        let previous = self.undo.pop()?;
        self.undo_bytes -= previous.bytes;
        self.revision += 1;
        let bytes = current.retained_bytes();
        self.redo_bytes += bytes;
        self.redo.push(Entry {
            dom: current,
            changes: previous.changes.clone(),
            at: Instant::now(),
            bytes,
        });
        self.trim_redo();
        Some((previous.dom, previous.changes))
    }

    /// Symmetric to [`History::undo`]: pops the last undone snapshot, pushes
    /// `current` back onto the undo stack (with the same log, for the same
    /// reason `undo` carries it onto `redo`), and returns the popped DOM
    /// plus that log.
    pub(crate) fn redo(&mut self, current: WeakDom) -> Option<(WeakDom, Vec<Change>)> {
        let next = self.redo.pop()?;
        self.redo_bytes -= next.bytes;
        self.revision += 1;
        let bytes = current.retained_bytes();
        self.undo_bytes += bytes;
        self.undo.push(Entry {
            dom: current,
            changes: next.changes.clone(),
            at: Instant::now(),
            bytes,
        });
        self.trim();
        Some((next.dom, next.changes))
    }

    /// [`History::trim`] for the redo stack: past the budget the oldest
    /// redos go, with no floor, since a step that can no longer be reached
    /// again is the cheapest thing an out-of-memory machine can lose.
    fn trim_redo(&mut self) {
        while !self.redo.is_empty() && self.redo_bytes > self.budget {
            self.redo_bytes -= self.redo[0].bytes;
            self.redo.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> WeakDom {
        let mut dom = WeakDom::new();
        dom.new_instance("Part", name, None);
        dom
    }

    fn root_name(dom: &WeakDom) -> String {
        dom.root_refs()
            .first()
            .and_then(|r| dom.get(*r))
            .map(|instance| instance.name().to_string())
            .expect("a root instance")
    }

    #[test]
    fn undo_then_redo_returns_to_the_same_state() {
        let mut history = History::new(DEFAULT_CAP);
        let before = named("Before");
        let after = named("After");

        history.push(before);
        let (undone, _) = history.undo(after.clone()).expect("something to undo");
        assert_eq!(root_name(&undone), "Before");

        let (redone, _) = history.redo(undone).expect("something to redo");
        assert_eq!(root_name(&redone), "After");
    }

    #[test]
    fn pushing_past_cap_drops_the_oldest_and_does_not_grow_unbounded() {
        let mut history = History::new(2);
        history.push(named("One"));
        history.push(named("Two"));
        history.push(named("Three"));

        assert_eq!(history.undo.len(), 2);
        let (top, _) = history.undo(named("Current")).expect("something to undo");
        assert_eq!(root_name(&top), "Three");
        let (next, _) = history
            .undo(named("Current"))
            .expect("still something to undo");
        assert_eq!(
            root_name(&next),
            "Two",
            "the oldest entry (\"One\") was evicted"
        );
    }

    #[test]
    fn a_new_push_clears_the_redo_stack() {
        let mut history = History::new(DEFAULT_CAP);
        history.push(named("Before"));
        let (undone, _) = history.undo(named("After")).expect("something to undo");
        assert!(!history.redo.is_empty());

        history.push(undone);
        assert!(
            history.redo.is_empty(),
            "a new action must invalidate any redo history"
        );
    }

    #[test]
    fn a_recorded_change_log_survives_undo_and_carries_onto_redo() {
        let mut history = History::new(DEFAULT_CAP);
        history.push(named("Before"));
        let change = Change::Property {
            referent: rbx_dom::Ref::new(1),
            name: "Transparency".to_string(),
        };
        history.record_changes(vec![change.clone()]);

        let (undone, changes) = history.undo(named("After")).expect("something to undo");
        assert_eq!(
            changes,
            vec![change.clone()],
            "the log recorded before undo travels with the snapshot"
        );

        let (_, changes) = history.redo(undone).expect("something to redo");
        assert_eq!(
            changes,
            vec![change],
            "redo reapplies the same mutation, so it carries the same log"
        );
    }

    #[test]
    fn record_changes_with_nothing_pushed_is_a_no_op() {
        let mut history = History::new(DEFAULT_CAP);
        history.record_changes(vec![Change::Added(rbx_dom::Ref::new(1))]);
        assert!(history.undo.is_empty());
    }

    #[test]
    fn undo_with_an_empty_stack_is_a_no_op() {
        let mut history = History::new(DEFAULT_CAP);
        assert!(history.undo(named("Current")).is_none());
        assert!(history.undo.is_empty());
        assert!(history.redo.is_empty());
    }

    #[test]
    fn redo_with_an_empty_stack_is_a_no_op() {
        let mut history = History::new(DEFAULT_CAP);
        assert!(history.redo(named("Current")).is_none());
    }

    #[test]
    fn ctrl_z_undoes() {
        let modifiers = Modifiers::secondary_key();
        assert_eq!(action_for("z", modifiers), Some(Action::Undo));
    }

    #[test]
    fn ctrl_y_and_ctrl_shift_z_both_redo() {
        let ctrl_y = Modifiers::secondary_key();
        assert_eq!(action_for("y", ctrl_y), Some(Action::Redo));

        let ctrl_shift_z = Modifiers {
            shift: true,
            ..Modifiers::secondary_key()
        };
        assert_eq!(action_for("z", ctrl_shift_z), Some(Action::Redo));
    }

    #[test]
    fn z_without_control_does_nothing() {
        assert_eq!(action_for("z", Modifiers::none()), None);
    }

    #[test]
    fn revision_moves_on_every_change_and_not_on_a_no_op() {
        let mut history = History::new(DEFAULT_CAP);
        assert!(history.undo(named("a")).is_none());
        assert_eq!(history.revision(), 0);
        history.push(named("a"));
        history.undo(named("b"));
        history.redo(named("a"));
        assert_eq!(history.revision(), 3);
        assert!(history.redo(named("b")).is_none());
        assert_eq!(history.revision(), 3);
    }

    fn script_with_source(size: usize) -> (WeakDom, rbx_dom::Ref) {
        let mut dom = WeakDom::new();
        let script = dom.new_instance("ModuleScript", "Counter", None);
        dom.set_property(script, "Source", rbx_dom::Variant::String("x".repeat(size)))
            .unwrap();
        (dom, script)
    }

    #[test]
    fn the_undo_stack_is_bounded_by_bytes_as_well_as_steps() {
        // Each snapshot carries a 24 KB source; the step cap would hold ten
        // of them, the budget two. Bytes bind first, the oldest go, and the
        // latest always stays.
        let mut history = History::with_limits(10, 64 * 1024);
        for _ in 0..5 {
            let (dom, _) = script_with_source(24 * 1024);
            history.push(dom);
        }
        assert_eq!(history.undo.len(), 2, "the budget, not the cap, binds");
        assert!(history.undo_bytes <= 64 * 1024);
        assert!(history.undo.first().unwrap().bytes > 24 * 1024);

        // Undo twice works; the third has nothing left to undo.
        assert!(history.undo(named("now")).is_some());
        assert!(history.undo(named("now")).is_some());
        assert!(history.undo(named("now")).is_none());
    }

    #[test]
    fn one_huge_snapshot_is_still_kept_alone() {
        // A budget of nothing still keeps the last step: undo of the most
        // recent action must exist whatever it cost.
        let mut history = History::with_limits(DEFAULT_CAP, 0);
        for _ in 0..3 {
            let (dom, _) = script_with_source(1024);
            history.push(dom);
        }
        assert_eq!(history.undo.len(), 1);
        assert!(history.undo(named("now")).is_some());
    }

    #[test]
    fn a_typing_streak_is_one_step_not_one_per_pause() {
        let (mut dom, script) = script_with_source(16);
        let mut history = History::new(DEFAULT_CAP);
        let window = Duration::from_secs(30);
        let write_source = |dom: &mut WeakDom, text: &str| {
            dom.set_property(script, "Source", rbx_dom::Variant::String(text.into()))
                .unwrap();
        };
        let logged = |dom: &mut WeakDom| dom.take_changes();

        // The first pause in a streak: `commit_script`'s snapshot path.
        history.push(dom.clone());
        write_source(&mut dom, "print(2)\n");
        history.record_changes(logged(&mut dom));

        // The pauses after it, within the window: the write continues the
        // step, and no further snapshot is taken.
        assert!(history.continues_streak(script, "Source", window));
        write_source(&mut dom, "print(3)\n");
        history.record_continued_change(logged(&mut dom));
        assert_eq!(history.undo.len(), 1, "the streak added no snapshot");
        assert!(history.continues_streak(script, "Source", window));
        assert_eq!(
            history.revision(),
            2,
            "a continued write moves revision too"
        );

        // One undo steps over the whole streak back to before it began; one
        // redo reinstates the streak's last text, not its first.
        let (previous, changes) = history.undo(dom.clone()).expect("the streak to undo");
        assert_eq!(
            changes,
            vec![Change::Property {
                referent: script,
                name: "Source".to_string()
            }]
        );
        assert_eq!(
            previous.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("x".repeat(16).into()))
        );
        let (next, _) = history.redo(previous).expect("the streak to redo");
        assert_eq!(
            next.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("print(3)\n".into()))
        );
    }

    #[test]
    fn a_streak_only_continues_its_own_step_and_only_while_fresh() {
        let (mut dom, script) = script_with_source(16);
        let mut history = History::new(DEFAULT_CAP);
        history.push(dom.clone());
        let window = Duration::from_secs(30);

        // Nothing recorded yet: the top entry's log is not the streak.
        assert!(!history.continues_streak(script, "Source", window));
        dom.take_changes();
        dom.set_property(
            script,
            "Source",
            rbx_dom::Variant::String("print(2)\n".into()),
        )
        .unwrap();
        history.record_changes(dom.take_changes());

        // Someone else's step, or another property, or a stale window: no.
        assert!(!history.continues_streak(rbx_dom::Ref::new(99), "Source", window));
        assert!(!history.continues_streak(script, "Health", window));
        let far = Instant::now() + window + window;
        assert!(!history.continues_streak_at(script, "Source", window, far));
        // The streak itself: yes.
        assert!(history.continues_streak(script, "Source", window));

        // A step pushed on top ends it, wherever its own log points.
        history.push(named("a drag"));
        assert!(!history.continues_streak(script, "Source", window));
    }
}
