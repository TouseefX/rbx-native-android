use super::*;

fn with(buttons: &[Button]) -> PadState {
    let mut state = PadState::default();
    for button in buttons {
        state.set(*button, true);
    }
    state
}

#[test]
fn devices_map_to_targets() {
    assert_eq!(target(Device::Xbox), Some(Target::Xbox360));
    assert_eq!(target(Device::Generic), Some(Target::Xbox360));
    assert_eq!(target(Device::Handheld), Some(Target::Xbox360));
    assert_eq!(target(Device::Ps4), Some(Target::DualShock4));
    assert_eq!(target(Device::Ps5), Some(Target::DualShock4));
    assert_eq!(target(Device::Quest), None);
    assert_eq!(target(Device::TvRemote), None);
}

#[test]
fn rest_reports() {
    assert_eq!(xusb_report(&PadState::default()), XusbReport::default());
    let ds4 = ds4_report(&PadState::default());
    assert_eq!(
        (ds4.thumb_lx, ds4.thumb_ly, ds4.thumb_rx, ds4.thumb_ry),
        (128, 128, 128, 128)
    );
    assert_eq!(ds4.buttons, DS4_DPAD_NONE);
    assert_eq!((ds4.special, ds4.trigger_l, ds4.trigger_r), (0, 0, 0));
}

#[test]
fn xusb_face_buttons_are_positional() {
    assert_eq!(xusb_report(&with(&[Button::South])).buttons, 0x1000);
    assert_eq!(xusb_report(&with(&[Button::West])).buttons, 0x4000);
    assert_eq!(xusb_report(&with(&[Button::North])).buttons, 0x8000);
    assert_eq!(
        xusb_report(&with(&[Button::Home, Button::DpadLeft])).buttons,
        0x0404
    );
}

#[test]
fn xusb_axes_keep_y_up_and_full_range() {
    let mut state = PadState::default();
    state.set_stick(Side::Left, 1., -1.);
    state.set_stick(Side::Right, -0.5, 0.5);
    state.set_trigger(Side::Right, 1.);
    let report = xusb_report(&state);
    assert_eq!((report.thumb_lx, report.thumb_ly), (32767, -32767));
    assert_eq!((report.thumb_rx, report.thumb_ry), (-16384, 16384));
    assert_eq!((report.left_trigger, report.right_trigger), (0, 255));
}

#[test]
fn ds4_y_grows_down() {
    let mut state = PadState::default();
    state.set_stick(Side::Left, -1., 1.);
    let report = ds4_report(&state);
    assert_eq!((report.thumb_lx, report.thumb_ly), (1, 1));
    state.set_stick(Side::Left, 1., -1.);
    let report = ds4_report(&state);
    assert_eq!((report.thumb_lx, report.thumb_ly), (255, 255));
}

#[test]
fn ds4_buttons_triggers_and_ps() {
    let mut state = with(&[Button::South, Button::Home]);
    state.set_trigger(Side::Left, 0.5);
    let report = ds4_report(&state);
    assert_eq!(report.buttons, (1 << 5) | DS4_TRIGGER_LEFT | DS4_DPAD_NONE);
    assert_eq!(report.special, DS4_SPECIAL_PS);
    assert_eq!(report.trigger_l, 128);
}

#[test]
fn ds4_dpad_directions() {
    assert_eq!(ds4_report(&with(&[Button::DpadUp])).buttons, 0);
    assert_eq!(
        ds4_report(&with(&[Button::DpadUp, Button::DpadRight])).buttons,
        1
    );
    assert_eq!(ds4_report(&with(&[Button::DpadDown])).buttons, 4);
    assert_eq!(
        ds4_report(&with(&[Button::DpadLeft, Button::DpadUp])).buttons,
        7
    );
    assert_eq!(
        ds4_report(&with(&[Button::DpadLeft, Button::DpadRight])).buttons,
        DS4_DPAD_NONE
    );
}

#[test]
fn error_texts() {
    assert!(error_text(0xE000_0001).contains("isn't installed"));
    assert!(error_text(0xE000_0002).contains("free slot"));
    assert!(error_text(0xE000_0008).contains("update both"));
    assert_eq!(
        error_text(0xE000_0099),
        "ViGEmBus failed (error 0xe0000099)"
    );
    assert!(load_error_text(false).contains("is missing"));
    assert!(load_error_text(true).contains("vigem_*"));
}
