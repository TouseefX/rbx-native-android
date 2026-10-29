//! The join script -> [`Config`] bridge: the reference's
//! `readJoinTarget`/`readSessionCrypto`, verbatim semantics. Everything
//! the transport needs comes out of the join-game reply's `joinScript`
//! object; the one number people get wrong is in here as a comment —
//! the RUPP TLV2 pair is `rcc_ip:NetStackPort`, *not* the RCC's own
//! port from `ServerConnections`.

use crate::client::Config;
use serde_json::Value;

/// Build the transport [`Config`] out of a join script.
///
/// - `NetStackPort` — a number in v1 joins, a string in legacy replies.
/// - `UdmuxEndpoints[0]` — the RakNet endpoint we dial.
/// - `ServerConnections[0].Address` — the RCC address for RUPP TLV2,
///   paired with `NetStackPort` (the field's own `Port` is a different
///   number and is never used).
/// - `NetStackTokenValue` — base64, 16 bytes, optional.
/// - `EphemeralEarlyPubKey` — the per-session server key, bare base64
///   or a keyring document.
/// - `ClientTicket` — parts 2 and 3 (split on `;`) are the handshake's
///   pre/auth blobs; shorter tickets leave the defaults.
pub fn config_from_join_script(js: &Value) -> Result<Config, String> {
    let mut cfg = Config::default();

    let netstack_port = port_of_field(js, "NetStackPort")
        .ok_or_else(|| "joinScript lacks NetStackPort".to_string())?;
    if netstack_port == 0 {
        return Err("joinScript has no usable NetStackPort".to_string());
    }

    let endpoints = array_field(js, "UdmuxEndpoints")
        .ok_or_else(|| "join script has no UdmuxEndpoints[]".to_string())?;
    let ep = &endpoints[0];
    let addr = address_of(ep);
    if addr.is_empty() {
        return Err("join script has no UdmuxEndpoints[0].Address".to_string());
    }
    cfg.peer_ip = ipv4_field(&addr, "UdmuxEndpoints[0].Address")?;
    // The endpoint's own Port when present, else the NetStackPort.
    cfg.peer_port = match ep.get("Port") {
        Some(p) => {
            let p = port_of(p);
            if p != 0 {
                p
            } else {
                netstack_port
            }
        }
        None => netstack_port,
    };

    let servers = array_field(js, "ServerConnections")
        .ok_or_else(|| "joinScript lacks ServerConnections[]".to_string())?;
    let rcc = address_of(&servers[0]);
    if rcc.is_empty() {
        return Err("joinScript lacks ServerConnections[0].Address".to_string());
    }
    cfg.rcc_ip = ipv4_field(&rcc, "ServerConnections[0].Address")?;
    // RUPP TLV2 carries rcc_ip:NetStackPort (native rupp_tx001). The RCC
    // port in ServerConnections is a different number and is NOT the
    // TLV value — sending it makes the edge drop the handshake.
    cfg.rcc_port = netstack_port;

    // The token is parsed but not required: a caller may supply one out
    // of band, and policy belongs to whoever needs it.
    if let Some(t) = js.get("NetStackTokenValue").and_then(Value::as_str) {
        let tok = b64_decode(t);
        if tok.len() == 16 {
            cfg.join_token = tok[..16].try_into().unwrap();
        }
    }

    cfg.peer_pub = key_from_field(js, "EphemeralEarlyPubKey")?;

    if let Some(t) = js.get("ClientTicket").and_then(Value::as_str) {
        let parts: Vec<&str> = t.split(';').collect();
        if parts.len() >= 4 {
            let pre = b64_decode(parts[2]);
            let auth = b64_decode(parts[3]);
            // The reference flags has_crypto at >=16/>=32 and gates the
            // handshake on it; an empty Vec is our absent.
            if pre.len() >= 16 && auth.len() >= 32 {
                cfg.ticket_pre = pre;
                cfg.ticket_auth = auth;
            }
        }
    }

    Ok(cfg)
}

