//! The pad on Windows, through the ViGEmBus driver: an Xbox 360 or a
//! DualShock 4 the client cannot tell from a plugged-in one.
//!
//! ViGEmBus is a kernel driver the user installs; its user-mode library,
//! `ViGEmClient.dll`, is not shipped (it is built from the MIT-licensed
//! ViGEmClient sources) but loaded at runtime from beside the executable,
//! the way the network simulator loads WinDivert. The report layouts and
//! constants below follow ViGEmClient's `Common.h` and `Client.h`; none of
//! it has been run on Windows.
//!
//! This file holds the pure parts (which target a device becomes, the
//! reports, error wording), compiled and unit-tested everywhere; the
//! Windows-only modules below do the calls.

// Only the Windows back end reads these; elsewhere they stay compiled (and
// tested) like the uinput layouts.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod ffi;
#[cfg(windows)]
mod pad;

#[cfg(windows)]
pub use pad::VirtualPad;

use super::{Button, Device, PadState, Side};

/// The library looked for next to the executable.
pub(crate) const DLL: &str = "ViGEmClient.dll";

/// What ViGEmBus can plug in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Xbox360,
    DualShock4,
}

/// The target standing in for `device`: ViGEmBus has no DualSense, so a
/// PS5 pad is a DualShock 4 (same buttons, its own button images in game).
pub(crate) fn target(device: Device) -> Option<Target> {
    match device {
        Device::Generic | Device::Xbox | Device::Handheld => Some(Target::Xbox360),
        Device::Ps4 | Device::Ps5 => Some(Target::DualShock4),
        Device::TvRemote | Device::Quest => None,
    }
}

/// `XUSB_REPORT`, the XInput gamepad state.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct XusbReport {
    pub(crate) buttons: u16,
    pub(crate) left_trigger: u8,
    pub(crate) right_trigger: u8,
    pub(crate) thumb_lx: i16,
    pub(crate) thumb_ly: i16,
    pub(crate) thumb_rx: i16,
    pub(crate) thumb_ry: i16,
}

/// `DS4_REPORT`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ds4Report {
    pub(crate) thumb_lx: u8,
    pub(crate) thumb_ly: u8,
    pub(crate) thumb_rx: u8,
    pub(crate) thumb_ry: u8,
    /// Face and shoulder buttons in the high bits, the D-pad's direction in
    /// the low nibble.
    pub(crate) buttons: u16,
    /// `DS4_SPECIAL_BUTTONS`: PS and the touchpad click.
    pub(crate) special: u8,
    pub(crate) trigger_l: u8,
    pub(crate) trigger_r: u8,
}

const _: () = assert!(std::mem::size_of::<XusbReport>() == 12);
const _: () = assert!(std::mem::size_of::<Ds4Report>() == 10);

/// `XUSB_BUTTON` bits.
const XUSB_BUTTONS: [(Button, u16); 15] = [
    (Button::DpadUp, 0x0001),
    (Button::DpadDown, 0x0002),
    (Button::DpadLeft, 0x0004),
    (Button::DpadRight, 0x0008),
    (Button::Start, 0x0010),
    (Button::Select, 0x0020),
    (Button::LeftStick, 0x0040),
    (Button::RightStick, 0x0080),
    (Button::LeftBumper, 0x0100),
    (Button::RightBumper, 0x0200),
    (Button::Home, 0x0400),
    (Button::South, 0x1000),
    (Button::East, 0x2000),
    (Button::West, 0x4000),
    (Button::North, 0x8000),
];

/// `DS4_BUTTONS` bits; the D-pad is not among them.
const DS4_BUTTONS: [(Button, u16); 10] = [
    (Button::West, 1 << 4),
    (Button::South, 1 << 5),
    (Button::East, 1 << 6),
    (Button::North, 1 << 7),
    (Button::LeftBumper, 1 << 8),
    (Button::RightBumper, 1 << 9),
    (Button::Select, 1 << 12),
    (Button::Start, 1 << 13),
    (Button::LeftStick, 1 << 14),
    (Button::RightStick, 1 << 15),
];
const DS4_TRIGGER_LEFT: u16 = 1 << 10;
const DS4_TRIGGER_RIGHT: u16 = 1 << 11;
const DS4_SPECIAL_PS: u8 = 1 << 0;
/// `DS4_BUTTON_DPAD_NONE`; north is 0, then clockwise to north-west, 7.
const DS4_DPAD_NONE: u16 = 0x8;

