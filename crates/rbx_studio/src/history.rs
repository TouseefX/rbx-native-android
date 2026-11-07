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
//!
//! And the budget bounds what the stack holds, not what taking a snapshot
//! costs: the clone is a second copy of the place for as long as it is being
//! made, on top of the first, before `push` can measure and trim anything.
//! The two paths that fire per keystroke and per click cannot afford that at
//! all — a script editor's debounced `Source` write and a Properties-panel
//! commit file their steps as the writes themselves (see [`PropertyWrite`]
//! and [`History::push_properties`]): the value each write replaced and the
//! value it left, `old` and `new`, which undo and redo apply directly to the
//! live tree. One property reverts without the tree ever being copied, and
//! the transient that ran the phone out of memory is never allocated. The
//! rarer, wider mutations — inserts, deletes, drags, script runs — keep
//! their snapshots.

use std::time::{Duration, Instant};

use gpui_kit::Modifiers;
use rbx_dom::{Change, Ref, Variant, WeakDom};

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

/// One property write, filed as its own inverse: what the property held
/// before the step (`old`) and what the step left it holding (`new`),
/// `None` on either side for a property that was absent. Undo applies
/// `old` back — removing the property when it was absent — redo applies
/// `new`, and neither direction ever holds a copy of the tree: a
/// whole-place snapshot per script commit was the second copy of the
/// place that ran the phone out of memory (see the module doc).
#[derive(Clone, Debug)]
pub(crate) struct PropertyWrite {
    pub(crate) referent: Ref,
    pub(crate) name: String,
    pub(crate) old: Option<Variant>,
    pub(crate) new: Option<Variant>,
}

/// What one step hands back to the shell's undo and redo. The DOM the
/// caller passes in is consumed onto the opposite stack for a snapshot
/// step and handed straight back otherwise, so every variant carries the
/// DOM the caller should end up holding.
pub(crate) enum Stepped {
    /// A snapshot step: the tree as it stood before the step for an undo,
    /// as it stands now for a redo, plus the log to reflect against it.
    Snapshot(WeakDom, Vec<Change>),
    /// A property step: the caller's own tree back, untouched, plus the
    /// writes to apply to it — each write's `old` for an undo, its `new`
    /// for a redo, with `None` removing the property.
    Property(WeakDom, Vec<(Ref, String, Option<Variant>)>),
    /// No step to take: the caller's tree, unchanged.
    Unchanged(WeakDom),
}

/// One step on either stack.
struct Entry {
    /// What the step is: a whole tree, or property writes that are their
    /// own inverse. See [`Step`].
    step: Step,
    /// When the entry was last written to: pushed, or continued by
    /// [`History::continue_streak`]. Read by
    /// [`History::continues_streak`] to decide whether a script's next
    /// debounced write still belongs to this step.
    at: Instant,
    /// What the entry retains: `dom.retained_bytes()` for a snapshot, the
    /// writes' values for a property step (see [`write_bytes`]), taken
    /// once at construction so the budget never re-walks a tree to know
    /// what it is holding.
    bytes: usize,
}

