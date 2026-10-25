//! The dock's typed-in numbers and asset ids, and the height slider: the
//! controls that need entities of their own. Built the first time the dock
//! draws (an `InputState` needs a window) and brought back in line with
//! the place every draw, since undo, a preset or the Properties panel can
//! move the same values. A field being typed into is left alone.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use rbx_dom::WeakDom;

use crate::avatar_settings::{
    self as rules, write_height, Key, Rule, Rules, Slot, SlotValue, Writes, ACCESSORY_SLOTS,
    BODY_SLOTS, BUILD_RANGES, CLIP_SLOTS, CLOTHING_SLOTS, COLLIDER_SIZE, HEIGHT_RANGE,
    WORLD_HEIGHT,
};

use super::super::Shell;

/// The four tables of custom asset rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    Body,
    Clothing,
    Accessory,
    Clip,
}

impl Group {
    pub(super) fn slots(self) -> &'static [Slot] {
        match self {
            Group::Body => &BODY_SLOTS,
            Group::Clothing => &CLOTHING_SLOTS,
            Group::Accessory => &ACCESSORY_SLOTS,
            Group::Clip => &CLIP_SLOTS,
        }
    }
}

pub(super) const CLOTHING_BOUNDS: Key = (Rule::Clothing, "LimitBounds");
pub(super) const ACCESSORY_BOUNDS: Key = (Rule::Accessory, "LimitBounds");

/// Which number or id a field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    /// Custom Scale's one height, without "Set minimum and maximum".
    Height,
    HeightMin,
    HeightMax,
    /// A Build row's minimum (`false`) or maximum (`true`), in percent.
    Build(usize, bool),
    /// One axis of a `Vector3` property.
    Axis(Key, usize),
    Slot(Group, usize),
}

impl Field {
    fn all() -> Vec<Field> {
        let mut all = vec![Field::Height, Field::HeightMin, Field::HeightMax];
        for row in 0..BUILD_RANGES.len() {
            all.extend([Field::Build(row, false), Field::Build(row, true)]);
        }
        for key in [CLOTHING_BOUNDS, ACCESSORY_BOUNDS, COLLIDER_SIZE] {
            all.extend((0..3).map(|axis| Field::Axis(key, axis)));
        }
        for group in [Group::Body, Group::Clothing, Group::Accessory, Group::Clip] {
            all.extend((0..group.slots().len()).map(|row| Field::Slot(group, row)));
        }
        all
    }

    fn read(self, rules: &Rules, dom: &WeakDom) -> String {
        let (min, max) = rules.range_of(dom, WORLD_HEIGHT);
        match self {
            Field::Height | Field::HeightMax => number(max),
            Field::HeightMin => number(min),
            Field::Build(row, upper) => {
                let (min, max) = BUILD_RANGES[row].read(rules, dom);
                number(if upper { max } else { min })
            }
            Field::Axis(key, axis) => number(rules.vector_of(dom, key)[axis]),
            Field::Slot(group, row) => match group.slots()[row].read(rules, dom) {
                SlotValue::Asset(id) => id.to_string(),
                SlotValue::Off | SlotValue::Remove => String::new(),
            },
        }
    }

    /// What typing `text` here writes, `None` for text that isn't a value.
    fn writes(self, text: &str, rules: &Rules, dom: &WeakDom) -> Option<Writes> {
        let text = text.trim();
        if let Field::Slot(group, row) = self {
            let slot = &group.slots()[row];
            let value = match text.parse::<i64>() {
                Ok(id) if id > 0 => SlotValue::Asset(id),
                _ if !text.is_empty() => return None,
                // Emptied: a ticked row that may be blank removes the
                // category; one that may not goes back to the player's own.
                _ if slot.blank_removes && slot.read(rules, dom) != SlotValue::Off => {
                    SlotValue::Remove
                }
                _ => SlotValue::Off,
            };
            return Some(slot.write(value));
        }
        let value = text.trim_end_matches('%').trim().parse::<f32>().ok()?;
        let (min, max) = rules.range_of(dom, WORLD_HEIGHT);
        Some(match self {
            Field::Height => write_height(value, value),
            Field::HeightMin => write_height(value, max),
            Field::HeightMax => write_height(min, value),
            Field::Build(row, upper) => {
                let (min, max) = BUILD_RANGES[row].read(rules, dom);
                if upper {
                    BUILD_RANGES[row].write(min, value)
                } else {
                    BUILD_RANGES[row].write(value, max)
                }
            }
            Field::Axis(key, axis) => {
                let mut v = rules.vector_of(dom, key);
                v[axis] = value.max(0.0);
                vec![(key, rules::vector(v))]
            }
            Field::Slot(..) => unreachable!("handled above"),
        })
    }
}

