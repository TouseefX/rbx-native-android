use rbx_play::protocol::{Command, CommandBody, DeviceReport, Located, Metrics, Side, Uplink};
use serde_json::json;

fn set_tags() -> CommandBody {
    CommandBody::SetTags {
        side: Side::Client,
        target: 7,
        tags: vec!["A".into()],
    }
}

#[test]
fn set_tags_body_serializes_to_wire_shape() {
    let value = serde_json::to_value(set_tags()).expect("serialize SetTags");
    assert_eq!(
        value,
        json!({
            "type": "set_tags",
            "side": "client",
            "target": 7,
            "tags": ["A"],
        })
    );
}

#[test]
fn stats_uplink_with_partial_metrics_fills_the_rest_with_none() {
    let uplink: Uplink = serde_json::from_str(
        r#"{"type":"stats","side":"server","metrics":{"fps":60,"players":1}}"#,
    )
    .expect("deserialize stats");

    let Uplink::Stats { side, metrics } = uplink else {
        panic!("expected Uplink::Stats");
    };
    assert_eq!(side, Side::Server);
    assert_eq!(metrics.fps, Some(60.0));
    assert_eq!(metrics.players, Some(1));
    assert_eq!(
        metrics,
        Metrics {
            fps: Some(60.0),
            players: Some(1),
            ..Metrics::default()
        }
    );
}

#[test]
fn stats_uplink_with_empty_metrics_is_all_none() {
    let uplink: Uplink = serde_json::from_str(r#"{"type":"stats","side":"client","metrics":{}}"#)
        .expect("deserialize stats");

    assert_eq!(
        uplink,
        Uplink::Stats {
            side: Side::Client,
            metrics: Metrics::default(),
        }
    );
}

#[test]
fn device_uplink_with_full_report_deserializes() {
    let uplink: Uplink = serde_json::from_str(
        r#"{"type":"device","viewport_width":852,"viewport_height":393,"touch_enabled":true,"keyboard_enabled":false,"gamepad_enabled":false,"ten_foot":false,"preferred_input":"Touch","on_screen_keyboard_visible":false,"text_focus":true,"last_input":"Touch"}"#,
    )
    .expect("deserialize device");

    assert_eq!(
        uplink,
        Uplink::Device(DeviceReport {
            viewport_width: 852.0,
            viewport_height: 393.0,
            touch_enabled: true,
            keyboard_enabled: false,
            gamepad_enabled: false,
            ten_foot: false,
            preferred_input: Some("Touch".into()),
            on_screen_keyboard_visible: false,
            text_focus: true,
            last_input: Some("Touch".into()),
        })
    );
}

#[test]
fn device_uplink_with_only_type_fills_report_with_defaults() {
    let uplink: Uplink =
        serde_json::from_str(r#"{"type":"device"}"#).expect("deserialize bare device");

    assert_eq!(uplink, Uplink::Device(DeviceReport::default()));
}

#[test]
fn device_report_round_trips_flat_with_type_tag() {
    let report = DeviceReport {
        viewport_width: 844.0,
        viewport_height: 390.0,
        touch_enabled: true,
        keyboard_enabled: true,
        gamepad_enabled: true,
        ten_foot: true,
        preferred_input: Some("Gamepad".into()),
        on_screen_keyboard_visible: true,
        text_focus: true,
        last_input: Some("Gamepad1".into()),
    };
    let value = serde_json::to_value(Uplink::Device(report)).expect("serialize device");
    assert_eq!(value["type"], "device");
    assert_eq!(value["viewport_width"], 844.0);
    assert!(value.get("device").is_none());

    let parsed: Uplink = serde_json::from_value(value).expect("deserialize device");
    assert_eq!(
        parsed,
        Uplink::Device(DeviceReport {
            viewport_width: 844.0,
            viewport_height: 390.0,
            touch_enabled: true,
            keyboard_enabled: true,
            gamepad_enabled: true,
            ten_foot: true,
            preferred_input: Some("Gamepad".into()),
            on_screen_keyboard_visible: true,
            text_focus: true,
            last_input: Some("Gamepad1".into()),
        })
    );
}

#[test]
fn command_with_set_tags_round_trips() {
    let original = Command {
        id: 3,
        body: set_tags(),
    };
    let text = serde_json::to_string(&original).expect("serialize Command");
    let parsed: Command = serde_json::from_str(&text).expect("deserialize Command");
    assert_eq!(parsed, original);
}

#[test]
fn pointer_body_serializes_to_wire_shape() {
    let value = serde_json::to_value(CommandBody::Pointer { inside: false }).expect("serialize");
    assert_eq!(value, json!({ "type": "pointer", "inside": false }));
}

#[test]
fn input_commands_serialize_to_wire_shape() {
    let navigate = CommandBody::Navigate {
        path: None,
        x: 1.0,
        y: 2.0,
        z: 3.0,
        speed: 0.5,
    };
    assert_eq!(
        serde_json::to_value(navigate).expect("serialize"),
        json!({ "type": "navigate", "x": 1.0, "y": 2.0, "z": 3.0, "speed": 0.5 })
    );
    let locate = CommandBody::Locate {
        path: Some("LocalPlayer.PlayerGui.Gui.Button".to_owned()),
    };
    assert_eq!(
        serde_json::to_value(locate).expect("serialize"),
        json!({ "type": "locate", "path": "LocalPlayer.PlayerGui.Gui.Button" })
    );
    assert_eq!(
        serde_json::to_value(CommandBody::TypeText {
            text: "hi".to_owned(),
            path: None
        })
        .expect("serialize"),
        json!({ "type": "type_text", "text": "hi" })
    );
}

#[test]
fn located_reads_the_probes_answer_with_or_without_a_point() {
    let screen: Located =
        serde_json::from_str(r#"{"inset_x":0,"inset_y":58,"width":1342,"height":794}"#)
            .expect("parse");
    assert_eq!((screen.x, screen.y, screen.inset_y), (None, None, 58.0));
    let point: Located = serde_json::from_str(
        r#"{"x":200,"y":130,"inset_x":0,"inset_y":58,"width":1342,"height":794}"#,
    )
    .expect("parse");
    assert_eq!((point.x, point.y), (Some(200.0), Some(130.0)));
}

#[test]
fn a_client_ack_with_its_side_still_reads_as_an_ack() {
    let ack: Uplink = serde_json::from_value(
        json!({ "type": "ack", "id": 4, "ok": true, "result": "Success", "side": "client" }),
    )
    .expect("parse");
    assert_eq!(
        ack,
        Uplink::Ack {
            id: 4,
            ok: true,
            result: "Success".to_owned()
        }
    );
}
