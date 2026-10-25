use super::{find, names, KEY_CODES};
use std::collections::HashSet;

#[test]
fn table_has_every_name() {
    assert_eq!(KEY_CODES.len(), 283);
}

#[test]
fn names_are_unique() {
    let unique: HashSet<&str> = names().collect();
    assert_eq!(unique.len(), KEY_CODES.len());
}

#[test]
fn letter_maps_to_ascii_and_vk() {
    let w = find("W").unwrap();
    assert_eq!(w.keysym, 0x77);
    assert_eq!(w.vk, 0x57);
    assert!(!w.extended);
}

#[test]
fn arrow_is_extended_and_typeable() {
    let left = find("Left").unwrap();
    assert!(left.extended);
    assert!(left.typeable());
}

#[test]
fn gamepad_button_is_not_typeable() {
    assert!(!find("ButtonA").unwrap().typeable());
}

#[test]
fn unknown_name_is_none() {
    assert!(find("Banana").is_none());
}

#[test]
fn first_name_is_none_key() {
    assert_eq!(names().next(), Some("None"));
}
