use rbx_dom::{Content, Variant, WeakDom};

use super::of;

#[test]
fn only_a_wrap_with_data_or_an_asset_has_hidden_surfaces() {
    let mut dom = WeakDom::new();
    let wrap = dom.new_instance("WrapTarget", "UpperTorsoWrapTarget", None);
    let empty = Variant::Unknown {
        type_id: 0x1c,
        raw: Vec::new(),
    };
    dom.set_property(wrap, "HSRData", empty).unwrap();
    dom.set_property(
        wrap,
        "HSRAssetId",
        Variant::Content(Content::Uri(String::new())),
    )
    .unwrap();
    assert!(of(dom.get(wrap).unwrap()).is_empty());
    let asset = Variant::Content(Content::Uri("rbxassetid://13251297563".into()));
    dom.set_property(wrap, "HSRAssetId", asset.clone()).unwrap();
    let props = of(dom.get(wrap).unwrap());
    assert!(props.contains(&("HSRAssetId".to_string(), asset)));
}

#[test]
fn every_stock_part_with_a_cage_has_its_hidden_surface_asset() {
    use super::super::bundle::{stock as stock_pieces, Family};
    use super::super::BodyShape;
    for (family, shape) in [
        (Family::Classic, BodyShape::Masculine),
        (Family::Classic, BodyShape::Feminine),
        (Family::Mannequin, BodyShape::Masculine),
    ] {
        for piece in stock_pieces(family, shape)
            .values()
            .filter(|p| p.cage.is_some())
        {
            assert!(
                super::stock(piece.mesh).is_some(),
                "{family:?} {}",
                piece.name
            );
        }
    }
    // The Mannequin's UpperTorso, as the reference avatar names it.
    assert_eq!(super::stock(70696180344244), Some(100231335472912));
}