/// An Address field that must parse as IPv4.
fn ipv4_field(addr: &str, what: &str) -> Result<[u8; 4], String> {
    match read_ipv4(addr) {
        Some(ip) => Ok(ip),
        None => Err(format!("{what} is not IPv4: {addr}")),
    }
}

/// `js[name]` as a non-empty array.
fn array_field<'a>(js: &'a Value, name: &str) -> Option<&'a Vec<Value>> {
    match js.get(name) {
        Some(Value::Array(a)) if !a.is_empty() => Some(a),
        _ => None,
    }
}

/// An endpoint's `Address`, if it is an object carrying one.
fn address_of(endpoint: &Value) -> String {
    match endpoint {
        Value::Object(o) => match o.get("Address") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// A port that arrives as a number (v1) or a string (legacy replies).
fn port_of(v: &Value) -> u16 {
    if let Some(n) = v.as_i64() {
        return n as u16;
    }
    if let Some(s) = v.as_str() {
        if let Ok(n) = s.trim().parse::<i32>() {
            return n as u16;
        }
    }
    0
}

fn port_of_field(js: &Value, name: &str) -> Option<u16> {
    js.get(name).map(port_of)
}

/// `"1.2.3.4"` -> `[1, 2, 3, 4]`, trailing junk tolerated (the
/// reference's `sscanf("%u.%u.%u.%u")` stops after the fourth).
pub fn read_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut parts = s.split('.');
    let mut out = [0u8; 4];
    for slot in out.iter_mut() {
        let part = parts.next()?.trim_start();
        // sscanf's %u: leading digits, ignore whatever follows them.
        let digits = part
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("");
        *slot = digits.parse::<u32>().ok()? as u8;
    }
    Some(out)
}

/// The reference's percent-decode: `%XX` -> byte, everything else
/// verbatim. Join-script fields arrive percent-encoded (`…%3D`).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The reference's `b64decode`: filter to the alphabet, decode
/// pad-free in quads, a leftover of 2 or 3 chars decodes its bytes, and
/// a leftover of 1 (which Python's b64decode rejects) falls back to the
/// raw string's bytes. No padding-bit validation.
pub fn b64_decode(input: &str) -> Vec<u8> {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut vals = [255u8; 256];
    for (i, &c) in ALPHA.iter().enumerate() {
        vals[usize::from(c)] = i as u8;
    }
    let s: Vec<u8> = input
        .as_bytes()
        .iter()
        .filter(|&&c| vals[usize::from(c)] != 255)
        .map(|&c| vals[usize::from(c)])
        .collect();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= s.len() {
        let quad = (u32::from(s[i]) << 18)
            | (u32::from(s[i + 1]) << 12)
            | (u32::from(s[i + 2]) << 6)
            | u32::from(s[i + 3]);
        out.push((quad >> 16) as u8);
        out.push(((quad >> 8) & 0xff) as u8);
        out.push((quad & 0xff) as u8);
        i += 4;
    }
    match s.len() - i {
        2 => {
            let quad = (u32::from(s[i]) << 18) | (u32::from(s[i + 1]) << 12);
            out.push((quad >> 16) as u8);
        }
        3 => {
            let quad =
                (u32::from(s[i]) << 18) | (u32::from(s[i + 1]) << 12) | (u32::from(s[i + 2]) << 6);
            out.push((quad >> 16) as u8);
            out.push(((quad >> 8) & 0xff) as u8);
        }
        1 => return input.as_bytes().to_vec(),
        _ => {}
    }
    out
}

/// The key fields come in two shapes: a bare base64 key, or a keyring
/// document *string* (`{"applications":{"<app>":{"versions":[{"id":n,
/// "value":"<b64>"}]}}}`), which is what `ClientPublicKeyData` always
/// is — take the first 32-byte value. (The reference's json iterates
/// applications alphabetically; serde_json here preserves document
/// order. Real keyrings carry one application, so the two agree.)
pub fn key_from_field(js: &Value, name: &str) -> Result<[u8; 32], String> {
    let Some(raw) = js.get(name).and_then(Value::as_str) else {
        return Err(format!("join script has no {name}"));
    };
    let s = percent_decode(raw);
    let mut cands: Vec<(String, String)> = Vec::new();
    if s.starts_with('{') {
        let Ok(doc) = serde_json::from_str::<Value>(&s) else {
            return Err(format!("{name} is not a readable keyring document"));
        };
        let Some(Value::Object(apps)) = doc.get("applications") else {
            return Err(format!("{name} keyring has no applications"));
        };
        for (app, body) in apps {
            if let Some(Value::Array(versions)) = body.get("versions") {
                for v in versions {
                    if let Some(Value::String(b64)) = v.get("value") {
                        cands.push((app.clone(), b64.clone()));
                    }
                }
            }
        }
    } else {
        cands.push(("raw".to_string(), s.clone()));
    }
    for (_, b64) in &cands {
        let raw = b64_decode(b64);
        if raw.len() == 32 {
            return Ok(raw[..32].try_into().unwrap());
        }
    }
    // The reference's failure diagnostic is the decoded string's head,
    // whichever shape it came in.
    let head: String = s.chars().take(120).collect();
    Err(format!("{name} has no 32-byte key: {head}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The standard padded encoder, to build test inputs.
    fn b64(data: &[u8]) -> String {
        const ALPHA: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(ALPHA[(n >> 18) as usize & 63] as char);
            out.push(ALPHA[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                ALPHA[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHA[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    #[test]
    fn a_full_join_script_builds_the_config() {
        let token = [7u8; 16];
        let key = [9u8; 32];
        let pre = [0xAAu8; 33];
        let auth = [0x55u8; 66];
        let js = json!({
            "NetStackPort": 51751,
            "UdmuxEndpoints": [{"Address": "10.190.0.5", "Port": 50105}],
            "ServerConnections": [{"Address": "10.17.9.213", "Port": 9999}],
            "NetStackTokenValue": b64(&token),
            "EphemeralEarlyPubKey": b64(&key),
            "ClientTicket": format!("a;b;{};{};e", b64(&pre), b64(&auth)),
        });
        let cfg = config_from_join_script(&js).unwrap();
        assert_eq!(cfg.peer_ip, [10, 190, 0, 5]);
        assert_eq!(cfg.peer_port, 50105);
        assert_eq!(cfg.rcc_ip, [10, 17, 9, 213]);
        // The RCC pair is the NetStackPort, never ServerConnections' own.
        assert_eq!(cfg.rcc_port, 51751);
        assert_eq!(cfg.join_token, token);
        assert_eq!(cfg.peer_pub, key);
        assert_eq!(cfg.ticket_pre, pre);
        assert_eq!(cfg.ticket_auth, auth);
    }

    #[test]
    fn legacy_string_ports_and_the_endpoint_fallback() {
        let js = json!({
            "NetStackPort": "51751",
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
            "EphemeralEarlyPubKey": b64(&[1u8; 32]),
        });
        let cfg = config_from_join_script(&js).unwrap();
        assert_eq!(cfg.peer_port, 51751); // no endpoint Port -> NetStackPort
        assert_eq!(cfg.rcc_port, 51751);
        // No NetStackTokenValue and no ClientTicket: defaults stay.
        assert_eq!(cfg.join_token, [0u8; 16]);
        assert!(cfg.ticket_pre.is_empty());
        assert!(cfg.ticket_auth.is_empty());
    }

    #[test]
    fn the_keyring_document_shape_takes_the_first_32_byte_value() {
        let key = [0x42u8; 32];
        let ring = json!({
            "applications": {"transport": {"versions": [
                {"id": 1, "value": b64(&[0u8; 16])},
                {"id": 2, "value": b64(&key)},
            ]}}
        });
        let js = json!({
            "NetStackPort": 1,
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
            "EphemeralEarlyPubKey": ring.to_string(),
        });
        let cfg = config_from_join_script(&js).unwrap();
        assert_eq!(cfg.peer_pub, key);
    }

    #[test]
    fn percent_encoding_on_the_key_field_decodes_before_use() {
        let key = [0x42u8; 32];
        // The same base64 with its padding spelled as %3D escapes.
        let b = b64(&key);
        let encoded = format!("{}%3D", b.trim_end_matches('='));
        let js = json!({
            "NetStackPort": 1,
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
            "EphemeralEarlyPubKey": encoded,
        });
        let cfg = config_from_join_script(&js).unwrap();
        assert_eq!(cfg.peer_pub, key);
    }

    #[test]
    fn missing_fields_report_what_they_are() {
        let e = config_from_join_script(&json!({})).unwrap_err();
        assert!(e.contains("NetStackPort"), "{e}");

        let js = json!({"NetStackPort": 1});
        let e = config_from_join_script(&js).unwrap_err();
        assert!(e.contains("UdmuxEndpoints"), "{e}");

        let js = json!({"NetStackPort": 1, "UdmuxEndpoints": [{"Port": 5}]});
        let e = config_from_join_script(&js).unwrap_err();
        assert!(e.contains("Address"), "{e}");

        let js = json!({"NetStackPort": 1, "UdmuxEndpoints": [{"Address": "1.2.3.4"}]});
        let e = config_from_join_script(&js).unwrap_err();
        assert!(e.contains("ServerConnections"), "{e}");

        let js = json!({
            "NetStackPort": 1,
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
        });
        let e = config_from_join_script(&js).unwrap_err();
        assert!(e.contains("EphemeralEarlyPubKey"), "{e}");

        let js = json!({
            "NetStackPort": 1,
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
            "EphemeralEarlyPubKey": b64(&[1u8; 16]),
        });
        let e = config_from_join_script(&js).unwrap_err();
        assert!(e.contains("no 32-byte key"), "{e}");
    }

    #[test]
    fn a_short_ticket_leaves_the_crypto_defaults() {
        let js = json!({
            "NetStackPort": 1,
            "UdmuxEndpoints": [{"Address": "1.2.3.4"}],
            "ServerConnections": [{"Address": "5.6.7.8"}],
            "EphemeralEarlyPubKey": b64(&[1u8; 32]),
            "ClientTicket": "a;b;c",
        });
        let cfg = config_from_join_script(&js).unwrap();
        assert!(cfg.ticket_pre.is_empty());
        assert!(cfg.ticket_auth.is_empty());
    }

    #[test]
    fn ipv4_parsing_matches_the_sscanf_shape() {
        assert_eq!(read_ipv4("1.2.3.4"), Some([1, 2, 3, 4]));
        assert_eq!(read_ipv4("255.255.255.255"), Some([255, 255, 255, 255]));
        // Trailing junk after the fourth number, like the reference.
        assert_eq!(read_ipv4("10.0.0.1junk"), Some([10, 0, 0, 1]));
        assert_eq!(read_ipv4("1.2.3"), None);
        assert_eq!(read_ipv4("host.example"), None);
    }

    #[test]
    fn percent_decode_handles_the_join_field_shapes() {
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("a%3Db"), "a=b");
        assert_eq!(percent_decode("a%2Bb"), "a+b");
        // Incomplete escapes pass through.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn b64_decode_matches_the_reference_tolerances() {
        assert_eq!(b64_decode("QQ=="), b"A".to_vec());
        assert_eq!(b64_decode("QUI="), b"AB".to_vec());
        assert_eq!(b64_decode("QUJD"), b"ABC".to_vec());
        // Padding-free groups decode.
        assert_eq!(b64_decode("QUI"), b"AB".to_vec());
        // Whitespace and strays are filtered.
        assert_eq!(b64_decode("QU JD\n"), b"ABC".to_vec());
        // A leftover of one char is the raw-string fallback.
        assert_eq!(b64_decode("A"), b"A".to_vec());
    }
}