/// The two kinds of step the stacks hold.
enum Step {
    /// The DOM as it stood before the mutation, plus the `Change` log that
    /// mutation went on to produce (see [`History::record_changes`]):
    /// what a snapshot step's undo hands back to install.
    Snapshot { dom: WeakDom, changes: Vec<Change> },
    /// The writes of a property step, each its own inverse: undo applies
    /// `old`, redo applies `new`, and no tree is held at all.
    Properties { writes: Vec<PropertyWrite> },
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
            step: Step::Snapshot {
                dom,
                changes: Vec::new(),
            },
            at: Instant::now(),
            bytes,
        });
        self.redo.clear();
        self.redo_bytes = 0;
        self.trim();
        self.revision += 1;
    }

    /// Files `writes` as one undo step that is its own inverse: undo
    /// applies each write's `old`, redo its `new`, and the step never
    /// holds a copy of the tree. For the edits this exists for — a
    /// script's `Source` on a typing debounce, a Properties-panel commit
    /// over a selection — a whole-place snapshot cost a second copy of
    /// the place per step, transiently two while it was taken, which on
    /// a device-sized place is the difference between an undo history
    /// and an out-of-memory abort (see the module doc).
    pub(crate) fn push_properties(&mut self, writes: Vec<PropertyWrite>) {
        let bytes = writes.iter().map(write_bytes).sum();
        self.undo_bytes += bytes;
        self.undo.push(Entry {
            step: Step::Properties { writes },
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

    /// Whether [`History::undo`] has a step to take. The shell asks before
    /// taking its DOM out to hand over: a no-op undo must not cost the
    /// live tree its fitting preview (see `shell::history`).
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// [`History::can_undo`] for the redo stack.
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Attaches `changes` to the snapshot entry `push` most recently
    /// added, see [`Entry`]'s doc comment. A no-op if nothing has been
    /// pushed yet (should not happen given `Shell::push_history`'s own
    /// call sites, but costs nothing to guard), and a no-op on a property
    /// step, which reflects what undo applies rather than a log.
    pub(crate) fn record_changes(&mut self, changes: Vec<Change>) {
        if let Some(entry) = self.undo.last_mut() {
            if let Step::Snapshot { changes: log, .. } = &mut entry.step {
                *log = changes;
            }
        }
    }

    /// A typing streak's latest commit, recorded against the step already
    /// on top of the undo stack: for a property step the write's `new`
    /// becomes `value` — its `old` still standing from before the streak
    /// began — and for a snapshot step the log is re-recorded against the
    /// tree as it stood before the streak began (a Properties-panel
    /// `Source` edit continued from the editor lands there). `revision`
    /// moves either way: the tree changed without a push.
    pub(crate) fn continue_streak(
        &mut self,
        script: Ref,
        property: &str,
        value: Option<Variant>,
        changes: Vec<Change>,
    ) {
        if let Some(entry) = self.undo.last_mut() {
            match &mut entry.step {
                Step::Properties { writes } => match writes.as_mut_slice() {
                    [write] if write.referent == script && write.name == property => {
                        write.new = value;
                    }
                    _ => {}
                },
                Step::Snapshot { changes: log, .. } => *log = changes,
            }
            entry.at = Instant::now();
            self.revision += 1;
        }
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
        if now.duration_since(entry.at) > within {
            return false;
        }
        match &entry.step {
            // A property step is the streak's own shape: one write to the
            // script's property.
            Step::Properties { writes } => matches!(
                writes.as_slice(),
                [write] if write.referent == script && write.name == property
            ),
            // A snapshot step qualifies by its log: the same single write,
            // filed the Properties-panel way.
            Step::Snapshot { changes, .. } => matches!(
                changes.as_slice(),
                [Change::Property { referent, name }]
                    if *referent == script && name.as_str() == property
            ),
        }
    }

    /// Pops the last step. A snapshot step consumes `current` onto the redo
    /// stack — paired with the same `Change` log, since redoing it reapplies
    /// exactly the mutation undoing it just reverted — and hands the popped
    /// DOM plus that log back for the caller to install. A property step
    /// hands `current` straight back with each write's `old` to apply to it,
    /// and files the writes swapped — `new` where `old` stood — onto the
    /// redo stack for the redo to apply back. [`Stepped::Unchanged`] with
    /// nothing to undo, a no-op that leaves both stacks and the DOM alone.
    pub(crate) fn undo_step(&mut self, current: WeakDom) -> Stepped {
        let Some(previous) = self.undo.pop() else {
            return Stepped::Unchanged(current);
        };
        self.undo_bytes -= previous.bytes;
        self.revision += 1;
        match previous.step {
            Step::Snapshot { dom, changes } => {
                let bytes = current.retained_bytes();
                self.redo_bytes += bytes;
                self.redo.push(Entry {
                    step: Step::Snapshot {
                        dom: current,
                        changes: changes.clone(),
                    },
                    at: Instant::now(),
                    bytes,
                });
                self.trim_redo();
                Stepped::Snapshot(dom, changes)
            }
            Step::Properties { writes } => {
                let undone = writes
                    .iter()
                    .map(|write| (write.referent, write.name.clone(), write.old.clone()))
                    .collect();
                self.redo_bytes += previous.bytes;
                self.redo.push(Entry {
                    step: Step::Properties {
                        writes: swapped(&writes),
                    },
                    at: Instant::now(),
                    bytes: previous.bytes,
                });
                self.trim_redo();
                Stepped::Property(current, undone)
            }
        }
    }

    /// Symmetric to [`History::undo_step`]: pops the last undone step,
    /// snapshot or property, files `current` (or the swapped writes) back
    /// onto the undo stack, and hands back what the caller should hold and
    /// apply.
    pub(crate) fn redo_step(&mut self, current: WeakDom) -> Stepped {
        let Some(next) = self.redo.pop() else {
            return Stepped::Unchanged(current);
        };
        self.redo_bytes -= next.bytes;
        self.revision += 1;
        match next.step {
            Step::Snapshot { dom, changes } => {
                let bytes = current.retained_bytes();
                self.undo_bytes += bytes;
                self.undo.push(Entry {
                    step: Step::Snapshot {
                        dom: current,
                        changes: changes.clone(),
                    },
                    at: Instant::now(),
                    bytes,
                });
                self.trim();
                Stepped::Snapshot(dom, changes)
            }
            Step::Properties { writes } => {
                let redone = writes
                    .iter()
                    .map(|write| (write.referent, write.name.clone(), write.new.clone()))
                    .collect();
                self.undo_bytes += next.bytes;
                self.undo.push(Entry {
                    step: Step::Properties {
                        writes: swapped(&writes),
                    },
                    at: Instant::now(),
                    bytes: next.bytes,
                });
                self.trim();
                Stepped::Property(current, redone)
            }
        }
    }

    /// [`History::undo_step`] with the property half applied on the way
    /// out: a property step's writes are applied to the caller's own tree
    /// — which is handed back — and the log they produce returned beside
    /// it, so a caller that only wants "the tree as it stood, and the log
    /// to reflect against it" sees one shape whatever kind the step was.
    /// The shell's undo uses [`History::undo_step`] directly instead,
    /// applying the writes through the live tree's own reflection path.
    /// `None` with nothing to undo, as ever a no-op.
    pub(crate) fn undo(&mut self, current: WeakDom) -> Option<(WeakDom, Vec<Change>)> {
        match self.undo_step(current) {
            Stepped::Snapshot(dom, changes) => Some((dom, changes)),
            Stepped::Property(mut dom, writes) => {
                apply_writes(&mut dom, &writes);
                let changes = dom.take_changes();
                Some((dom, changes))
            }
            Stepped::Unchanged(_) => None,
        }
    }

    /// [`History::redo`]'s shape over [`History::redo_step`]: the property
    /// half applied, the log it produces returned.
    pub(crate) fn redo(&mut self, current: WeakDom) -> Option<(WeakDom, Vec<Change>)> {
        match self.redo_step(current) {
            Stepped::Snapshot(dom, changes) => Some((dom, changes)),
            Stepped::Property(mut dom, writes) => {
                apply_writes(&mut dom, &writes);
                let changes = dom.take_changes();
                Some((dom, changes))
            }
            Stepped::Unchanged(_) => None,
        }
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

/// Applies property-step writes to a tree: what [`History::undo`] and
/// [`History::redo`] do with the property half of what a step hands back,
/// applying each write's `old` (or `new`) and letting the tree's own change
/// log say what was touched.
fn apply_writes(dom: &mut WeakDom, writes: &[(Ref, String, Option<Variant>)]) {
    for (referent, name, value) in writes {
        match value {
            Some(value) => {
                let _ = dom.set_property(*referent, name, value.clone());
            }
            None => {
                let _ = dom.remove_property(*referent, name);
            }
        }
    }
}

/// The same writes with `old` and `new` exchanged: what undoing a property
/// step files onto the redo stack, and what redoing one files back onto
/// undo, so each direction's step is the other's inverse.
fn swapped(writes: &[PropertyWrite]) -> Vec<PropertyWrite> {
    writes
        .iter()
        .map(|write| PropertyWrite {
            referent: write.referent,
            name: write.name.clone(),
            old: write.new.clone(),
            new: write.old.clone(),
        })
        .collect()
}

/// What one property write retains: the write's own size plus its two
/// values', in `Variant::retained_bytes`' over-estimating spirit, so a
/// history of big scripts stays inside [`RETAINED_BUDGET`] the same way a
/// history of snapshots does.
fn write_bytes(write: &PropertyWrite) -> usize {
    std::mem::size_of::<PropertyWrite>()
        + write.name.len()
        + write.old.as_ref().map_or(0, Variant::retained_bytes)
        + write.new.as_ref().map_or(0, Variant::retained_bytes)
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

    /// The snapshot half of what a step hands back, as the shell's snapshot
    /// arm consumes it.
    fn snapshot_step(step: Stepped) -> (WeakDom, Vec<Change>) {
        match step {
            Stepped::Snapshot(dom, changes) => (dom, changes),
            Stepped::Property(..) | Stepped::Unchanged(..) => {
                panic!("expected a snapshot step")
            }
        }
    }

    /// The property half of what a step hands back, as the shell's property
    /// arm consumes it.
    fn property_step(step: Stepped) -> (WeakDom, Vec<(Ref, String, Option<Variant>)>) {
        match step {
            Stepped::Property(dom, writes) => (dom, writes),
            Stepped::Snapshot(..) | Stepped::Unchanged(..) => {
                panic!("expected a property step")
            }
        }
    }

    /// `Shell::apply_history_writes`: what the shell does with the writes a
    /// property step hands back.
    fn apply(dom: &mut WeakDom, writes: &[(Ref, String, Option<Variant>)]) {
        for (referent, name, value) in writes {
            match value {
                Some(value) => {
                    dom.set_property(*referent, name, value.clone()).unwrap();
                }
                None => {
                    dom.remove_property(*referent, name).unwrap();
                }
            }
        }
    }

    #[test]
    fn undo_then_redo_returns_to_the_same_state() {
        let mut history = History::new(DEFAULT_CAP);
        let before = named("Before");
        let after = named("After");

        history.push(before);
        let (undone, _) = snapshot_step(history.undo_step(after.clone()));
        assert_eq!(root_name(&undone), "Before");

        let (redone, _) = snapshot_step(history.redo_step(undone));
        assert_eq!(root_name(&redone), "After");
    }

    #[test]
    fn pushing_past_cap_drops_the_oldest_and_does_not_grow_unbounded() {
        let mut history = History::new(2);
        history.push(named("One"));
        history.push(named("Two"));
        history.push(named("Three"));

        assert_eq!(history.undo.len(), 2);
        let (top, _) = snapshot_step(history.undo_step(named("Current")));
        assert_eq!(root_name(&top), "Three");
        let (next, _) = snapshot_step(history.undo_step(named("Current")));
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
        let (undone, _) = snapshot_step(history.undo_step(named("After")));
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

        let (undone, changes) = snapshot_step(history.undo_step(named("After")));
        assert_eq!(
            changes,
            vec![change.clone()],
            "the log recorded before undo travels with the snapshot"
        );

        let (_, changes) = snapshot_step(history.redo_step(undone));
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
        assert!(matches!(
            history.undo_step(named("Current")),
            Stepped::Unchanged(_)
        ));
        assert!(history.undo.is_empty());
        assert!(history.redo.is_empty());
    }

    #[test]
    fn redo_with_an_empty_stack_is_a_no_op() {
        let mut history = History::new(DEFAULT_CAP);
        assert!(matches!(
            history.redo_step(named("Current")),
            Stepped::Unchanged(_)
        ));
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
        assert!(matches!(
            history.undo_step(named("a")),
            Stepped::Unchanged(_)
        ));
        assert_eq!(history.revision(), 0);
        history.push(named("a"));
        history.undo_step(named("b"));
        history.redo_step(named("a"));
        assert_eq!(history.revision(), 3);
        assert!(matches!(
            history.redo_step(named("b")),
            Stepped::Unchanged(_)
        ));
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
        assert!(!matches!(
            history.undo_step(named("now")),
            Stepped::Unchanged(_)
        ));
        assert!(!matches!(
            history.undo_step(named("now")),
            Stepped::Unchanged(_)
        ));
        assert!(matches!(
            history.undo_step(named("now")),
            Stepped::Unchanged(_)
        ));
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
        assert!(!matches!(
            history.undo_step(named("now")),
            Stepped::Unchanged(_)
        ));
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

        // The first pause in a streak: `commit_script`'s property path —
        // drain, read the old value, write, file the write as its own
        // inverse.
        dom.take_changes();
        let old = dom
            .get(script)
            .and_then(|instance| instance.properties().get("Source"))
            .cloned();
        write_source(&mut dom, "print(2)\n");
        history.push_properties(vec![PropertyWrite {
            referent: script,
            name: "Source".to_string(),
            old,
            new: Some(rbx_dom::Variant::String("print(2)\n".into())),
        }]);
        logged(&mut dom);

        // The pauses after it, within the window: the write continues the
        // step, and nothing new is filed.
        assert!(history.continues_streak(script, "Source", window));
        write_source(&mut dom, "print(3)\n");
        history.continue_streak(
            script,
            "Source",
            Some(rbx_dom::Variant::String("print(3)\n".into())),
            logged(&mut dom),
        );
        assert_eq!(history.undo.len(), 1, "the streak added no step");
        assert!(history.continues_streak(script, "Source", window));
        assert_eq!(
            history.revision(),
            2,
            "a continued write moves revision too"
        );

        // One undo steps over the whole streak back to before it began; one
        // redo reinstates the streak's last text, not its first.
        let (mut undone, writes) = property_step(history.undo_step(dom.clone()));
        apply(&mut undone, &writes);
        assert_eq!(
            undone.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("x".repeat(16)))
        );
        let (mut redone, writes) = property_step(history.redo_step(undone));
        apply(&mut redone, &writes);
        assert_eq!(
            redone.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("print(3)\n".into()))
        );
    }

    #[test]
    fn a_streak_only_continues_its_own_step_and_only_while_fresh() {
        let (mut dom, script) = script_with_source(16);
        let mut history = History::new(DEFAULT_CAP);
        history.push(dom.clone());
        let window = Duration::from_secs(30);

        // Nothing recorded yet: the top snapshot entry's log is not the
        // streak.
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
        // The streak itself: yes — a snapshot step qualifies by its log, a
        // property step by its write.
        assert!(history.continues_streak(script, "Source", window));

        // A step pushed on top ends it, wherever its own log points.
        history.push(named("a drag"));
        assert!(!history.continues_streak(script, "Source", window));

        // A property step to another script or another property is not
        // this streak's continuation either.
        history.push_properties(vec![PropertyWrite {
            referent: script,
            name: "Source".to_string(),
            old: None,
            new: None,
        }]);
        assert!(history.continues_streak(script, "Source", window));
        history.push_properties(vec![PropertyWrite {
            referent: script,
            name: "Health".to_string(),
            old: None,
            new: None,
        }]);
        assert!(!history.continues_streak(script, "Source", window));
    }

    /// The regression the device crash fixed: a script's step costs its
    /// text, not the place. A snapshot of the same tree would have put the
    /// tree's whole retained size on the stack's books.
    #[test]
    fn a_property_step_costs_its_values_not_the_place() {
        let (dom, _) = script_with_source(64 * 1024);
        let tree_bytes = dom.retained_bytes();
        let mut history = History::with_limits(DEFAULT_CAP, 16 * 1024 * 1024);
        history.push_properties(vec![PropertyWrite {
            referent: rbx_dom::Ref::new(1),
            name: "Source".to_string(),
            old: Some(rbx_dom::Variant::String("x".repeat(16))),
            new: Some(rbx_dom::Variant::String("y".repeat(16))),
        }]);
        assert!(
            history.undo_bytes < tree_bytes,
            "a property step must not retain the place: {} vs the tree's {}",
            history.undo_bytes,
            tree_bytes
        );
        assert_eq!(
            history.undo_bytes,
            std::mem::size_of::<PropertyWrite>()
                + "Source".len()
                + 32
                + 2 * std::mem::size_of::<Variant>(),
            "the two texts, twice the enum, and the write itself"
        );

        // And it undoes: the stack was never trimmed by the tree's size,
        // only by the writes'.
        assert!(!matches!(history.undo_step(dom), Stepped::Unchanged(_)));
    }

    #[test]
    fn a_property_step_undoes_and_redoes_without_ever_holding_the_tree() {
        let mut dom = WeakDom::new();
        let script = dom.new_instance("ModuleScript", "Counter", None);
        dom.set_property(
            script,
            "Source",
            rbx_dom::Variant::String("print(1)\n".into()),
        )
        .unwrap();
        dom.take_changes();

        let mut history = History::new(DEFAULT_CAP);
        history.push_properties(vec![PropertyWrite {
            referent: script,
            name: "Source".to_string(),
            old: Some(rbx_dom::Variant::String("print(1)\n".into())),
            new: Some(rbx_dom::Variant::String("print(2)\n".into())),
        }]);

        // Undo hands the caller's own tree back with the old value to
        // apply; no snapshot is installed, the tree never changed identity.
        let (mut undone, writes) = property_step(history.undo_step(dom.clone()));
        apply(&mut undone, &writes);
        assert_eq!(
            undone.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("print(1)\n".into()))
        );

        // Redo reapplies the new value the same way.
        let (mut redone, writes) = property_step(history.redo_step(undone));
        apply(&mut redone, &writes);
        assert_eq!(
            redone.get(script).unwrap().properties().get("Source"),
            Some(&rbx_dom::Variant::String("print(2)\n".into()))
        );
        assert!(!matches!(history.redo_step(redone), Stepped::Unchanged(_)));
    }

    /// A property that was absent before the step: undo removes it again
    /// rather than writing some value, and a step that removed a property
    /// has it redone as a removal.
    #[test]
    fn an_absent_property_is_undone_as_an_absence_and_redone_as_one() {
        let mut dom = WeakDom::new();
        let part = dom.new_instance("Part", "Part", None);
        dom.take_changes();

        let mut history = History::new(DEFAULT_CAP);
        dom.set_property(part, "Transparency", Variant::Float32(0.5))
            .unwrap();
        history.push_properties(vec![PropertyWrite {
            referent: part,
            name: "Transparency".to_string(),
            old: None,
            new: Some(Variant::Float32(0.5)),
        }]);
        dom.take_changes();

        let (mut undone, writes) = property_step(history.undo_step(dom));
        apply(&mut undone, &writes);
        assert_eq!(
            undone.get(part).unwrap().properties().get("Transparency"),
            None,
            "undo removes what the step created"
        );

        let (mut redone, writes) = property_step(history.redo_step(undone));
        apply(&mut redone, &writes);
        assert_eq!(
            redone.get(part).unwrap().properties().get("Transparency"),
            Some(&Variant::Float32(0.5))
        );

        // A step that removed a property, redone, removes it again.
        history.push_properties(vec![PropertyWrite {
            referent: part,
            name: "Transparency".to_string(),
            old: Some(Variant::Float32(0.5)),
            new: None,
        }]);
        let (mut undone, writes) = property_step(history.undo_step(redone));
        apply(&mut undone, &writes);
        assert!(undone
            .get(part)
            .unwrap()
            .properties()
            .get("Transparency")
            .is_some());
        let (mut redone, writes) = property_step(history.redo_step(undone));
        apply(&mut redone, &writes);
        assert!(redone
            .get(part)
            .unwrap()
            .properties()
            .get("Transparency")
            .is_none());
    }

    /// Both kinds of step share the stacks: undo walks back through
    /// property steps and snapshot steps in the order they were filed, and
    /// redo walks the same way forward.
    #[test]
    fn snapshot_and_property_steps_undo_in_order() {
        let mut dom = WeakDom::new();
        let part = dom.new_instance("Part", "Part", None);
        dom.set_property(part, "Transparency", Variant::Float32(0.0))
            .unwrap();
        dom.take_changes();

        let mut history = History::new(DEFAULT_CAP);

        // A structural step: the part is renamed through a snapshot.
        history.push(dom.clone());
        dom.set_name(part, "Renamed").unwrap();
        history.record_changes(dom.take_changes());

        // A property step on top of it.
        dom.set_property(part, "Transparency", Variant::Float32(0.5))
            .unwrap();
        history.push_properties(vec![PropertyWrite {
            referent: part,
            name: "Transparency".to_string(),
            old: Some(Variant::Float32(0.0)),
            new: Some(Variant::Float32(0.5)),
        }]);
        dom.take_changes();

        // Undo the property step: the tree is handed back, the write
        // applied, and the rename from the snapshot step survives.
        let (mut current, writes) = property_step(history.undo_step(dom));
        apply(&mut current, &writes);
        assert_eq!(
            current.get(part).unwrap().properties().get("Transparency"),
            Some(&Variant::Float32(0.0))
        );
        assert_eq!(current.get(part).unwrap().name(), "Renamed");

        // Undo the snapshot step: the pre-rename tree is installed.
        let (previous, _) = snapshot_step(history.undo_step(current));
        assert_eq!(previous.get(part).unwrap().name(), "Part");

        // Redo both, in order, ending where the edits left off.
        let (current, _) = snapshot_step(history.redo_step(previous));
        assert_eq!(current.get(part).unwrap().name(), "Renamed");
        let (mut current, writes) = property_step(history.redo_step(current));
        apply(&mut current, &writes);
        assert_eq!(
            current.get(part).unwrap().properties().get("Transparency"),
            Some(&Variant::Float32(0.5))
        );
        assert!(matches!(history.redo_step(current), Stepped::Unchanged(_)));
    }
}
