use serde_json::{json, Value};

use super::{call_with, fetch, html, search, Fetched};

fn page(body: &str, content_type: &str) -> Fetched {
    Fetched {
        body: body.to_owned(),
        content_type: content_type.to_owned(),
    }
}

/// `http_get` over a fake fetch that serves `body`, and the text it answered.
fn get(arguments: Value, body: &str, content_type: &str) -> Result<String, String> {
    call_with(&arguments, |_| Ok(page(body, content_type))).map(|value| match value {
        Value::String(text) => text,
        other => panic!("http_get answers text, got {other}"),
    })
}

const CLASS_PAGE: &str = "# Class: TweenService\n\n## Methods\n\n### Create\n\nCreates a Tween.\n\
Parameters: instance, tweenInfo, propertyTable\n\n### GetValue\n\nReturns an eased alpha.\n\
Uses EasingStyle and EasingDirection.\n";

#[test]
fn the_allowlist_takes_documentation_and_refuses_everything_else() {
    for url in [
        "https://create.roblox.com/docs/reference/engine/classes/TweenService.md",
        "https://create.roblox.com/docs/llms.txt",
        "https://create.roblox.com/docs/cloud/llms.txt",
        "https://create.roblox.com/docs/performance-optimization/improve.md",
        "https://raw.githubusercontent.com/Roblox/creator-docs/main/content/en-us/ui/index.md",
        "https://github.com/Roblox/libmp/releases/download/latest/SKILL.md",
    ] {
        assert!(fetch::allowed(url).is_ok(), "{url} should be allowed");
    }
    for url in [
        "http://create.roblox.com/docs/llms.txt",
        "https://create.roblox.com/dashboard/creations",
        "https://create.roblox.com:8443/docs/llms.txt",
        "https://user:pw@create.roblox.com/docs/llms.txt",
        "https://create.roblox.com.evil.example/docs/llms.txt",
        "https://example.com/docs/index.md",
        "https://raw.githubusercontent.com/someone/else/main/README.md",
        "https://objects.githubusercontent.com/anything",
        "file:///etc/passwd",
        "not a url",
    ] {
        assert!(fetch::allowed(url).is_err(), "{url} should be refused");
    }
}

#[test]
fn a_refused_url_is_never_fetched() {
    let answer = call_with(&json!({ "url": "https://example.com/x.md" }), |_| {
        panic!("a refused URL must not be fetched")
    });
    assert!(answer.unwrap_err().starts_with("URL not allowed"));
    assert_eq!(
        call_with(&json!({}), |_| unreachable!()).unwrap_err(),
        "missing the `url` argument"
    );
}

#[test]
fn without_a_query_the_page_comes_back_whole() {
    let url = "https://create.roblox.com/docs/reference/engine/classes/TweenService.md";
    let text = get(json!({ "url": url }), CLASS_PAGE, "text/markdown").unwrap();
    assert_eq!(text, CLASS_PAGE.trim());
}

#[test]
fn a_query_returns_numbered_excerpts_around_each_match() {
    let url = "https://create.roblox.com/docs/reference/engine/classes/TweenService.md";
    let text = get(
        json!({ "url": url, "query": "easingstyle", "context_lines": 1 }),
        CLASS_PAGE,
        "text/markdown",
    )
    .unwrap();
    assert_eq!(
        text,
        format!("{url}\n1 match for \"easingstyle\":\n\n12: Returns an eased alpha.\n13: Uses EasingStyle and EasingDirection.")
    );
}

#[test]
fn a_miss_is_one_line_and_return_full_gives_the_page_on_a_hit() {
    let url = "https://create.roblox.com/docs/reference/engine/classes/TweenService.md";
    let miss = get(
        json!({ "url": url, "query": "zzqq" }),
        CLASS_PAGE,
        "text/markdown",
    );
    assert_eq!(miss.unwrap(), format!("No match for \"zzqq\" in {url}"));
    let full = get(
        json!({ "url": url, "query": "GetValue", "return_full": true }),
        CLASS_PAGE,
        "text/markdown",
    );
    assert_eq!(full.unwrap(), CLASS_PAGE.trim());
}

#[test]
fn excerpts_that_touch_are_joined_and_far_ones_are_split() {
    let text: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    let joined = search::excerpts(&text, "line 3", 1).unwrap();
    assert!(joined.contains("\n2: line 2\n3: line 3\n4: line 4"));
    let both = search::excerpts(&text, "line 1", 0).unwrap();
    // "line 1" is in lines 1 and 10 to 19: eleven matches, two runs.
    assert!(both.starts_with("11 matches"));
    assert_eq!(both.matches("\n...\n").count(), 1);
}

#[test]
fn a_fetch_failure_is_the_tools_own_error() {
    let url = "https://create.roblox.com/docs/reference/engine/classes/Nope.md";
    let answer = call_with(&json!({ "url": url }), |_| {
        Err(format!("HTTP 404 fetching {url}"))
    });
    assert_eq!(answer.unwrap_err(), format!("HTTP 404 fetching {url}"));
}

#[test]
fn html_is_stripped_to_readable_text() {
    let page_html = r#"<!DOCTYPE html><html><head><title>T</title><style>p{}</style></head>
<body><nav><a href="/">Home</a> Menu</nav><main><h1>Tween&nbsp;Service</h1>
<p>Creates <a href="/x">tweens</a> &amp; plays them.<script>alert(1)</script></p>
<ul><li>One</li><li>Two &lt;3</li></ul><pre>local t = 1
print(t)</pre><!-- hidden --></main><footer>Footer</footer></body></html>"#;
    assert_eq!(
        html::to_text(page_html),
        "# Tween Service\nCreates tweens & plays them.\n- One\n- Two <3\nlocal t = 1\nprint(t)"
    );
    let text = get(
        json!({ "url": "https://create.roblox.com/docs/ui" }),
        page_html,
        "text/html; charset=utf-8",
    )
    .unwrap();
    assert!(!text.contains("Menu") && !text.contains("alert") && !text.contains("Footer"));
}

#[test]
fn entities_decode_and_stray_ampersands_survive() {
    assert_eq!(
        html::decode_entities("a &amp; b &#60; &#x3E; & c &bogus;"),
        "a & b < > & c &bogus;"
    );
}
