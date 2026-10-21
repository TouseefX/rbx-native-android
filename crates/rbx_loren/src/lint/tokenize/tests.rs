use super::*;

fn values(src: &str) -> Vec<String> {
    tokenize(src).into_iter().map(|t| t.value).collect()
}

#[test]
fn comments_are_skipped() {
    assert_eq!(
        values("a -- note\nb --[[ x\ny ]] c --[==[ ]] ]==] d"),
        ["a", "b", "c", "d"]
    );
    assert_eq!(tokenize("a\n--x\nb")[1].line, 3);
}

#[test]
fn quoted_and_long_strings() {
    let t = tokenize("x = \"a\\\"b\" [==[hi]]]==]");
    assert_eq!(t[2].text.as_deref(), Some("a\\\"b"));
    assert_eq!(t[3].value, "[==[hi]]]==]");
    assert_eq!(t[3].text.as_deref(), Some("hi]]"));
    let open = tokenize("'abc\nz");
    assert_eq!(open[0].text.as_deref(), Some("abc"));
    assert_eq!(open[1].line, 2);
}

#[test]
fn interpolated_strings() {
    let t = tokenize("`a {f(\"}\")} {{x}}` y");
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].kind, TokenKind::Str);
    assert_eq!(t[0].text, None);
    assert_eq!(t[1].value, "y");
}

#[test]
fn numbers_and_operators() {
    assert_eq!(
        values("1e+5 0x1e+5 a..b 1..2 x//=3 é"),
        ["1e+5", "0x1e", "+", "5", "a", "..", "b", "1", "..", "2", "x", "//=", "3", "é"]
    );
}

#[test]
fn if_expression_has_no_end() {
    let toks = tokenize("local x = if a then 1 else 2\nif b then f() end");
    let s = structure(&toks);
    let first_if = toks.iter().position(|t| t.value == "if").unwrap();
    assert_eq!(s.matching[first_if], -1);
    let second_if = toks.iter().rposition(|t| t.value == "if").unwrap();
    let end = toks.iter().rposition(|t| t.value == "end").unwrap();
    assert_eq!(s.matching[second_if], end as i32);
    assert_eq!(s.depth[end], 1);
}

#[test]
fn brackets_match() {
    let toks = tokenize("f({1})");
    let s = structure(&toks);
    assert_eq!(s.matching[1], 5);
    assert_eq!(s.matching[2], 4);
}
