use super::*;

#[test]
fn empty_containers() {
    assert_eq!(Json::parse("{}").unwrap().stringify(4), "{}");
    assert_eq!(Json::parse("[ ]").unwrap().stringify(4), "[]");
}

#[test]
fn nested_indent() {
    let v = Json::parse(r#"{"a":[1,{"b":null}],"c":"x"}"#).unwrap();
    assert_eq!(
        v.stringify(2),
        "{\n  \"a\": [\n    1,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": \"x\"\n}"
    );
    assert_eq!(v.stringify(0), r#"{"a":[1,{"b":null}],"c":"x"}"#);
}

#[test]
fn numbers_like_js() {
    let s = |n: f64| Json::Number(n).stringify(0);
    assert_eq!(s(1.0), "1");
    assert_eq!(s(-0.0), "0");
    assert_eq!(s(1.5), "1.5");
    assert_eq!(s(1e21), "1e+21");
    assert_eq!(s(1e20), "100000000000000000000");
    assert_eq!(s(1.5e-7), "1.5e-7");
    assert_eq!(s(0.000001), "0.000001");
    assert_eq!(s(123456.789), "123456.789");
    assert_eq!(s(f64::NAN), "null");
}

#[test]
fn string_escapes() {
    let v = Json::String("a\"\\\u{8}\u{c}\n\r\t\u{1}\u{1f}é/".to_string());
    assert_eq!(
        v.stringify(0),
        "\"a\\\"\\\\\\b\\f\\n\\r\\t\\u0001\\u001fé/\""
    );
}

#[test]
fn parse_escapes_and_pairs() {
    assert_eq!(
        Json::parse(r#""é😀\/""#).unwrap(),
        Json::String("é😀/".to_string())
    );
}

#[test]
fn duplicate_keys() {
    let v = Json::parse(r#"{"a":1,"b":2,"a":3}"#).unwrap();
    assert_eq!(v.stringify(0), r#"{"a":3,"b":2}"#);
}

#[test]
fn rejects_invalid() {
    for bad in [
        "",
        "{",
        "[1,]",
        "01",
        "1.",
        "'a'",
        "{\"a\":1,}",
        "\u{feff}1",
        "\"a\nb\"",
        "nul",
        "1 2",
    ] {
        assert!(Json::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn accessors() {
    let mut v = Json::object(vec![("a", Json::Bool(true))]);
    v.set("b", Json::Null);
    v.set("a", Json::Number(2.0));
    assert_eq!(v.stringify(0), r#"{"a":2,"b":null}"#);
    assert_eq!(v.remove("a"), Some(Json::Number(2.0)));
    assert!(v.get("a").is_none());
    assert!(v.get_mut("b").is_some());
    assert!(v.is_object());
}
