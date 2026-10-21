//! Scrolls a panel so the field Tab just moved to is on screen.
//!
//! GPUI only autoscrolls a `List`; the Properties panel is a plain scrolling
//! `div`, so Tab walked on into rows below the fold and the focus ring went
//! with them (WCAG 2.4.11, Focus Not Obscured). A field reports where it was
//! painted while it holds keyboard focus ([`mark`]), and the panel scrolls
//! that box into view on the next frame, once per focus change
//! ([`follow`]), so a wheel scroll afterwards isn't fought.

use std::cell::{Cell, RefCell};

use gpui_kit::*;

thread_local! {
    /// The keyboard-focused field's box this frame.
    static FOCUSED: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The handle last scrolled to, so the panel scrolls once per stop.
    static REVEALED: RefCell<Option<FocusHandle>> = const { RefCell::new(None) };
}

/// A zero-size element for a field's box: records the box while `handle`
/// holds keyboard focus.
pub(super) fn mark(handle: &FocusHandle) -> impl IntoElement {
    let handle = handle.clone();
    canvas(
        move |bounds, window, cx| {
            if handle.contains_focused(window, cx) && window.last_input_was_keyboard() {
                FOCUSED.set(Some(bounds));
                if REVEALED.with_borrow(|seen| seen.as_ref() != Some(&handle)) {
                    REVEALED.set(Some(handle.clone()));
                    // The panel's `follow` painted ahead of its rows, so it
                    // acts on this on the next frame.
                    window.refresh();
                } else {
                    // Already revealed: leave the scroll to the user.
                    FOCUSED.set(None);
                }
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// A zero-size element in a panel's scrolling body: scrolls `scroll` so the
/// box [`mark`] last recorded is inside its viewport.
pub(super) fn follow(scroll: &ScrollHandle) -> impl IntoElement {
    let scroll = scroll.clone();
    canvas(
        move |_, window, _| {
            let Some(field) = FOCUSED.take() else {
                return;
            };
            let view = scroll.bounds();
            let offset = scroll.offset();
            let shift = into_view(view.top(), view.bottom(), field.top(), field.bottom());
            if shift != px(0.) {
                let max = scroll.max_offset().y;
                let y = (offset.y - shift).clamp(-max, px(0.));
                scroll.set_offset(point(offset.x, y));
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// How far down (positive) or up the content has to move for the field to
/// sit inside the view, with a little margin; zero when it already does.
fn into_view(top: Pixels, bottom: Pixels, field_top: Pixels, field_bottom: Pixels) -> Pixels {
    const MARGIN: Pixels = px(8.);
    if field_bottom > bottom {
        field_bottom - bottom + MARGIN
    } else if field_top < top {
        field_top - top - MARGIN
    } else {
        px(0.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn a_field_below_the_fold_scrolls_up_into_view_and_one_inside_stays() {
        assert_eq!(into_view(px(0.), px(100.), px(120.), px(150.)), px(58.));
        assert_eq!(into_view(px(0.), px(100.), px(-40.), px(-10.)), px(-48.));
        assert_eq!(into_view(px(0.), px(100.), px(20.), px(50.)), px(0.));
    }
}
