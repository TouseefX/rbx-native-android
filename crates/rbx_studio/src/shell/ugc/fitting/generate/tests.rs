use rbx_dom::{CFrameData, Content, Ref, Variant, Vector3Data, WeakDom};
use rbx_ugc::BodyScale;

use super::super::state::{AutoSkin, Wrap};
use super::*;

/// The bow Studio's tool was measured on: a textured `MeshPart`.
fn bow(dom: &mut WeakDom) -> (Ref, Ref) {
    let workspace = dom.new_instance("Workspace", "Workspace", None);
    let part = dom.new_instance("MeshPart", "Bow", Some(workspace));
    let props = [
        ("Anchored", Variant::Bool(true)),
        ("CanCollide", Variant::Bool(true)),
        (
            "MeshId",
            Variant::Content(Content::Uri("rbxassetid://6874004992".into())),
        ),
        (
            "size",
            Variant::Vector3(Vector3Data {
                x: 3.2833,
                y: 3.1139,
                z: 0.5717,
            }),
        ),
    ];
    for (k, v) in props {
        dom.set_property(part, k, v).unwrap();
    }
    dom.new_instance("SurfaceAppearance", "SurfaceAppearance", Some(part));
    (workspace, part)
}

fn prop<'a>(dom: &'a WeakDom, node: Ref, key: &str) -> &'a Variant {
    dom.get(node).unwrap().properties().get(key).unwrap()
}

fn cframe(dom: &WeakDom, node: Ref) -> CFrameData {
    match prop(dom, node, "CFrame") {
        Variant::CFrame(c) => *c,
        other => panic!("{other:?}"),
    }
}

fn children(dom: &WeakDom, node: Ref) -> Vec<(String, String)> {
    dom.get(node)
        .unwrap()
        .children()
        .iter()
        .map(|&c| {
            let i = dom.get(c).unwrap();
            (i.class().to_string(), i.name().to_string())
        })
        .collect()
}

fn child(dom: &WeakDom, node: Ref, name: &str) -> Ref {
    *dom.get(node)
        .unwrap()
        .children()
        .iter()
        .find(|&&c| dom.get(c).unwrap().name() == name)
        .unwrap()
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

fn pos(c: &CFrameData) -> [f32; 3] {
    [c.position.x, c.position.y, c.position.z]
}

const HAT: Cf = Cf::at([0.0, 6.2, 0.0]);

fn rigid(handle: Cf, scale: BodyScale) -> Output<'static> {
    Output {
        attachment: "HatAttachment",
        handle,
        body_attachment: HAT,
        scale: 1.0,
        kind: Kind::Rigid(scale),
    }
}

#[test]
fn an_untouched_rigid_fit_is_studios_accessory() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    let acc = generate(&mut dom, part, workspace, &rigid(HAT, BodyScale::Classic));

    let a = dom.get(acc).unwrap();
    assert_eq!((a.class(), a.name()), ("Accessory", "Bow"));
    assert_eq!(prop(&dom, acc, "AccessoryType"), &Variant::Enum(0));
    assert_eq!(
        prop(&dom, acc, "AttachmentPoint"),
        &Variant::CFrame(Cf::at([0.0; 3]).data())
    );
    // The source part itself is the handle.
    assert_eq!(dom.get(part).unwrap().name(), "Handle");
    assert_eq!(dom.parent(part), Some(acc));
    for (key, want) in [
        ("Anchored", false),
        ("Massless", false),
        ("CanQuery", true),
        ("CanTouch", true),
        ("CanCollide", true),
    ] {
        assert_eq!(prop(&dom, part, key), &Variant::Bool(want), "{key}");
    }
    assert_eq!(
        prop(&dom, part, "MeshId"),
        &Variant::Content(Content::Uri("rbxassetid://6874004992".into()))
    );
    assert!(close(pos(&cframe(&dom, part)), [0.0, 6.2, 0.0]));
    assert_eq!(
        children(&dom, part),
        [
            ("SurfaceAppearance".into(), "SurfaceAppearance".into()),
            ("StringValue".into(), "AvatarPartScaleType".into()),
            ("Attachment".into(), "HatAttachment".into()),
        ]
    );
    let value = child(&dom, part, "AvatarPartScaleType");
    assert_eq!(
        prop(&dom, value, "Value"),
        &Variant::String("Classic".into())
    );
    let att = child(&dom, part, "HatAttachment");
    assert_eq!(cframe(&dom, att), Cf::at([0.0; 3]).data());
}

