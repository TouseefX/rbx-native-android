//! JSON with the semantics the CLI's files depend on: objects keep their key
//! order, and [`Json::stringify`] prints exactly what `JSON.stringify(value,
//! null, indent)` does, so rewritten project and settings files diff cleanly.

mod parse;
#[cfg(test)]
mod tests;

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
        parse::parse_text(text)
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
