//! JSON with the semantics the CLI's files depend on: objects keep their key
//! order, and [`Json::stringify`] prints exactly what `JSON.stringify(value,
//! null, indent)` does, so rewritten project and settings files diff cleanly.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    /// Insertion-ordered; a duplicate key keeps its first position and the last value (as `JSON.parse`).
    Object(Vec<(String, Json)>),
}

impl Json {
    /// `JSON.parse`. A leading BOM is an error, as upstream (callers strip it).
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut p = Parser {
            s: text.as_bytes(),
            text,
            i: 0,
        };
        p.ws();
        let value = p.value()?;
        p.ws();
        if p.i != p.s.len() {
            return Err(p.fail("unexpected data after JSON value"));
        }
        Ok(value)
    }

    /// `JSON.stringify(self, null, indent)`; `indent` 0 is the compact form.
    /// JS caps the indent at 10 spaces.
    pub fn stringify(&self, indent: usize) -> String {
        let mut out = String::new();
        write_value(self, indent.min(10), 0, &mut out);
        out
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        match self {
            Json::Object(pairs) => pairs.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Replaces the value in place, or appends the key. No-op on a non-object.
    pub fn set(&mut self, key: &str, value: Json) {
        if let Json::Object(pairs) = self {
            match pairs.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = value,
                None => pairs.push((key.to_string(), value)),
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Json> {
        match self {
            Json::Object(pairs) => {
                let at = pairs.iter().position(|(k, _)| k == key)?;
                Some(pairs.remove(at).1)
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, Json)>> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Json::Object(_))
    }

    /// `Json::Object` from pairs.
    pub fn object(pairs: Vec<(&str, Json)>) -> Json {
        Json::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

fn write_value(v: &Json, indent: usize, depth: usize, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) => out.push_str(&number_to_string(*n)),
        Json::String(s) => write_string(s, out),
        Json::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(indent, depth + 1, out);
                write_value(item, indent, depth + 1, out);
            }
            newline(indent, depth, out);
            out.push(']');
        }
        Json::Object(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, item)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(indent, depth + 1, out);
                write_string(k, out);
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
                write_value(item, indent, depth + 1, out);
            }
            newline(indent, depth, out);
            out.push('}');
        }
    }
}

fn newline(indent: usize, depth: usize, out: &mut String) {
    if indent > 0 {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', indent * depth));
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `Number#toString` (non-finite values print as `null`, as `JSON.stringify` does).
fn number_to_string(n: f64) -> String {
    if !n.is_finite() {
        return "null".to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    let sci = format!("{:e}", n.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n10 = exp.parse::<i32>().unwrap_or(0) + 1;
    let mut s = String::new();
    if n < 0.0 {
        s.push('-');
    }
    if k <= n10 && n10 <= 21 {
        s.push_str(&digits);
        s.extend(std::iter::repeat_n('0', (n10 - k) as usize));
    } else if 0 < n10 && n10 <= 21 {
        s.push_str(&digits[..n10 as usize]);
        s.push('.');
        s.push_str(&digits[n10 as usize..]);
    } else if -6 < n10 && n10 <= 0 {
        s.push_str("0.");
        s.extend(std::iter::repeat_n('0', (-n10) as usize));
        s.push_str(&digits);
    } else {
        s.push_str(&digits[..1]);
        if k > 1 {
            s.push('.');
            s.push_str(&digits[1..]);
        }
        let e = n10 - 1;
        s.push('e');
        s.push(if e < 0 { '-' } else { '+' });
        s.push_str(&e.abs().to_string());
    }
    s
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn fail(&self, what: &str) -> String {
        format!("{what} at position {}", self.i)
    }

    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &str, value: Json) -> Result<Json, String> {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            Ok(value)
        } else {
            Err(self.fail("unexpected token"))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.s.get(self.i) {
            None => Err(self.fail("unexpected end of JSON input")),
            Some(b'n') => self.eat("null", Json::Null),
            Some(b't') => self.eat("true", Json::Bool(true)),
            Some(b'f') => self.eat("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(),
            Some(b'{') => self.object(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.fail("unexpected token")),
        }
    }

    fn digits(&mut self) -> Result<(), String> {
        let start = self.i;
        while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        if self.i == start {
            Err(self.fail("invalid number"))
        } else {
            Ok(())
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.s[self.i] == b'-' {
            self.i += 1;
        }
        if self.s.get(self.i) == Some(&b'0') {
            self.i += 1;
        } else {
            self.digits()?;
        }
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            self.digits()?;
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            self.digits()?;
        }
        self.text[start..self.i]
            .parse::<f64>()
            .map(Json::Number)
            .map_err(|e| e.to_string())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let chunk = self
            .s
            .get(self.i..self.i + 4)
            .ok_or_else(|| self.fail("bad unicode escape"))?;
        let text = std::str::from_utf8(chunk).map_err(|_| self.fail("bad unicode escape"))?;
        if !chunk.iter().all(u8::is_ascii_hexdigit) {
            return Err(self.fail("bad unicode escape"));
        }
        self.i += 4;
        u32::from_str_radix(text, 16).map_err(|e| e.to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.i..];
            let c = rest
                .chars()
                .next()
                .ok_or_else(|| self.fail("unterminated string"))?;
            self.i += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = *self
                        .s
                        .get(self.i)
                        .ok_or_else(|| self.fail("unterminated string"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&hi)
                                && self.s[self.i..].starts_with(b"\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                match self.hex4() {
                                    Ok(lo) if (0xdc00..0xe000).contains(&lo) => {
                                        0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                                    }
                                    _ => {
                                        self.i = save;
                                        hi
                                    }
                                }
                            } else {
                                hi
                            };
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.fail("bad escape")),
                    }
                }
                c if (c as u32) < 0x20 => return Err(self.fail("bad control character in string")),
                c => out.push(c),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.fail("expected ',' or ']'")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.i += 1;
        let mut obj = Json::Object(Vec::new());
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(obj);
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return Err(self.fail("expected property name"));
            }
            let key = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return Err(self.fail("expected ':'"));
            }
            self.i += 1;
            self.ws();
            let value = self.value()?;
            obj.set(&key, value);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(obj);
                }
                _ => return Err(self.fail("expected ',' or '}'")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
