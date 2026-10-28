//! Keeping the client's gamepads live while another window of the editor
//! holds the keyboard: the Controller Emulator's.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, EventMask, FocusInEvent, NotifyDetail, NotifyMode, FOCUS_IN_EVENT,
};

use super::{x11, Embedded};
use crate::embed::EmbedError;

impl Embedded {
    /// Tells the client it has the keyboard focus without moving it there:
    /// a synthetic `FocusIn` on its window.
    ///
    /// SDL, inside Sober, drops a gamepad's presses (and its sticks leaving
    /// rest) while its window lacks the focus, and Sober does not honour
    /// SDL's `SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS` (both measured live).
    /// The Controller Emulator needs the real focus for its keyboard
    /// bindings, so it sends this before each change of the pad instead.
    /// SDL takes a sent `FocusIn` as a real one; the next real `FocusOut`
    /// or `FocusIn` puts things back as they were, and the client gets no
    /// keys meanwhile, since the server still delivers them to the window
    /// that has the focus.
    pub fn lend_focus(&self) -> Result<(), EmbedError> {
        let event = FocusInEvent {
            response_type: FOCUS_IN_EVENT,
            // `Nonlinear`, as from an unrelated window: SDL ignores the
            // `Inferior` and `Pointer` details, and `Grab` modes.
            detail: NotifyDetail::NONLINEAR,
            sequence: 0,
            event: self.window,
            mode: NotifyMode::NORMAL,
        };
        self.conn
            .send_event(false, self.window, EventMask::FOCUS_CHANGE, event)
            .map_err(x11)?;
        self.conn.flush().map_err(x11)
    }
}
