//! The window's number fields and sliders, and the values they edit.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;

use super::motion::Scale;
use super::state::Settings;
use super::FittingWindow;

/// A typed value, with a stepper either side of its field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    MoveX,
    MoveY,
    MoveZ,
    TurnX,
    TurnY,
    TurnZ,
    Scale,
    Order,
    Puffiness,
    Shrink,
    NudgeAmount,
    NudgeRadius,
}

impl Field {
    const ALL: [Field; 12] = [
        Field::MoveX,
        Field::MoveY,
        Field::MoveZ,
        Field::TurnX,
        Field::TurnY,
        Field::TurnZ,
        Field::Scale,
        Field::Order,
        Field::Puffiness,
        Field::Shrink,
        Field::NudgeAmount,
        Field::NudgeRadius,
    ];

    pub(super) fn step(self) -> f32 {
        match self {
            Field::MoveX | Field::MoveY | Field::MoveZ => 0.05,
            Field::TurnX | Field::TurnY | Field::TurnZ => 5.0,
            Field::Scale | Field::Shrink => 0.05,
            Field::Order => 1.0,
            Field::Puffiness | Field::NudgeRadius => 0.1,
            Field::NudgeAmount => 0.02,
        }
    }

    /// The value kept in range: a turn wraps, everything else clamps.
    pub(super) fn bound(self, v: f32) -> f32 {
        match self {
            Field::MoveX | Field::MoveY | Field::MoveZ => v.clamp(-10.0, 10.0),
            Field::TurnX | Field::TurnY | Field::TurnZ => (v + 180.0).rem_euclid(360.0) - 180.0,
            Field::Scale => v.clamp(0.1, 10.0),
            Field::Order => v.round().clamp(0.0, 99.0),
            // Studio clamps both to -1..1 on write.
            Field::Puffiness | Field::Shrink => v.clamp(-1.0, 1.0),
            Field::NudgeAmount => v.clamp(-1.0, 1.0),
            Field::NudgeRadius => v.clamp(0.1, 4.0),
        }
    }

    pub(super) fn format(self, v: f32) -> String {
        match self {
            Field::Order => format!("{v:.0}"),
            Field::TurnX | Field::TurnY | Field::TurnZ => format!("{v:.1}"),
            _ => format!("{v:.2}"),
        }
    }
}

pub(super) struct Inputs {
    fields: Vec<(Field, Entity<InputState>)>,
    pub(super) user: Entity<InputState>,
    pub(super) scales: Vec<(Scale, Entity<SliderState>)>,
    pub(super) opacity: Entity<SliderState>,
}

impl Inputs {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<FittingWindow>,
    ) -> (Inputs, Vec<Subscription>) {
        let mut subs = Vec::new();
        let defaults = Settings::default();
        let fields = Field::ALL
            .into_iter()
            .map(|field| {
                let text = field.format(read(&defaults, &Default::default(), field));
                let state = cx.new(|cx| InputState::new(window, cx).default_value(text));
                subs.push(cx.subscribe_in(
                    &state,
                    window,
                    move |view, state, ev: &InputEvent, window, cx| {
                        if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            let text = state.read(cx).value().to_string();
                            if let Ok(v) = text.trim().parse::<f32>() {
                                view.set_field(field, v, window, cx);
                            }
                        }
                    },
                ));
                (field, state)
            })
            .collect();
        let user = cx.new(|cx| InputState::new(window, cx).placeholder("User ID"));
        subs.push(
            cx.subscribe_in(&user, window, |view, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    view.load_player(cx);
                }
            }),
        );
        let scales = Scale::ALL
            .into_iter()
            .map(|scale| {
                let (min, max) = scale.range();
                let state = cx.new(|_| {
                    SliderState::new()
                        .min(min)
                        .max(max)
                        .step(0.01)
                        .default_value(max.min(1.0).max(min))
                });
                subs.push(cx.subscribe(&state, move |view, _, ev: &SliderEvent, cx| {
                    if let SliderEvent::Change(SliderValue::Single(v)) = ev {
                        view.set_scale(scale, *v, cx);
                    }
                }));
                (scale, state)
            })
            .collect();
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.1)
                .max(1.0)
                .step(0.05)
                .default_value(0.6)
        });
        subs.push(cx.subscribe(&opacity, |view, _, ev: &SliderEvent, cx| {
            if let SliderEvent::Change(SliderValue::Single(v)) = ev {
                view.cage_opacity = *v;
                view.sync(cx);
            }
        }));
        let inputs = Inputs {
            fields,
            user,
            scales,
            opacity,
        };
        (inputs, subs)
    }

    pub(super) fn field(&self, field: Field) -> &Entity<InputState> {
        &self
            .fields
            .iter()
            .find(|(f, _)| *f == field)
            .expect("every field has an input")
            .1
    }

    fn show(&self, field: Field, v: f32, window: &mut Window, cx: &mut App) {
        let text = field.format(v);
        self.field(field)
            .update(cx, |s, cx| s.set_value(text, window, cx));
    }

    /// Shows the fit's values after something other than typing changed
    /// them.
    pub(super) fn show_settings(&self, settings: &Settings, window: &mut Window, cx: &mut App) {
        for field in Field::ALL {
            if !matches!(field, Field::NudgeAmount | Field::NudgeRadius) {
                self.show(
                    field,
                    read(settings, &Default::default(), field),
                    window,
                    cx,
                );
            }
        }
    }

    /// Puts the sliders at `scales`.
    pub(super) fn show_scales(
        &self,
        scales: &crate::shell::rig::Scales,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (scale, state) in &self.scales {
            let v = scale.read(scales);
            state.update(cx, |s, cx| s.set_value(v, window, cx));
        }
    }
}

