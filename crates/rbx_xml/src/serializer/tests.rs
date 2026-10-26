use rbx_dom::{Content, Instance, Ref, Variant, Vector3Data, WeakDom};

use super::serialize;
use crate::error::XmlError;

#[test]
fn empty_dom_serializes_to_just_the_root_and_meta() {
    let dom = WeakDom::new();
    let xml = serialize(&dom).unwrap();
    assert_eq!(
        xml,
        "<roblox version=\"4\">\n  <Meta name=\"ExplicitAutoJoints\">true</Meta>\n</roblox>\n"
    );
}

#[test]
fn single_instance_round_trips_class_name_and_a_property() {
    let mut dom = WeakDom::new();
    let part = dom.new_instance("Part", "Baseplate", None);
    dom.set_property(
        part,
        "size",
        Variant::Vector3(Vector3Data {
            x: 512.0,
            y: 1.2,
            z: 512.0,
        }),
    )
    .unwrap();

    let xml = serialize(&dom).unwrap();
    let round_tripped = crate::deserialize(&xml).unwrap();

    let roots = round_tripped.root_refs();
    assert_eq!(roots.len(), 1);
    let got = round_tripped.get(roots[0]).unwrap();
    assert_eq!(got.class(), "Part");
    assert_eq!(got.name(), "Baseplate");
    assert_eq!(
        got.properties().get("size"),
        Some(&Variant::Vector3(Vector3Data {
            x: 512.0,
            y: 1.2,
            z: 512.0
        }))
    );
}

#[test]
fn nested_children_and_a_ref_property_round_trip() {
    let mut dom = WeakDom::new();
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let part_a = dom.new_instance("Part", "A", Some(workspace));
    let part_b = dom.new_instance("Part", "B", Some(workspace));
    dom.set_property(part_a, "Target", Variant::Ref(part_b))
        .unwrap();
    dom.set_property(part_a, "Source", Variant::Content(Content::Object(part_b)))
        .unwrap();

    let xml = serialize(&dom).unwrap();
    let round_tripped = crate::deserialize(&xml).unwrap();

    let ws = round_tripped.get(round_tripped.root_refs()[0]).unwrap();
    assert_eq!(ws.children().len(), 2);
    let a = round_tripped
        .get(ws.children()[0])
        .filter(|i| i.name() == "A")
        .or_else(|| round_tripped.get(ws.children()[1]))
        .unwrap();
    let target_ref = match a.properties().get("Target") {
        Some(Variant::Ref(r)) => *r,
        other => panic!("expected a Ref property, got {other:?}"),
    };
    let target = round_tripped.get(target_ref).unwrap();
    assert_eq!(target.name(), "B");
    assert_eq!(
        a.properties().get("Source"),
        Some(&Variant::Content(Content::Object(target_ref)))
    );
}

fn only_root(dom: &WeakDom) -> &Instance {
    dom.get(dom.root_refs()[0]).unwrap()
}

#[test]
fn a_content_id_goes_out_as_a_url_whether_held_as_string_or_content() {
    // A file loads `Decal.Texture` as a String, a script or Properties
    // edit sets it as a Content; Studio drops it unless it is a `url`.
    for texture in [
        Variant::String("rbxassetid://1".into()),
        Variant::Content(Content::Uri("rbxassetid://1".into())),
    ] {
        let mut dom = WeakDom::new();
        let decal = dom.new_instance("Decal", "Decal", None);
        dom.set_property(decal, "Texture", texture).unwrap();

        let xml = serialize(&dom).unwrap();
        assert!(xml.contains(
            "<Content name=\"Texture\">\n        <url>rbxassetid://1</url>\n      </Content>"
        ));
        let back = crate::deserialize(&xml).unwrap();
        assert_eq!(
            only_root(&back).properties().get("Texture"),
            Some(&Variant::String("rbxassetid://1".into()))
        );
    }
}

#[test]
fn an_empty_content_id_goes_out_as_null_and_reads_back_empty() {
    let mut dom = WeakDom::new();
    let label = dom.new_instance("ImageLabel", "Label", None);
    dom.set_property(label, "Image", Variant::Content(Content::None))
        .unwrap();

    let xml = serialize(&dom).unwrap();
    assert!(xml.contains("<Content name=\"Image\">\n        <null></null>"));
    let back = crate::deserialize(&xml).unwrap();
    assert_eq!(
        only_root(&back).properties().get("Image"),
        Some(&Variant::String(String::new()))
    );
}

#[test]
fn a_content_held_as_a_string_goes_out_as_a_uri() {
    let mut dom = WeakDom::new();
    let label = dom.new_instance("ImageLabel", "Label", None);
    dom.set_property(
        label,
        "ImageContent",
        Variant::String("rbxassetid://3".into()),
    )
    .unwrap();

    let xml = serialize(&dom).unwrap();
    assert!(xml.contains("<uri>rbxassetid://3</uri>"));
    let back = crate::deserialize(&xml).unwrap();
    assert_eq!(
        only_root(&back).properties().get("ImageContent"),
        Some(&Variant::Content(Content::Uri("rbxassetid://3".into())))
    );
}

#[test]
fn script_source_goes_out_as_a_protected_string() {
    let source = Variant::String("if a < b and c > d then print(\"&]]>\") end\n".into());
    let mut dom = WeakDom::new();
    let script = dom.new_instance("Script", "Script", None);
    dom.set_property(script, "Source", source.clone()).unwrap();

    let xml = serialize(&dom).unwrap();
    assert!(xml.contains("<ProtectedString name=\"Source\">if a &lt; b"));
    assert!(!xml.contains("<string name=\"Source\">"));
    let back = crate::deserialize(&xml).unwrap();
    assert_eq!(only_root(&back).properties().get("Source"), Some(&source));
}