fn number(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    format!("{rounded}")
}

pub(in crate::shell) struct AvatarFields {
    inputs: Vec<(Field, Entity<InputState>)>,
    height: Entity<SliderState>,
    _subscriptions: Vec<Subscription>,
}

impl AvatarFields {
    pub(super) fn new(shell: &Shell, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let rules = Rules::find(&shell.dom);
        let mut subscriptions = Vec::new();
        let inputs = Field::all()
            .into_iter()
            .map(|field| {
                let text = field.read(&rules, &shell.dom);
                let placeholder = if matches!(field, Field::Slot(..)) {
                    "AssetID"
                } else {
                    ""
                };
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(text)
                });
                subscriptions.push(cx.subscribe(
                    &input,
                    move |shell, input, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            let text = input.read(cx).value().to_string();
                            shell.commit_avatar_field(field, &text, cx);
                        }
                    },
                ));
                (field, input)
            })
            .collect();
        let (_, max) = rules.range_of(&shell.dom, WORLD_HEIGHT);
        let height = cx.new(|_| {
            SliderState::new()
                .min(HEIGHT_RANGE.0)
                .max(HEIGHT_RANGE.1)
                .step(0.05)
                .default_value(max)
        });
        subscriptions.push(cx.subscribe(&height, |shell, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(SliderValue::Single(value)) = event {
                let value = (value * 20.0).round() / 20.0;
                shell.apply_avatar_writes(write_height(value, value), cx);
            }
        }));
        AvatarFields {
            inputs,
            height,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn input(&self, field: Field) -> &Entity<InputState> {
        &self
            .inputs
            .iter()
            .find(|(f, _)| *f == field)
            .expect("every field is built")
            .1
    }

    pub(super) fn height(&self) -> &Entity<SliderState> {
        &self.height
    }

    /// Puts every value back where the place has it.
    pub(super) fn sync(&self, dom: &WeakDom, window: &mut Window, cx: &mut App) {
        let rules = Rules::find(dom);
        let (_, max) = rules.range_of(dom, WORLD_HEIGHT);
        if (self.height.read(cx).value().end() - max).abs() > 1e-3 {
            self.height
                .update(cx, |state, cx| state.set_value(max, window, cx));
        }
        for (field, input) in &self.inputs {
            if input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            let text = field.read(&rules, dom);
            if input.read(cx).value().as_ref() != text {
                input.update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
    }
}

impl Shell {
    fn commit_avatar_field(&mut self, field: Field, text: &str, cx: &mut Context<Self>) {
        let rules = Rules::find(&self.dom);
        match field.writes(text, &rules, &self.dom) {
            Some(writes) => self.apply_avatar_writes(writes, cx),
            // Not a value: the next draw's sync puts the stored one back.
            None => cx.notify(),
        }
    }

    /// A custom row's box: unticking hands the category back to the
    /// player; ticking an empty row removes it where that's allowed, and
    /// otherwise waits for an id, as Studio refuses a blank body part.
    pub(super) fn toggle_avatar_slot(
        &mut self,
        group: Group,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let slot = &group.slots()[row];
        let rules = Rules::find(&self.dom);
        let value = match slot.read(&rules, &self.dom) {
            SlotValue::Off if slot.blank_removes => SlotValue::Remove,
            SlotValue::Off => {
                if let Some(fields) = &self.avatar.fields {
                    let handle = fields
                        .input(Field::Slot(group, row))
                        .read(cx)
                        .focus_handle(cx);
                    window.focus(&handle, cx);
                }
                return;
            }
            SlotValue::Remove | SlotValue::Asset(_) => SlotValue::Off,
        };
        self.apply_avatar_writes(slot.write(value), cx);
    }
}