fn bits(state: &PadState, table: &[(Button, u16)]) -> u16 {
    table
        .iter()
        .filter(|(button, _)| state.pressed(*button))
        .fold(0, |all, (_, bit)| all | bit)
}

/// `-1..=1` to XInput's signed axis, y up as in [`PadState`].
fn xinput_axis(value: f32) -> i16 {
    (value.clamp(-1., 1.) * f32::from(i16::MAX)).round() as i16
}

/// `0..=1` to a byte.
fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

/// `-1..=1` to a DS4 axis byte, 0x80 at rest; pass `-y`, since its y grows
/// downwards.
fn ds4_axis(value: f32) -> u8 {
    (128. + value.clamp(-1., 1.) * 127.).round() as u8
}

/// The D-pad as a hat direction: opposite presses cancel out.
fn ds4_dpad(state: &PadState) -> u16 {
    let axis = |neg, pos| i8::from(state.pressed(pos)) - i8::from(state.pressed(neg));
    let x = axis(Button::DpadLeft, Button::DpadRight);
    let y = axis(Button::DpadUp, Button::DpadDown);
    match (x, y) {
        (0, -1) => 0,
        (1, -1) => 1,
        (1, 0) => 2,
        (1, 1) => 3,
        (0, 1) => 4,
        (-1, 1) => 5,
        (-1, 0) => 6,
        (-1, -1) => 7,
        _ => DS4_DPAD_NONE,
    }
}

pub(crate) fn xusb_report(state: &PadState) -> XusbReport {
    let (lx, ly) = state.stick(Side::Left);
    let (rx, ry) = state.stick(Side::Right);
    XusbReport {
        buttons: bits(state, &XUSB_BUTTONS),
        left_trigger: byte(state.trigger(Side::Left)),
        right_trigger: byte(state.trigger(Side::Right)),
        thumb_lx: xinput_axis(lx),
        thumb_ly: xinput_axis(ly),
        thumb_rx: xinput_axis(rx),
        thumb_ry: xinput_axis(ry),
    }
}

pub(crate) fn ds4_report(state: &PadState) -> Ds4Report {
    let (lx, ly) = state.stick(Side::Left);
    let (rx, ry) = state.stick(Side::Right);
    let (left, right) = (state.trigger(Side::Left), state.trigger(Side::Right));
    // A real DS4 also reports a pulled trigger as a button.
    let mut buttons = bits(state, &DS4_BUTTONS) | ds4_dpad(state);
    if left > 0. {
        buttons |= DS4_TRIGGER_LEFT;
    }
    if right > 0. {
        buttons |= DS4_TRIGGER_RIGHT;
    }
    Ds4Report {
        thumb_lx: ds4_axis(lx),
        thumb_ly: ds4_axis(-ly),
        thumb_rx: ds4_axis(rx),
        thumb_ry: ds4_axis(-ry),
        buttons,
        special: if state.pressed(Button::Home) {
            DS4_SPECIAL_PS
        } else {
            0
        },
        trigger_l: byte(left),
        trigger_r: byte(right),
    }
}

/// `VIGEM_ERROR_NONE`; every failure has the high nibble `0xE`.
pub(crate) const VIGEM_OK: u32 = 0x2000_0000;

/// Plain words for a `VIGEM_ERROR`.
pub(crate) fn error_text(code: u32) -> String {
    match code {
        0xE000_0001 => "the ViGEmBus driver isn't installed: install it from https://github.com/nefarius/ViGEmBus/releases, then try again".into(),
        0xE000_0002 => "ViGEmBus has no free slot for another virtual pad".into(),
        0xE000_0008 => "the installed ViGEmBus driver is a version ViGEmClient.dll doesn't speak: update both".into(),
        0xE000_0009 => "ViGEmBus is installed but couldn't be opened".into(),
        other => format!("ViGEmBus failed (error {other:#010x})"),
    }
}

/// Why `ViGEmClient.dll` could not be used: missing, or not ViGEmClient.
pub(crate) fn load_error_text(missing_export: bool) -> String {
    if missing_export {
        format!("{DLL} next to rbxstudio.exe isn't a ViGEmClient build exporting the vigem_* calls")
    } else {
        format!(
            "{DLL} is missing: build ViGEmClient (MIT, https://github.com/nefarius/ViGEmClient) as a DLL and put it next to rbxstudio.exe, with the ViGEmBus driver installed"
        )
    }
}

#[cfg(test)]
mod tests;