/// A field's value in `settings` (or `nudge`).
fn read(settings: &Settings, nudge: &super::view_fit::Nudge, field: Field) -> f32 {
    let p = &settings.placement;
    let w = &settings.wrap;
    match field {
        Field::MoveX => p.offset[0],
        Field::MoveY => p.offset[1],
        Field::MoveZ => p.offset[2],
        Field::TurnX => p.rotation[0],
        Field::TurnY => p.rotation[1],
        Field::TurnZ => p.rotation[2],
        Field::Scale => p.scale,
        Field::Order => w.order as f32,
        Field::Puffiness => w.puffiness,
        Field::Shrink => w.shrink,
        Field::NudgeAmount => nudge.amount,
        Field::NudgeRadius => nudge.radius,
    }
}

impl FittingWindow {
    pub(super) fn field_value(&self, field: Field) -> f32 {
        read(&self.settings, &self.nudge, field)
    }

    /// Sets a field from typing or a stepper, and shows what it kept.
    pub(super) fn set_field(
        &mut self,
        field: Field,
        v: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let v = field.bound(v);
        let p = &mut self.settings.placement;
        let w = &mut self.settings.wrap;
        match field {
            Field::MoveX => p.offset[0] = v,
            Field::MoveY => p.offset[1] = v,
            Field::MoveZ => p.offset[2] = v,
            Field::TurnX => p.rotation[0] = v,
            Field::TurnY => p.rotation[1] = v,
            Field::TurnZ => p.rotation[2] = v,
            Field::Scale => p.scale = v,
            Field::Order => w.order = v as i32,
            Field::Puffiness => w.puffiness = v,
            Field::Shrink => w.shrink = v,
            Field::NudgeAmount => self.nudge.amount = v,
            Field::NudgeRadius => self.nudge.radius = v,
        }
        self.inputs.show(field, v, window, cx);
        if !matches!(field, Field::NudgeAmount | Field::NudgeRadius) {
            self.sync(cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::Field;

    #[test]
    fn turns_wrap_and_everything_else_clamps() {
        assert_eq!(Field::TurnY.bound(190.0), -170.0);
        assert_eq!(Field::TurnY.bound(-180.0), -180.0);
        assert_eq!(Field::Scale.bound(0.0), 0.1);
        assert_eq!(Field::Order.bound(2.6), 3.0);
        assert_eq!(Field::Shrink.bound(3.0), 1.0);
        assert_eq!(Field::Puffiness.bound(2.0), 1.0);
        assert_eq!(Field::Shrink.bound(-3.0), -1.0);
        assert_eq!(Field::Order.format(3.0), "3");
        assert_eq!(Field::MoveX.format(0.783), "0.78");
    }
}
