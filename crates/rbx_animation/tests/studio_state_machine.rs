//! A state machine saved by Roblox Studio (`AnimationGraphDefinition` "Graph":
//! a `StateMachineNode` with states State and State1 (the entry is State),
//! feeding the output). The writer must produce the bytes Studio did for
//! `EntryState` and `InputPinData`. The `.rbxmx` is the same graph saved by
//! Studio into an XML place, where `EntryState` is a `__attrRef_EntryState`
//! Ref property rather than part of `AttributesSerialize`.

use rbx_animation::graph::dom::{read, write};
use rbx_animation::graph::model::{NodeKind, Prop, Value};
use rbx_dom::{Ref, Variant, WeakDom};

fn fixture(extension: &str) -> Vec<u8> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!(
        "{manifest}/tests/fixtures/studio_state_machine.{extension}"
    ))
    .unwrap()
}

fn load() -> (WeakDom, Ref) {
    let dom = rbx_binary::deserialize(&fixture("rbxm")).unwrap();
    let def = dom.root_refs()[0];
    (dom, def)
}

fn load_xml() -> (WeakDom, Ref) {
    let text = String::from_utf8(fixture("rbxmx")).unwrap();
    let dom = rbx_xml::deserialize(&text).unwrap();
    let def = dom.root_refs()[0];
    (dom, def)
}

fn entry(dom: &WeakDom, sm: Ref) -> Ref {
    let attributes =
        rbx_dom::attributes::decode(dom.get(sm).unwrap().properties().get("AttributesSerialize"));
    let Some(Variant::Ref(target)) = attributes.get("EntryState") else {
        panic!("no entry: {attributes:?}");
    };
    *target
}

fn machine(dom: &WeakDom, def: Ref) -> Ref {
    child(dom, def, "StateMachineNode")
}

fn child(dom: &WeakDom, of: Ref, name: &str) -> Ref {
    dom.get(of)
        .unwrap()
        .children()
        .iter()
        .copied()
        .find(|c| dom.get(*c).unwrap().name() == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

fn raw(dom: &WeakDom, of: Ref, property: &str) -> Vec<u8> {
    match dom.get(of).unwrap().properties().get(property) {
        Some(Variant::Unknown { raw, .. }) => raw.clone(),
        Some(Variant::String(s)) => s.as_bytes().to_vec(),
        other => panic!("{property}: {other:?}"),
    }
}

#[test]
fn studio_s_entry_state_reads_as_the_state_it_names() {
    let (dom, def) = load();
    let graph = read(&dom, def);
    let sm = graph
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::StateMachine)
        .unwrap();
    assert_eq!(
        sm.props["EntryState"],
        Prop::Value(Value::Text("State".into()))
    );
    assert_eq!(sm.inputs.len(), 3);
}

#[test]
fn the_writer_makes_studio_s_entry_state_and_pin_data_bytes() {
    let (studio, def) = load();
    let graph = read(&studio, def);

    let mut dom = WeakDom::new();
    let out = write(&mut dom, None, &graph);
    let (a, b) = (machine(&studio, def), machine(&dom, out));

    assert_eq!(
        raw(&dom, b, "InputPinData"),
        raw(&studio, a, "InputPinData")
    );
    let ours =
        rbx_dom::attributes::decode(dom.get(b).unwrap().properties().get("AttributesSerialize"));
    let state = child(&dom, b, "State");
    assert_eq!(ours["EntryState"], Variant::Ref(state));

    // In a binary file the attribute holds the state's file referent; saved
    // and reloaded it must still name the same ObjectValue.
    let saved = rbx_binary::serialize(&dom).unwrap();
    let back = rbx_binary::deserialize(&saved).unwrap();
    let def = back.root_refs()[0];
    let sm = machine(&back, def);
    let theirs = rbx_dom::attributes::decode(
        back.get(sm)
            .unwrap()
            .properties()
            .get("AttributesSerialize"),
    );
    let Variant::Ref(target) = theirs["EntryState"] else {
        panic!("no entry");
    };
    assert_eq!(back.get(target).unwrap().name(), "State");
    assert_eq!(
        raw(&back, sm, "InputPinData"),
        raw(&studio, a, "InputPinData")
    );
}

#[test]
fn studio_s_xml_entry_state_reads_as_the_binary_one_does() {
    let (xml, def) = load_xml();
    let sm = machine(&xml, def);
    assert_eq!(entry(&xml, sm), child(&xml, sm, "State"));
    let (binary, bdef) = load();
    assert_eq!(read(&xml, def), read(&binary, bdef));
}

#[test]
fn an_xml_save_writes_entry_state_as_studio_does() {
    let (xml, def) = load_xml();
    let text = rbx_xml::serialize(&xml).unwrap();
    let sm = machine(&xml, def);
    let state = child(&xml, sm, "State");
    // Studio's own lines for the node: an empty blob, then the Ref.
    let blob = r#"<BinaryString name="AttributesSerialize">AAAAAA==</BinaryString>"#;
    let reference = format!(
        r#"<Ref name="__attrRef_EntryState">RBX{}</Ref>"#,
        state.value()
    );
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| *l == reference).expect("no Ref");
    assert_eq!(lines[at - 1], blob);

    let back = rbx_xml::deserialize(&text).unwrap();
    let back_def = back.root_refs()[0];
    let sm = machine(&back, back_def);
    assert_eq!(back.get(entry(&back, sm)).unwrap().name(), "State");
    assert_eq!(read(&back, back_def), read(&xml, def));
}
