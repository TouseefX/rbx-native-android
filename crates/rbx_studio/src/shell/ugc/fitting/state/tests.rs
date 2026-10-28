use super::*;

#[test]
fn studio_lists_ten_accessory_types_and_nine_clothing_entries() {
    assert_eq!(Mode::Accessory.types().len(), 10);
    assert!(Mode::Accessory.types().contains(&AssetType::Eyebrow));
    assert_eq!(Mode::Clothing.types().len(), 9);
    assert!(!Mode::Clothing.types().contains(&AssetType::Eyebrow));
}

#[test]
fn a_new_type_keeps_an_allowed_attachment_and_resets_the_fit() {
    let mut s = Settings::default();
    assert_eq!(s.attachment, "HatAttachment");
    s.set_asset(AssetType::Shoulder);
    assert_eq!(s.attachment, "RightShoulderAttachment");
    s.attachment = "NeckAttachment";
    s.placement.offset = [1.0, 0.0, 0.0];
    s.set_asset(AssetType::Neck);
    assert_eq!(s.attachment, "NeckAttachment");
    assert_eq!(s.placement, Placement::default());
}

#[test]
fn switching_mode_picks_that_modes_default_type() {
    let mut s = Settings::default();
    s.set_mode(Mode::Clothing);
    assert_eq!(s.asset, AssetType::Jacket);
    assert_eq!(s.attachment, "BodyFrontAttachment");
    assert!(s.layered());
    s.set_mode(Mode::Accessory);
    assert_eq!(s.asset, AssetType::Hat);
}

#[test]
fn shoes_sit_on_their_own_foot() {
    assert_eq!(attachments(AssetType::LeftShoe), ["LeftFootAttachment"]);
    assert_eq!(layered_part(AssetType::RightShoe), "RightFoot");
    assert_eq!(layered_part(AssetType::Pants), "LowerTorso");
    assert_eq!(layered_part(AssetType::Jacket), "UpperTorso");
}

#[test]
fn attachment_labels_read_like_studios_picker() {
    assert_eq!(
        attachment_label("RightShoulderAttachment"),
        "Right Shoulder"
    );
    assert_eq!(attachment_label("NeckAttachment"), "Neck");
}

#[test]
fn placement_rotates_like_cframe_angles() {
    let p = Placement {
        offset: [1.0, 2.0, 3.0],
        rotation: [0.0, 90.0, 0.0],
        scale: 1.0,
    };
    let f = p.frame();
    assert_eq!(f.p, [1.0, 2.0, 3.0]);
    // CFrame.Angles(0, pi/2, 0) turns +X to -Z.
    let x = f.rotate([1.0, 0.0, 0.0]);
    assert!((x[2] + 1.0).abs() < 1e-5 && x[0].abs() < 1e-5, "{x:?}");
    assert_eq!(Placement::default().frame(), Cf::at([0.0; 3]));
}

#[test]
fn only_the_item_step_is_open_without_an_item() {
    assert!(Step::Item.reachable(false));
    assert!(!Step::Fit.reachable(false));
    assert!(Step::Generate.reachable(true));
}

#[test]
fn auto_skin_round_trips_its_enum_value() {
    for a in AutoSkin::ALL {
        assert_eq!(AutoSkin::from_value(a.value()), a);
    }
    assert_eq!(AutoSkin::EnabledOverride.value(), 2);
}
