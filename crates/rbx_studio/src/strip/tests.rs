use super::*;

use rbx_dom::Variant;

use crate::layers;
use crate::notes;
use crate::save::Format;

fn find(dom: &WeakDom, name: &str) -> Ref {
    descendants(dom)
        .into_iter()
        .find(|&r| dom.get(r).is_some_and(|i| i.name() == name))
        .unwrap_or_else(|| panic!("no instance named {name}"))
}

fn sample() -> WeakDom {
    let mut dom = WeakDom::new();
    let ws = dom.new_instance("Workspace", "Workspace", None);
    dom.new_instance("ServerStorage", "ServerStorage", None);
    let keep = dom.new_instance("Part", "Keep", Some(ws));
    let only = dom.new_instance("Part", "OnlyLayer", Some(ws));
    let id = layers::add_layer(&mut dom, "Walls", (1, 2, 3)).expect("layer");
    layers::assign(&mut dom, &[keep, only], Some(&id)).expect("assign");
    put_attribute(&mut dom, keep, "Mine", Some(Variant::String("x".into()))).expect("attr");
    dom.set_property(keep, "Transparency", Variant::Float32(0.5))
        .expect("property");
    notes::add_note(
        &mut dom,
        "look here",
        "me",
        (9, 8, 7),
        [1.0, 2.0, 3.0],
        None,
    )
    .expect("note");
    dom
}

fn decode(dom: &WeakDom, format: Format) -> WeakDom {
    let bytes = format.encode(dom).expect("encode");
    match format {
        Format::Binary => rbx_binary::deserialize(&bytes).expect("binary decode"),
        Format::Xml => {
            rbx_xml::deserialize(std::str::from_utf8(&bytes).expect("utf8")).expect("xml decode")
        }
    }
}

/// A note as it reads back, referent aside: a decode numbers instances anew.
type NoteFields = (String, String, (u8, u8, u8), [f32; 3]);

fn note_fields(dom: &WeakDom) -> Vec<NoteFields> {
    notes::notes(dom)
        .into_iter()
        .map(|n| (n.text, n.author, n.color, n.position))
        .collect()
}

fn editor_attribute_count(dom: &WeakDom) -> usize {
    descendants(dom)
        .into_iter()
        .map(|r| {
            attributes(dom, r)
                .keys()
                .filter(|k| k.starts_with(layers::PREFIX))
                .count()
        })
        .sum()
}

#[test]
fn layers_membership_and_notes_survive_both_file_formats() {
    let dom = sample();
    for format in [Format::Binary, Format::Xml] {
        let back = decode(&dom, format);
        assert_eq!(layers::layers(&back), layers::layers(&dom));
        let id = layers::layers(&dom)[0].id.clone();
        assert_eq!(
            layers::layer_of(&back, find(&back, "Keep")).as_deref(),
            Some(id.as_str())
        );
        assert_eq!(note_fields(&back), note_fields(&dom));
    }
}

#[test]
fn a_stripped_copy_carries_no_editor_data_after_a_round_trip() {
    let clean = stripped(&sample());
    assert_eq!(editor_attribute_count(&clean), 0);
    assert!(notes::folder(&clean).is_none());
    for format in [Format::Binary, Format::Xml] {
        let back = decode(&clean, format);
        assert_eq!(editor_attribute_count(&back), 0);
        assert!(descendants(&back)
            .into_iter()
            .all(|r| back.get(r).is_some_and(|i| i.name() != notes::FOLDER_NAME)));
    }
}

#[test]
fn stripping_keeps_other_attributes_and_properties() {
    let clean = stripped(&sample());
    let keep = find(&clean, "Keep");
    assert!(attributes(&clean, keep).contains_key("Mine"));
    assert_eq!(
        clean
            .get(keep)
            .and_then(|i| i.properties().get("Transparency")),
        Some(&Variant::Float32(0.5))
    );
}

#[test]
fn stripped_leaves_the_source_untouched() {
    let dom = sample();
    let before = editor_attribute_count(&dom);
    assert!(before > 0);
    let _ = stripped(&dom);
    assert_eq!(editor_attribute_count(&dom), before);
    assert!(notes::folder(&dom).is_some());
}

#[test]
fn an_instance_whose_only_attribute_was_the_layer_loses_its_attribute_blob() {
    let clean = stripped(&sample());
    let only = find(&clean, "OnlyLayer");
    assert!(attributes(&clean, only).is_empty());
    assert!(!clean
        .get(only)
        .expect("instance")
        .properties()
        .contains_key("AttributesSerialize"));
}