#[test]
fn a_moved_fit_lives_in_the_attachment() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    // Moved 0.783 studs along -X with Studio's Move tool: the attachment
    // reads (0.783, 0, 0).
    let handle = HAT.mul(&Cf::at([-0.783, 0.0, 0.0]));
    generate(&mut dom, part, workspace, &rigid(handle, BodyScale::Normal));
    let att = child(&dom, part, "HatAttachment");
    assert!(close(pos(&cframe(&dom, att)), [0.783, 0.0, 0.0]));
    let value = child(&dom, part, "AvatarPartScaleType");
    assert_eq!(
        prop(&dom, value, "Value"),
        &Variant::String("ProportionsNormal".into())
    );
}

#[test]
fn a_turned_fit_turns_the_attachment_back() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    let quarter = Cf::with([0.0; 3], [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0]);
    generate(
        &mut dom,
        part,
        workspace,
        &rigid(HAT.mul(&quarter), BodyScale::Slender),
    );
    let att = cframe(&dom, child(&dom, part, "HatAttachment"));
    assert_eq!(att.rotation, quarter.inverse().r);
}

#[test]
fn scale_multiplies_the_handle_size_under_its_own_key() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    let out = Output {
        scale: 2.0,
        ..rigid(HAT, BodyScale::Classic)
    };
    generate(&mut dom, part, workspace, &out);
    assert_eq!(
        prop(&dom, part, "size"),
        &Variant::Vector3(Vector3Data {
            x: 6.5666,
            y: 6.2278,
            z: 1.1434
        })
    );
    assert!(dom.get(part).unwrap().properties().get("Size").is_none());
}

#[test]
fn a_layered_fit_gets_a_wrap_layer_and_no_scale_value() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    let torso = Cf::at([0.0, 4.0, 0.0]);
    let front = torso.mul(&Cf::at([0.0, 0.9828, -0.5410]));
    let out = Output {
        attachment: "BodyFrontAttachment",
        handle: torso,
        body_attachment: front,
        scale: 1.0,
        kind: Kind::Layered {
            wrap: Wrap {
                auto_skin: AutoSkin::EnabledPreserve,
                ..Wrap::default()
            },
            cages: Some(CageUris {
                inner: "rbxasset://rbx-native-local/fitting/1/Bow_InnerCage.mesh".into(),
                outer: "rbxasset://rbx-native-local/fitting/1/Bow_OuterCage.mesh".into(),
            }),
        },
    };
    generate(&mut dom, part, workspace, &out);
    assert_eq!(prop(&dom, part, "CanCollide"), &Variant::Bool(false));
    assert_eq!(
        children(&dom, part),
        [
            ("SurfaceAppearance".into(), "SurfaceAppearance".into()),
            ("WrapLayer".into(), "HandleWrapLayer".into()),
            ("Attachment".into(), "BodyFrontAttachment".into()),
        ]
    );
    let wrap = child(&dom, part, "HandleWrapLayer");
    assert_eq!(prop(&dom, wrap, "Order"), &Variant::Int32(1));
    assert_eq!(prop(&dom, wrap, "Puffiness"), &Variant::Float32(1.0));
    assert_eq!(prop(&dom, wrap, "ShrinkFactor"), &Variant::Float32(0.0));
    assert_eq!(prop(&dom, wrap, "AutoSkin"), &Variant::Enum(1));
    assert_eq!(prop(&dom, wrap, "Enabled"), &Variant::Bool(true));
    assert_eq!(
        prop(&dom, wrap, "BindOffset"),
        &Variant::CFrame(Cf::at([0.0; 3]).data())
    );
    assert_eq!(
        prop(&dom, wrap, "ReferenceMeshId"),
        &Variant::Content(Content::Uri(
            "rbxasset://rbx-native-local/fitting/1/Bow_InnerCage.mesh".into()
        ))
    );
    assert_eq!(
        prop(&dom, wrap, "CageMeshId"),
        &Variant::Content(Content::Uri(
            "rbxasset://rbx-native-local/fitting/1/Bow_OuterCage.mesh".into()
        ))
    );
    let att = cframe(&dom, child(&dom, part, "BodyFrontAttachment"));
    assert!(close(pos(&att), [0.0, 0.9828, -0.5410]));
}

#[test]
fn fitting_twice_leaves_one_of_each() {
    let mut dom = WeakDom::new();
    let (workspace, part) = bow(&mut dom);
    let acc = generate(&mut dom, part, workspace, &rigid(HAT, BodyScale::Classic));
    let again = generate(&mut dom, part, workspace, &rigid(HAT, BodyScale::Normal));
    dom.remove(acc);
    assert_eq!(dom.parent(part), Some(again));
    assert_eq!(children(&dom, part).len(), 3);
}
