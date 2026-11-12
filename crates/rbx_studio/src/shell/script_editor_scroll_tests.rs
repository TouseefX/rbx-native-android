//! The script editor's touch panning, pinned end to end under GPUI's headless
//! window.
//!
//! On device a plain finger pan over the code editor does not scroll it,
//! while a selection drag's auto-scroll and every scrollable do scroll — so
//! the loss is somewhere between the raw touch and the editor's
//! `on_scroll_wheel`. This renders the very widget `shell::script_panel`
//! renders — the kit `Editor`, in the panel's own wrapper, focused the way a
//! script being edited is — and walks the whole path in one pass, collecting
//! what each step left behind before judging any of them, so one failing run
//! reports the full picture:
//!
//! 1. an offset set directly on the state must survive a draw (the scroll
//!    range is live and nothing in prepaint fights it);
//! 2. a `ScrollWheelEvent` over the editor must scroll it (the listener leg
//!    — the root's counter says whether the event even bubbled past);
//! 3. a raw `TouchEvent` sequence past the touch slop must scroll it (the
//!    recognizer leg).

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    div, point, px, relative, AppContext as _, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, ScrollDelta, ScrollWheelEvent, Styled as _,
    TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window,
};

const LINES: usize = 400;

/// The panel's own wrapper around the editor, as `shell::script_panel`
/// builds it: a relative, flex-1, overflow-hidden hole the editor fills.
/// The root counts wheel events that reached it, which only happens when
/// nothing on the way consumed them.
struct Panel {
    editor: Entity<EditorState>,
    wheeled: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wheeled = self.wheeled.clone();
        v_flex()
            .size_full()
            .on_scroll_wheel(move |_, _, _| {
                wheeled.set(wheeled.get() + 1);
            })
            .child(
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
    let wheeled = Rc::new(Cell::new(0));
    let mut built = None;
    let (_panel, cx) = cx.add_window_view(|window, cx| {
        let editor =
            cx.new(|cx| EditorState::new(window, cx).default_value("local x = 1\n".repeat(LINES)));
        built = Some(editor.clone());
        Panel {
            editor,
            wheeled: wheeled.clone(),
        }
    });
    let editor = built.unwrap();

    let draw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    };
    let offset = |cx: &mut VisualTestContext| cx.update(|_, cx| editor.read(cx).scroll_offset());
    let rows = |cx: &mut VisualTestContext| cx.update(|_, cx| editor.read(cx).visible_row_range());
    let caret = |cx: &mut VisualTestContext| cx.update(|_, cx| editor.read(cx).cursor_layout());
    let to_top = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_scroll_offset(point(px(0.), px(0.)), cx)
            });
        });
        draw(cx);
    };

    // The editor is focused, the way a script being edited is on device.
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| editor.focus(window, cx));
    });
    draw(cx);
    let rows_before = rows(cx);
    let bounds = cx.update(|_, cx| editor.read(cx).input_bounds());
    assert!(
        rows_before.as_ref().is_some_and(|rows| rows.end < LINES),
        "{LINES} lines must overflow the 1920x1080 test window: rows={rows_before:?} \
         bounds={bounds:?}"
    );

    // Leg one: an offset set directly on the state, through one draw and
    // then a second (a value that only survives the second draw is its own
    // answer).
    cx.update(|_, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_scroll_offset(point(px(0.), px(-40.)), cx)
        });
    });
    let set_before_draw = offset(cx);
    draw(cx);
    let set_after_draw = offset(cx);
    let rows_after_set = rows(cx);
    draw(cx);
    let set_after_second_draw = offset(cx);

    // Leg two: a wheel event over the editor.
    to_top(cx);
    let wheel_from = offset(cx);
    wheeled.set(0);
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(50.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    draw(cx);
    let wheel_after = offset(cx);
    let wheel_root_saw = wheeled.get();

    // Leg three: a raw finger pan past the slop.
    to_top(cx);
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
    let pan_after = offset(cx);
    let pan_rows = rows(cx);
    let pan_root_saw = wheeled.get();

    fn show(offset_y: Pixels) -> String {
        format!("{offset_y:?}")
    }
    assert!(
        set_after_draw.y == px(-40.) && wheel_after.y != px(0.) && pan_after.y != px(0.),
        "the script editor does not scroll: a directly-set -40px read {} before the draw, \
         {} after it and {} after a second (rows {rows_before:?} → {rows_after_set:?}, caret \
         {:?}); a wheel event left {} (from {}, {} events bubbled past the root); a touch pan \
         left {} (rows {pan_rows:?}, {} events past the root). bounds={bounds:?}",
        show(set_before_draw.y),
        show(set_after_draw.y),
        show(set_after_second_draw.y),
        caret(cx),
        show(wheel_after.y),
        show(wheel_from.y),
        wheel_root_saw,
        show(pan_after.y),
        pan_root_saw,
    );
}
