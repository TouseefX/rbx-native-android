//! The `RBX_STUDIO_UGC_FIT_*` knobs a scripted launch drives the window
//! with (listed in the module doc above), applied once it opens.

use gpui_kit::*;
use rbx_ugc::BodyScale;

use super::state::{Body, Motion, Step};
use super::FittingWindow;

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn triple(text: &str) -> Option<[f32; 3]> {
    let parts: Vec<f32> = text
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

pub(super) fn step(text: &str) -> Option<Step> {
    Step::ALL
        .into_iter()
        .find(|s| s.label().eq_ignore_ascii_case(text))
}

pub(super) fn body(text: &str) -> Option<Body> {
    Some(match text.to_ascii_lowercase().as_str() {
        "classic" => Body::Preset(BodyScale::Classic),
        "normal" => Body::Preset(BodyScale::Normal),
        "slender" => Body::Preset(BodyScale::Slender),
        "me" => Body::MyAvatar,
        other => Body::Player(other.strip_prefix("player:")?.parse().ok()?),
    })
}

pub(super) fn apply(
    view: &mut FittingWindow,
    window: &mut Window,
    cx: &mut Context<FittingWindow>,
) {
    let mut placement = view.settings.placement;
    if let Some(offset) = var("RBX_STUDIO_UGC_FIT_MOVE").as_deref().and_then(triple) {
        placement.offset = offset;
    }
    if let Some(rotation) = var("RBX_STUDIO_UGC_FIT_TURN").as_deref().and_then(triple) {
        placement.rotation = rotation;
    }
    if placement != view.settings.placement {
        view.settings.placement = placement;
        view.inputs.show_settings(&view.settings, window, cx);
    }
    if let Some(step) = var("RBX_STUDIO_UGC_FIT_STEP").as_deref().and_then(step) {
        view.go(step, cx);
    }
    if let Some(body) = var("RBX_STUDIO_UGC_FIT_BODY").as_deref().and_then(body) {
        view.set_body(body, cx);
    }
    if let Some(motion) = var("RBX_STUDIO_UGC_FIT_ANIM").and_then(|m| {
        Motion::ALL
            .into_iter()
            .find(|x| x.label().eq_ignore_ascii_case(&m))
    }) {
        view.play(motion, cx);
    }
    view.sync(cx);
}

#[cfg(test)]
mod tests {
    use super::{body, step, triple, Body, BodyScale, Step};

    #[test]
    fn knobs_parse() {
        assert_eq!(step("Test"), Some(Step::Test));
        assert_eq!(body("player:156"), Some(Body::Player(156)));
        assert_eq!(body("slender"), Some(Body::Preset(BodyScale::Slender)));
        assert_eq!(body("me"), Some(Body::MyAvatar));
        assert_eq!(body("nobody"), None);
        assert_eq!(triple("0.5, 0, -1"), Some([0.5, 0.0, -1.0]));
        assert_eq!(triple("1,2"), None);
    }
}
