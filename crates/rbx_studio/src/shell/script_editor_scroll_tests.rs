//! The script editor's touch panning, pinned end to end under GPUI's headless
//! window.
//!
//! On device a plain finger pan over the code editor does not scroll it,
//! while a selection drag's auto-scroll and every scrollable do scroll — so
//! the loss is somewhere between the raw touch and the editor's
//! `on_scroll_wheel`. This renders the very widget `shell::script_panel`
//! renders — the kit `Editor`, in the panel's own wrapper — and pans it the
//! way a finger does: a raw `TouchEvent` sequence past the touch slop, which
//! the recognizer must turn into the `ScrollWheelEvent` the editor consumes.
//! A wheel event is simulated first, on its own, so a failure names its half:
//! the listener (wheel leg) or the recognizer (pan leg).

use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    div, point, px, relative, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, ScrollDelta, ScrollWheelEvent, Styled as _, TestAppContext, TouchEvent, TouchId,
    TouchPhase, VisualTestContext, Window,
};

/// The panel's own wrapper around the editor, as `shell::script_panel`
/// builds it: a relative, flex-1, overflow-hidden hole the editor fills.
struct Panel {
    editor: Entity<EditorState>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().size_full().child(
            div().relative().flex_1().overflow_hidden().child(
                Editor::new(&self.editor)
                    .bordered(false)
                    .h(relative(1.0))
                    .w_full(),
            ),
        )
    }
}

#[gpui_kit::test]
fn a_finger_pan_scrolls_the_script_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut built = None;
    let (_panel, cx) = cx.add_window_view(|window, cx| {
        let editor =
            cx.new(|cx| EditorState::new(window, cx).default_value("local x = 1\n".repeat(400)));
        built = Some(editor.clone());
        Panel { editor }
    });
    let editor = built.unwrap();

    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    let offset = |cx: &mut VisualTestContext| cx.update(|_, cx| editor.read(cx).scroll_offset());
    draw(cx);
    assert_eq!(offset(cx).y, px(0.), "a fresh editor starts at the top");

    // The wheel leg: a wheel event delivered over the editor scrolls it.
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(50.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    draw(cx);
    assert!(
        offset(cx).y != px(0.),
        "a wheel event over the editor must scroll it"
    );

    // Back to the top, then the pan leg: a raw finger pan past the slop.
    cx.update(|_, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_scroll_offset(point(px(0.), px(0.)), cx)
        });
    });
    draw(cx);
    let touch = |phase, y| TouchEvent {
        id: TouchId(0),
        phase,
        position: point(px(50.), px(y)),
        predicted_position: None,
        force: None,
    };
    cx.simulate_event(touch(TouchPhase::Started, 30.));
    draw(cx);
    for y in [45., 70., 96.] {
        cx.simulate_event(touch(TouchPhase::Moved, y));
        draw(cx);
    }
    cx.simulate_event(touch(TouchPhase::Ended, 96.));
    draw(cx);
    assert!(
        offset(cx).y != px(0.),
        "a touch pan past the slop must scroll the script editor"
    );
}
