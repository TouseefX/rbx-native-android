//! The JSON text parser behind [`Json::parse`](super::Json::parse).

use super::Json;

pub(super) fn parse_text(text: &str) -> Result<Json, String> {
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
