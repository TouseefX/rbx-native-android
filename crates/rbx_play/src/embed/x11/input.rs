//! Pointer and key events into the embedded client through XTest: real
//! device events, which the client reads exactly as it reads a person's
//! mouse and keyboard (it drops `SendEvent` ones).

use std::thread;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, InputFocus, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT,
    KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::keys::{self, find_keysym, Key};
use super::x11;
use crate::embed::{Button, EmbedError, WindowId, CLICK_HOLD};
use crate::keycode::KeyCode;

const SHIFT_KEYSYM: u32 = 0xffe1;
/// X11's wheel: buttons 4 (up) and 5 (down), one notch per press.
const WHEEL_UP: u8 = 4;
const WHEEL_DOWN: u8 = 5;

/// One run of input into a client window, on its own connection.
pub struct Input {
    conn: RustConnection,
    root: u32,
    window: WindowId,
}

impl Input {
    pub fn open(window: WindowId) -> Result<Self, EmbedError> {
        let (conn, root) = super::connect()?;
        Ok(Input { conn, root, window })
    }

    /// The window's size in pixels.
    pub fn size(&self) -> Result<(u32, u32), EmbedError> {
        let geometry = self
            .conn
            .get_geometry(self.window)
            .map_err(x11)?
            .reply()
            .map_err(x11)?;
        Ok((u32::from(geometry.width), u32::from(geometry.height)))
    }

    /// Moves the pointer to `(x, y)` in the window's pixels, as a motion
    /// event (the client tracks hover from those, not from warps), and gives
    /// the window the focus: an unfocused client takes the next press at a
    /// stale position, before the editor's click-to-focus reaches it.
    pub fn move_to(&mut self, x: i32, y: i32) -> Result<(), EmbedError> {
        let clamp = |v: i32| i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX });
        let at = self
            .conn
            .translate_coordinates(self.window, self.root, clamp(x), clamp(y))
            .map_err(x11)?
            .reply()
            .map_err(x11)?;
        self.conn
            .xtest_fake_input(
                MOTION_NOTIFY_EVENT,
                0,
                x11rb::CURRENT_TIME,
                self.root,
                at.dst_x,
                at.dst_y,
                0,
            )
            .map_err(x11)?;
        self.focus()?;
        self.sync()
    }

    pub fn button(&mut self, button: Button, down: bool) -> Result<(), EmbedError> {
        let detail = match button {
            Button::Left => 1,
            Button::Right => 3,
        };
        self.fake(
            if down {
                BUTTON_PRESS_EVENT
            } else {
                BUTTON_RELEASE_EVENT
            },
            detail,
        )?;
        self.sync()
    }

    /// One wheel notch where the pointer is.
    pub fn scroll(&mut self, up: bool) -> Result<(), EmbedError> {
        let detail = if up { WHEEL_UP } else { WHEEL_DOWN };
        self.fake(BUTTON_PRESS_EVENT, detail)?;
        self.sync()?;
        thread::sleep(CLICK_HOLD);
        self.fake(BUTTON_RELEASE_EVENT, detail)?;
        self.sync()
    }

    /// Presses or releases the key that types `key` on the current layout,
    /// with Shift around it when the keysym is on the shifted level.
    pub fn key(&mut self, key: &KeyCode, down: bool) -> Result<(), EmbedError> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let map = self
            .conn
            .get_keyboard_mapping(min, max - min + 1)
            .map_err(x11)?
            .reply()
            .map_err(x11)?;
        let per = usize::from(map.keysyms_per_keycode.max(1));
        let Some((code, shifted)) = find_keysym(&map.keysyms, per, min, key.keysym) else {
            return Err(EmbedError::X11(format!(
                "no key on this keyboard layout types {}",
                key.name
            )));
        };
        self.take_keyboard()?;
        let shift = find_keysym(&map.keysyms, per, min, SHIFT_KEYSYM)
            .map(|(code, _)| code)
            .filter(|_| shifted);
        let kind = if down {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        };
        if let (Some(shift), true) = (shift, down) {
            self.fake(KEY_PRESS_EVENT, shift)?;
        }
        self.fake(kind, code)?;
        if let (Some(shift), true) = (shift, down) {
            self.fake(KEY_RELEASE_EVENT, shift)?;
        }
        self.sync()
    }

    /// Types `text` as key presses, for a text field a game script cannot
    /// reach (the chat bar), on the typing thread `keys` keeps.
    pub fn type_text(&mut self, text: &str) -> Result<(), EmbedError> {
        let typed: Vec<Key> = text
            .chars()
            .map(|c| if c == '\n' { Key::Return } else { Key::Char(c) })
            .collect();
        keys::type_into(self.window, typed)
    }

    /// The client reads the keyboard only while it has the focus and the
    /// pointer is over it (`keys`): the pointer stays where the mouse left
    /// it when that is over the window, and goes to its middle otherwise.
    fn take_keyboard(&mut self) -> Result<(), EmbedError> {
        let pointer = self
            .conn
            .query_pointer(self.window)
            .map_err(x11)?
            .reply()
            .map_err(x11)?;
        let (width, height) = self.size()?;
        let inside = pointer.same_screen
            && (0..i32::try_from(width).unwrap_or(i32::MAX)).contains(&i32::from(pointer.win_x))
            && (0..i32::try_from(height).unwrap_or(i32::MAX)).contains(&i32::from(pointer.win_y));
        if inside {
            self.focus()
        } else {
            let half = |extent: u32| i32::try_from(extent / 2).unwrap_or(0);
            self.move_to(half(width), half(height))
        }
    }

    fn focus(&self) -> Result<(), EmbedError> {
        self.conn
            .set_input_focus(InputFocus::PARENT, self.window, x11rb::CURRENT_TIME)
            .map_err(x11)?;
        Ok(())
    }

    fn fake(&self, kind: u8, detail: u8) -> Result<(), EmbedError> {
        self.conn
            .xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, 0, 0, 0)
            .map_err(x11)?;
        Ok(())
    }

    fn sync(&self) -> Result<(), EmbedError> {
        self.conn.sync().map_err(x11)
    }
}