#[test]
fn binary_string_properties_held_as_text_go_out_as_base64() {
    let mut dom = WeakDom::new();
    let part = dom.new_instance("Part", "Part", None);
    let tags = Variant::String("Coin".into());
    dom.set_property(part, "Tags", tags.clone()).unwrap();
    let terrain = dom.new_instance("Terrain", "Terrain", None);
    let colors = Variant::String("abc".into());
    dom.set_property(terrain, "MaterialColors", colors.clone())
        .unwrap();

    let xml = serialize(&dom).unwrap();
    assert!(xml.contains("<BinaryString name=\"Tags\">Q29pbg==</BinaryString>"));
    assert!(xml.contains("<BinaryString name=\"MaterialColors\">YWJj</BinaryString>"));
    let back = crate::deserialize(&xml).unwrap();
    let got = |name: &str, key: &str| {
        back.root_refs()
            .iter()
            .find_map(|&r| back.get(r).filter(|i| i.name() == name))
            .and_then(|i| i.properties().get(key).cloned())
    };
    assert_eq!(got("Part", "Tags"), Some(tags));
    assert_eq!(got("Terrain", "MaterialColors"), Some(colors));
}

#[test]
fn a_string_xml_cannot_hold_goes_out_as_binary_and_reads_back_the_same() {
    let mut dom = WeakDom::new();
    let terrain = dom.new_instance("Terrain", "Terrain", None);
    let grid = Variant::String("\u{2}\u{3}grid\tok".into());
    dom.set_property(terrain, "PhysicsGrid", grid.clone())
        .unwrap();

    let xml = serialize(&dom).unwrap();
    assert!(xml.contains("<BinaryString name=\"PhysicsGrid\">"));
    assert!(!xml.contains('\u{2}'));

    let back = crate::deserialize(&xml).unwrap();
    let root = back.root_refs()[0];
    assert_eq!(
        back.get(root).unwrap().properties().get("PhysicsGrid"),
        Some(&grid)
    );
}

#[test]
fn shared_strings_round_trip_through_one_deduplicated_table() {
    let mut dom = WeakDom::new();
    let mesh = Variant::Unknown {
        type_id: 0x1C,
        raw: vec![0xFF, 1, 2],
    };
    let a = dom.new_instance("Model", "A", None);
    let b = dom.new_instance("Model", "B", None);
    dom.set_property(a, "ModelMeshData", mesh.clone()).unwrap();
    dom.set_property(b, "ModelMeshData", mesh.clone()).unwrap();

    let xml = serialize(&dom).unwrap();
    assert_eq!(xml.matches("<SharedString md5=").count(), 1);

    let back = crate::deserialize(&xml).unwrap();
    for &root in back.root_refs() {
        let got = back.get(root).unwrap().properties().get("ModelMeshData");
        assert_eq!(got, Some(&mesh));
    }
}

#[test]
fn non_utf8_unknown_blob_round_trips_through_binary_string() {
    let mut dom = WeakDom::new();
    dom.insert(Instance::new(Ref::new(1), "Terrain", "Terrain"));
    let raw = vec![0xFF, 0xFE, 0x00, 0x80];
    dom.set_property(
        Ref::new(1),
        "SmoothGrid",
        Variant::Unknown {
            type_id: 1,
            raw: raw.clone(),
        },
    )
    .unwrap();

    let xml = serialize(&dom).unwrap();
    let round_tripped = crate::deserialize(&xml).unwrap();
    let got = round_tripped.get(round_tripped.root_refs()[0]).unwrap();
    assert_eq!(
        got.properties().get("SmoothGrid"),
        Some(&Variant::Unknown { type_id: 1, raw })
    );
}

#[test]
fn instances_of_the_same_class_may_disagree_on_which_properties_are_set() {
    // Unlike `rbx_binary` (one PROP chunk per property, shared across every
    // instance of a class), XML writes each `Item`'s properties independently,
    // so a property present on one instance and absent on another of the same
    // class is not a format-level inconsistency here.
    let mut dom = WeakDom::new();
    let loaded = dom.new_instance("Part", "Loaded", None);
    dom.set_property(loaded, "Transparency", Variant::Float32(0.5))
        .unwrap();
    dom.new_instance("Part", "Bare", None);

    let xml = serialize(&dom).unwrap();
    let round_tripped = crate::deserialize(&xml).unwrap();

    let bare = round_tripped
        .root_refs()
        .iter()
        .find_map(|&r| round_tripped.get(r).filter(|i| i.name() == "Bare"))
        .expect("the bare instance round-trips");
    assert_eq!(bare.class(), "Part");
    assert!(
        bare.properties().get("Transparency").is_none(),
        "XML has no need to invent a value for a property the instance never had"
    );
}

#[test]
fn unresolved_shared_string_index_is_reported_as_unsupported() {
    let mut dom = WeakDom::new();
    dom.insert(Instance::new(Ref::new(1), "Script", "A"));
    dom.set_property(Ref::new(1), "Source", Variant::SharedString(4))
        .unwrap();

    assert!(matches!(
        serialize(&dom),
        Err(XmlError::Unsupported("SharedString"))
    ));
}
