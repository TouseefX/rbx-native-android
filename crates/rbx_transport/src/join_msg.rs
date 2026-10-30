//! The RakNet-era join messages and the native message envelope ("seal")
//! they ride in, ported from Boblox's `raknet/join_msg.cpp` **[GAME]** and
//! reverse-engineered from the 2026-10-27 `libroblox.so`:
//!
//! - the envelope is AES-128 over a header the native's own PRNG pads: a
//!   checksum word, one random byte, a pad-length byte, PCG filler, then the
//!   plaintext, encrypted as a backwards-chained CBC (block 0 first, then
//!   from the last block down, each chained with the ciphertext before it
//!   in the loop). Deterministic for `(key, state, len)`, always a whole
//!   number of 16-byte blocks.
//! - `0x90` is the join body: a captured template whose JSON is re-sealed
//!   with this session's values (template key order preserved — the JSON
//!   has to match the native's formatting, which is why `serde_json` comes
//!   with `preserve_order` here; the workspace already builds it that way
//!   through the vendored gpui).
//! - `0x8a` submits the ClientTicket with a hash chain over xxh32.
//! - `0x92` and `0x8f` are one-varint and one-string messages.
//!
//! The account-specific capture vectors the C++ suite pins are deliberately
//! not checked in upstream, so they are not embedded here either; the
//! account-independent constants (the `0x8a` key shape, the `0x92` rand
//! round-trip, the `0x9b` step form) are.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use serde_json::{Map, Value};

/// The 0x90 envelope key (native `xmmword_77050B0`).
pub const KEY_90: [u8; 16] = [
    0xfe, 0xf9, 0xf0, 0xeb, 0xe2, 0xdd, 0xd4, 0xcf, 0xc6, 0xc1, 0xb8, 0xb3, 0xaa, 0xa5, 0x9c, 0x97,
];
/// The captured 0x90 envelope PRNG state.
pub const STATE_90: u64 = 0xE7DD_5C9C_5148_F144;
/// The captured 0x8a envelope PRNG state (feeds only the padding bytes).
pub const STATE_8A: u64 = 0xC9AA_A1F2_9BCF_8572;

// The native cipher's PCG update and checksum constants (sub_664D9B0).
const K_MUL: u64 = 0x685F_98A2_018F_ADE9;
const K_ADD: u64 = 0x399D_2694_6951_29DE;
const K_MIX1: u32 = 0xFFFF_CE6D;
const K_CHECKSUM_SEED: u32 = 0xD971;

// The ticket hash's build-wide constants.
const HASH_CONST: u32 = 0x7137_5635;
const TICKET_CONST: u32 = 17498;
const ITER_CONST: u32 = 0x5578_B7D7;
const BAD_F00D: i64 = 0x0BAD_F00D;

/// The 0x8a build constant: 40 hex chars, '!', 32 hex, '.'.
const CONST_74: &str = "2e427f51c4dab762fe9e3471c6cfa1650841723b!64d6a124222d374da86ce360982320f0.";

// --- envelope ---------------------------------------------------------------

fn rand_byte(s: u64) -> u8 {
    let v = ((s >> 27) ^ (s >> 45)) as u32;
    let rot = ((s >> 59) & 31) as u32;
    (v.rotate_right(rot) & 0xFF) as u8
}

fn encrypt_block(cipher: &Aes128, bytes: &mut [u8]) {
    cipher.encrypt_block(GenericArray::from_mut_slice(bytes));
}

fn decrypt_block(cipher: &Aes128, bytes: &mut [u8]) {
    cipher.decrypt_block(GenericArray::from_mut_slice(bytes));
}

/// Seal `plain` with the native envelope: `[checksum u32 LE][rand][pad
/// byte][PCG pad][plain]` as backwards-chained AES-128-CBC, `plain.len() +
/// pad + 6` bytes long (always a multiple of 16).
pub fn seal(plain: &[u8], key: &[u8; 16], state0: u64) -> Vec<u8> {
    let pad = !(plain.len() + 5) & 0x0F;
    let outlen = plain.len() + pad + 6;
    let mut out = vec![0u8; outlen];
    let r0 = rand_byte(state0);
    let s1 = K_MUL.wrapping_mul(state0).wrapping_add(K_ADD);
    let mut state = K_MUL.wrapping_mul(s1).wrapping_add(K_ADD);
    out[4] = r0;
    out[5] = (pad + 16 * usize::from(rand_byte(s1) & 0x0F)) as u8;
    out[pad + 6..].copy_from_slice(plain);
    for b in &mut out[6..6 + pad] {
        *b = rand_byte(state);
        state = K_MUL.wrapping_mul(state).wrapping_add(K_ADD);
    }
    let mut h = K_CHECKSUM_SEED;
    let mut acc = 0u32;
    for b in &out[4..] {
        let v = *b ^ (h >> 8) as u8;
        h = K_MIX1
            .wrapping_mul(h.wrapping_add(u32::from(v)))
            .wrapping_add(22719);
        acc = acc.wrapping_add(u32::from(v));
    }
    out[0..4].copy_from_slice(&acc.to_le_bytes());

    let cipher = Aes128::new(GenericArray::from_slice(key));
    let nblk = outlen / 16;
    encrypt_block(&cipher, &mut out[0..16]);
    let mut chain = [0u8; 16];
    chain.copy_from_slice(&out[0..16]);
    for i in (1..nblk).rev() {
        let lo = 16 * i;
        for (j, b) in out[lo..lo + 16].iter_mut().enumerate() {
            *b ^= chain[j];
        }
        encrypt_block(&cipher, &mut out[lo..lo + 16]);
        chain.copy_from_slice(&out[lo..lo + 16]);
    }
    out
}

/// Inverse of [`seal`]: `(plaintext, pad)`, or `None` when the body is not
/// a whole number of blocks or the pad runs past the end.
pub fn unseal(body: &[u8], key: &[u8; 16]) -> Option<(Vec<u8>, usize)> {
    if body.is_empty() || !body.len().is_multiple_of(16) {
        return None;
    }
    let nblk = body.len() / 16;
    let mut out = body.to_vec();
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut chain = [0u8; 16];
    chain.copy_from_slice(&body[0..16]);
    for i in (1..nblk).rev() {
        let lo = 16 * i;
        decrypt_block(&cipher, &mut out[lo..lo + 16]);
        for (j, b) in out[lo..lo + 16].iter_mut().enumerate() {
            *b ^= chain[j];
        }
        chain.copy_from_slice(&body[lo..lo + 16]);
    }
    decrypt_block(&cipher, &mut out[0..16]);
    let pad = usize::from(out[5] & 0x0F);
    if pad + 6 > out.len() {
        return None;
    }
    Some((out[pad + 6..].to_vec(), pad))
}

/// One sealed application message: `[id][seal(body)]`.
pub fn sealed_message(id: u8, body: &[u8], key: &[u8; 16], state0: u64) -> Vec<u8> {
    let mut msg = Vec::with_capacity(body.len() + 22);
    msg.push(id);
    msg.extend_from_slice(&seal(body, key, state0));
    msg
}

// --- little writers ----------------------------------------------------------

fn leb128(v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut v = v;
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out
}

fn put_leb(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&leb128(v));
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    put_leb(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

// --- xxh32 and the ticket hash ----------------------------------------------

/// XXHash32, one-shot with a seed — the same streaming core as Boblox's
/// `common/xxh32.h` (dependency-free there, and here).
pub fn xxh32(data: &[u8], seed: u32) -> u32 {
    const P1: u32 = 0x9E37_79B1;
    const P2: u32 = 0x85EB_CA77;
    const P3: u32 = 0xC2B2_AE3D;
    const P4: u32 = 0x27D4_EB2F;
    const P5: u32 = 0x1656_67B1;

    let mut i = 0;
    let mut h;
    if data.len() >= 16 {
        let mut v = [
            seed.wrapping_add(P1).wrapping_add(P2),
            seed.wrapping_add(P2),
            seed,
            seed.wrapping_sub(P1),
        ];
        while i + 16 <= data.len() {
            for lane in &mut v {
                let k = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
                *lane = lane
                    .wrapping_add(k.wrapping_mul(P2))
                    .rotate_left(13)
                    .wrapping_mul(P1);
                i += 4;
            }
        }
        h = v[0]
            .rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18));
    } else {
        h = seed.wrapping_add(P5);
    }
    h = h.wrapping_add(data.len() as u32);
    while i + 4 <= data.len() {
        let k = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        h = h
            .wrapping_add(k.wrapping_mul(P3))
            .rotate_left(17)
            .wrapping_mul(P4);
        i += 4;
    }
    while i < data.len() {
        h = h
            .wrapping_add(u32::from(data[i]).wrapping_mul(P5))
            .rotate_left(11)
            .wrapping_mul(P1);
        i += 1;
    }
    h ^= h >> 15;
    h = h.wrapping_mul(P2);
    h ^= h >> 13;
    h = h.wrapping_mul(P3);
    h ^= h >> 16;
    h
}

/// The Unreal-style check hash the 0x8a ticket field carries (sub_301CBE2's
/// chain, constants pinned against the live capture).
pub fn ticket_hash(ticket: &str) -> u32 {
    let s = HASH_CONST;
    let t = xxh32(ticket.as_bytes(), 1);
    // The native computes -17498 * seed; multiplication is linear mod 2^32,
    // so the negation can happen first and stay unsigned-clean.
    let v8 = 0u32.wrapping_sub(TICKET_CONST.wrapping_mul(s)) as u16;
    let v9 = if v8 & 2 != 0 { 7 } else { 25 };
    let v12 = if v8 & 4 != 0 {
        0u32.wrapping_sub(s)
    } else {
        0u32.wrapping_sub(ITER_CONST)
    };
    let v13 = t.wrapping_add(ITER_CONST).rotate_left(v9).wrapping_add(v12);
    let v14 = if v8 & 8 != 0 { s } else { ITER_CONST };
    let v15 = if v8 & 0x20 != 0 { s } else { ITER_CONST };
    let v16 = if v8 & 0x40 != 0 { s } else { ITER_CONST };
    let v17 = if v8 & 0x4000 != 0 { s } else { ITER_CONST };
    let v18 = v13.wrapping_mul(v14);
    let v19 = if v8 & 0x10 != 0 { 13 } else { 19 };
    let v20 = v16 ^ v15.wrapping_sub(v18.rotate_left(v19));
    let v20 = v20.rotate_left(2 * (u32::from(v8 & 0xFF) >> 7) + 15);
    let v11 = if v8 & 0x100 != 0 {
        s
    } else {
        0u32.wrapping_sub(s)
    };
    let v21 = v11.wrapping_add(ITER_CONST);
    let v22 = if v8 & 0x200 != 0 {
        v21
    } else {
        v11.wrapping_sub(ITER_CONST)
    };
    let v23 = v20.wrapping_add(v22);
    let v25 = v23.rotate_left(if v8 & 0x400 != 0 { 23 } else { 9 });
    let v26 = if v8 & 0x800 != 0 {
        0u32.wrapping_sub(v25)
    } else {
        v25
    };
    let v27 = s.wrapping_add(v26);
    let v28 = if v8 & 0x1000 != 0 {
        0u32.wrapping_sub(v27)
    } else {
        v27
    };
    let v29 = v28.wrapping_add(ITER_CONST);
    v17 ^ v29.rotate_left(if v8 & 0x2000 != 0 { 29 } else { 3 })
}

// --- the 0x92 / 0x8f / 0x9b builders -----------------------------------------

/// `tail[2]` of the 0x90 template — the build constant the 0x8a key is
/// derived from.
pub fn tail2_from_90_template(tmpl: &[u8]) -> Option<u32> {
    if tmpl.len() < 24 {
        return None;
    }
    let n = tmpl.len();
    Some(u32::from_le_bytes(tmpl[n - 16..n - 12].try_into().unwrap()))
}

/// `v9`, the 0x92's other input: `tail[3] ^ (u16(tail[2]) | 0x22640000)`,
/// read out of the 0x90 template's build-constant tail (sub_664CCD4).
pub fn v9_from_90_template(tmpl: &[u8]) -> Option<u32> {
    if tmpl.len() < 24 {
        return None;
    }
    let n = tmpl.len();
    let t2 = u32::from_le_bytes(tmpl[n - 16..n - 12].try_into().unwrap());
    let t3 = u32::from_le_bytes(tmpl[n - 12..n - 8].try_into().unwrap());
    Some(t3 ^ ((t2 & 0xFFFF) | 0x2264_0000))
}

/// The 0x8a envelope key: `"<place>-1733811612<tail2>594292182"` truncated
/// to 16 bytes. `place` is 64-bit — live place ids sit well past `u32`,
/// and the native's decimal formatting takes them whole.
pub fn key_8a(place_id: u64, tail2: u32) -> String {
    let mut s = format!("{place_id}-1733811612{tail2}594292182");
    s.truncate(s.len().min(16));
    s
}

/// `[0x92] + LEB(zigzag(rand32 << 32 | rand32 ^ v9))` — sub_63D8F36.
pub fn build_92(rand32: u32, v9: u32) -> Vec<u8> {
    let x = (u64::from(rand32) << 32) | u64::from(rand32 ^ v9);
    let zz = (x << 1) ^ if rand32 & 0x8000_0000 != 0 { !0u64 } else { 0 };
    let mut out = Vec::with_capacity(11);
    out.push(0x92);
    put_leb(&mut out, zz);
    out
}

/// `[0x8f] + string(server id)` — sub_63D9E42.
pub fn build_8f(server_id: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + server_id.len());
    out.push(0x8F);
    put_string(&mut out, server_id);
    out
}

/// The client's 9-byte 0x9B join step: `[9b][a u32 LE][b u32 LE]`.
pub fn build_9b_step(a: u32, b: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.push(0x9B);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out
}

/// The 9-byte 0x9B challenge answer: `[9b][u2 u32 LE][answer u32 LE]`.
pub fn build_9b_response(u2: u32, answer: u32) -> Vec<u8> {
    build_9b_step(u2, answer)
}

/// True when `p` is the complete server form of the challenge: a 13-byte
/// header plus a blob that fits.
pub fn is_challenge(p: &[u8]) -> bool {
    if p.len() < 13 || p[0] != 0x9B {
        return false;
    }
    let blob = u32::from_le_bytes(p[9..13].try_into().unwrap());
    blob as usize <= p.len() - 13
}

// --- the 0x90 and 0x8a bodies ------------------------------------------------

/// A parsed 0x8a payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed8a {
    pub user_id: u32,
    pub ticket: String,
    pub session_json: String,
}

/// Read a captured 0x8a payload back into its fields, in native field
/// order (sub_63D7EB2).
pub fn parse_8a_payload(p: &[u8]) -> Result<Parsed8a, String> {
    let mut r = Reader { p, i: 0 };
    let zz = r.leb().ok_or("0x8a template payload does not parse")?;
    let ticket = r.str().ok_or("0x8a template payload does not parse")?;
    r.u32le().ok_or("0x8a template payload does not parse")?; // application id
    r.str().ok_or("0x8a template payload does not parse")?; // the 74B constant
    r.str().ok_or("0x8a template payload does not parse")?; // "Android"
    r.str().ok_or("0x8a template payload does not parse")?; // "?"
    r.leb().ok_or("0x8a template payload does not parse")?; // the two ticket
    r.leb().ok_or("0x8a template payload does not parse")?; // hashes
    r.str().ok_or("0x8a template payload does not parse")?; // ""
    let session_json = r.str().ok_or("0x8a template payload does not parse")?;
    let user_id = ((zz >> 1) ^ 0u64.wrapping_sub(zz & 1)) as u32;
    Ok(Parsed8a {
        user_id,
        ticket,
        session_json,
    })
}

struct Reader<'a> {
    p: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn leb(&mut self) -> Option<u64> {
        let mut v = 0u64;
        let mut shift = 0u32;
        loop {
            let b = *self.p.get(self.i)?;
            self.i += 1;
            v |= u64::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
    }

    fn str(&mut self) -> Option<String> {
        let len = self.leb()? as usize;
        let s = self.p.get(self.i..)?.get(..len)?;
        self.i += len;
        Some(String::from_utf8_lossy(s).into_owned())
    }

    fn u32le(&mut self) -> Option<u32> {
        let b = self.p.get(self.i..self.i + 4)?;
        self.i += 4;
        Some(u32::from_le_bytes(b.try_into().unwrap()))
    }
}

/// A join script field as a string: the value itself when it is one,
/// stringified otherwise, empty when absent.
fn json_string(js: &Value, key: &str) -> String {
    match js.get(key) {
        None => String::new(),
        Some(v) => value_to_string(v),
    }
}

/// The C++ `hexToStr`: any JSON value as the string the native puts in a
/// string field. (Floats format shortest-round-trip here; the join script
/// carries none.)
fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

/// Replace every template field the join script also carries, recursively,
/// preserving the template's key order and value kinds.
fn merge_into(dst: &mut Map<String, Value>, src: &Value) {
    for (k, dv) in dst.iter_mut() {
        let Some(sv) = src.get(k.as_str()) else {
            continue;
        };
        match dv {
            Value::Object(inner) => {
                if sv.is_object() {
                    merge_into(inner, sv);
                }
            }
            Value::String(_) => *dv = Value::String(value_to_string(sv)),
            Value::Number(_) if sv.is_number() => *dv = sv.clone(),
            Value::Bool(_) if sv.is_boolean() => *dv = sv.clone(),
            Value::Null => *dv = sv.clone(),
            _ => {}
        }
    }
}

fn merge_session_json(tmpl_text: &str, join_script: &Value) -> Result<String, String> {
    let mut doc: Map<String, Value> =
        serde_json::from_str(tmpl_text).map_err(|_| "session JSON template does not parse")?;
    merge_into(&mut doc, join_script);
    serde_json::to_string(&Value::Object(doc)).map_err(|e| e.to_string())
}

/// Locate `{"UserId"` in the 0x90 template and the LEB length in front of
/// it: `(json offset, LEB offset, json length)`.
fn split_90(tmpl: &[u8]) -> Result<(usize, usize, usize), String> {
    const PROBE: &[u8] = br#"{"UserId""#;
    let i = tmpl
        .windows(PROBE.len())
        .position(|w| w == PROBE)
        .ok_or("0x90 template has no UserId JSON")?;
    let mut start = i.saturating_sub(1);
    while start > 0 && tmpl[start - 1] & 0x80 != 0 {
        start -= 1;
    }
    let mut len = 0u64;
    let mut shift = 0u32;
    for &b in &tmpl[start..i] {
        len |= u64::from(b & 0x7F) << shift;
        shift += 7;
    }
    // The reference pinned a 24-byte tail, but the tail is whatever follows
    // the JSON: the 2026-10-27 capture carried 24 bytes, the 10-29 one 20.
    // Sixteen is the least the end-anchored tail reads need.
    if i + len as usize + 16 > tmpl.len() {
        return Err("0x90 template is too short for its own length".into());
    }
    Ok((i, start, len as usize))
}

/// The 0x90 join body: the template with its JSON re-sealed around this
/// session's values. The tail is everything after the JSON — 24 bytes in
/// the 2026-10-27 capture the reference pinned, 20 in the 10-29 one — and
/// is spliced verbatim. `body` is what [`seal`] takes; `json` is the JSON
/// that went in.
pub fn build_90(
    tmpl: &[u8],
    join_script: &Value,
    reply: &Value,
) -> Result<(Vec<u8>, String), String> {
    let (json_off, leb_off, json_len) = split_90(tmpl)?;
    let tail_off = json_off + json_len;
    let tmpl_json = std::str::from_utf8(&tmpl[json_off..tail_off])
        .map_err(|_| "0x90 template JSON is not UTF-8".to_string())?;
    let mut doc: Map<String, Value> = serde_json::from_str(tmpl_json)
        .map_err(|_| "0x90 template JSON does not parse".to_string())?;
    merge_into(&mut doc, join_script);
    // Identity fields the join script does not carry stay on the template's
    // values — which belong to whoever that capture was taken as. Pin the
    // appearance id to the joining user; the native's is UserId.
    if let Some(uid) = join_script.get("UserId") {
        if join_script.get("CharacterAppearanceId").is_none() {
            doc.insert("CharacterAppearanceId".into(), uid.clone());
        }
    }
    for key in ["RandomSeed1", "APIsecurityToken"] {
        if let Some(sv) = join_script.get(key) {
            doc.insert(key.into(), Value::String(value_to_string(sv)));
        }
    }
    if let Value::Object(_) = reply {
        match reply.get("joinTicket") {
            Some(jt) if jt.is_object() => {
                let mut inner = Map::new();
                inner.insert(
                    "SerializedClientFields".into(),
                    Value::String(json_string(jt, "SerializedClientFields")),
                );
                inner.insert(
                    "EncryptedServerFields".into(),
                    Value::String(json_string(jt, "EncryptedServerFields")),
                );
                let dumped =
                    serde_json::to_string(&Value::Object(inner)).map_err(|e| e.to_string())?;
                doc.insert("__joinTicket".into(), Value::String(dumped));
            }
            _ => return Err("join reply has no joinTicket (0x90 keeps the template's)".into()),
        }
    }
    let json = serde_json::to_string(&Value::Object(doc)).map_err(|e| e.to_string())?;
    let mut body = Vec::with_capacity(leb_off + json.len() + 25);
    body.extend_from_slice(&tmpl[..leb_off]);
    put_string(&mut body, &json);
    body.extend_from_slice(&tmpl[tail_off..]);
    Ok((body, json))
}

/// The 0x8a ticket body: the ticket blob plus the session document.
pub fn build_8a(
    user_id: u32,
    client_ticket: &str,
    session_template: &[u8],
    join_script: &Value,
) -> Result<Vec<u8>, String> {
    // The session document. The join script's `SessionId` is not a uuid but
    // the whole session document, JSON-encoded; the native submits it
    // verbatim, so the RCC sees this session's ids.
    let mut session_json = String::new();
    let mut have_session_doc = false;
    if let Some(Value::String(raw)) = join_script.get("SessionId") {
        if let Ok(doc) = serde_json::from_str::<Value>(raw) {
            if doc.is_object() {
                session_json = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
                have_session_doc = true;
            }
        }
    }
    if !have_session_doc {
        // Fallback for a join script without the embedded document: take
        // the shape from the captured 0x8a payload and merge this session.
        if !session_template.is_empty() {
            let parsed = parse_8a_payload(session_template)?;
            if !parsed.session_json.is_empty() {
                session_json = merge_session_json(&parsed.session_json, join_script)?;
            }
        }
    }
    let zz = (u64::from(user_id) << 1) ^ ((user_id as i64) >> 63) as u64;
    let v31 = ticket_hash(client_ticket);
    let v8 = 0u32.wrapping_sub(TICKET_CONST.wrapping_mul(HASH_CONST)) as u16;
    let v36 = if v8 & 0x8000 != 0 {
        i64::from(v31)
    } else {
        -i64::from(v31)
    };

    let mut body = Vec::with_capacity(client_ticket.len() + session_json.len() + 64);
    put_leb(&mut body, zz);
    put_string(&mut body, client_ticket);
    body.extend_from_slice(&36u32.to_le_bytes());
    put_string(&mut body, CONST_74);
    put_string(&mut body, "Android");
    put_string(&mut body, "?");
    put_leb(&mut body, v36 as u64);
    put_leb(&mut body, v36.wrapping_sub(BAD_F00D) as u64);
    put_string(&mut body, "");
    put_string(&mut body, &session_json);
    body.extend_from_slice(&0xC001_CAFEu32.to_le_bytes());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xxh32_matches_the_reference_vectors() {
        // Ground truth from the reference xxHash implementation; the inputs
        // cover every loop: empty, the byte tail, one 16-byte stripe exactly,
        // and stripes plus a remainder.
        assert_eq!(xxh32(b"", 0), 0x02CC_5D05);
        assert_eq!(xxh32(b"abc", 0), 0x32D1_53FF);
        assert_eq!(xxh32(b"abc", 1), 0xAA3D_A8FF);
        assert_eq!(xxh32(b"0123456789abcdef", 0), 0xC2C4_5B69);
        assert_eq!(xxh32(b"0123456789abcdef", 1), 0xF31F_E1CF);
        let long: Vec<u8> = (0..70u8).collect();
        assert_eq!(xxh32(&long, 1), 0x7B99_6B2A);
        let longer: Vec<u8> = (0..71u8).collect();
        assert_eq!(xxh32(&longer, 1), 0xE835_BADD);
    }

    #[test]
    fn ticket_hash_is_stable_and_ticket_sensitive() {
        // The absolute pin (391596346 for the captured 510-byte ticket) is
        // account data, so it is checked in the capture-validation stage
        // instead of here; what these pin is that the fold is deterministic
        // and every input byte matters.
        assert_eq!(ticket_hash("ticket"), ticket_hash("ticket"));
        assert_ne!(ticket_hash("ticket"), ticket_hash("tickes"));
        assert_ne!(ticket_hash("ticket"), ticket_hash("tickett"));
        assert_ne!(ticket_hash(""), ticket_hash("x"));
    }

    #[test]
    fn seal_roundtrips_every_padding_shape() {
        for len in [0usize, 1, 2, 15, 16, 17, 100, 1000] {
            let plain: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let sealed = seal(&plain, &KEY_90, STATE_90);
            assert_eq!(sealed.len() % 16, 0, "len {len} must seal to whole blocks");
            let (back, pad) = unseal(&sealed, &KEY_90).expect("unseal must invert seal");
            // The pad nibble lives in the PLAINTEXT header at [5]; the
            // sealed byte there is ciphertext and carries nothing.
            assert_eq!(sealed.len(), plain.len() + pad + 6);
            assert_eq!(back, plain, "roundtrip failed for len {len}");
            assert_eq!(pad, (!(len + 5)) & 0x0F);
            // Deterministic for (key, state, len), and the state feeds the
            // padding — a different state must move bytes.
            assert_eq!(seal(&plain, &KEY_90, STATE_90), sealed);
            assert_ne!(seal(&plain, &KEY_90, STATE_8A), sealed);
        }
    }

    #[test]
    fn unseal_rejects_shapes_the_native_never_emits() {
        assert!(unseal(&[], &KEY_90).is_none());
        assert!(unseal(&[0u8; 15], &KEY_90).is_none());
        // A header whose pad runs past the end of the body cannot be a real
        // envelope. Encrypt a forged first block directly so the pad byte
        // survives decryption.
        let mut first = [0u8; 16];
        first[5] = 0x0B; // pad 11, but only 16 bytes in the body
        let cipher = Aes128::new(GenericArray::from_slice(&KEY_90));
        encrypt_block(&cipher, &mut first);
        assert!(unseal(&first, &KEY_90).is_none());
    }

    #[test]
    fn sealed_message_prepends_the_id() {
        let msg = sealed_message(0x90, b"body", &KEY_90, STATE_90);
        assert_eq!(msg[0], 0x90);
        assert_eq!(msg[1..], seal(b"body", &KEY_90, STATE_90)[..]);
    }

    #[test]
    fn key_8a_takes_the_captured_form() {
        // Pinned by the C++ suite: place 1818, tail[2] 551761825.
        assert_eq!(key_8a(1818, 551761825), "1818-17338116125");
        assert_eq!(key_8a(1818, 551761825).len(), 16);
        // Live place ids sit past u32 — the decimal is taken whole and
        // the truncation at 16 bytes simply lands further along.
        assert_eq!(key_8a(8867499317, 551761825), "8867499317-17338");
        assert_eq!(key_8a(8867499317, 551761825).len(), 16);
    }

    #[test]
    fn template_tail_helpers_read_the_build_constants() {
        let mut tail = [0u8; 24];
        tail[8..12].copy_from_slice(&0x0000_ABCDu32.to_le_bytes());
        tail[12..16].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        let mut tmpl = vec![0u8; 8];
        tmpl.extend_from_slice(&tail);
        assert_eq!(tail2_from_90_template(&tmpl), Some(0xABCD));
        // v9 = t3 ^ (u16(t2) | 0x22640000) = 0x11223344 ^ 0x2264ABCD.
        assert_eq!(v9_from_90_template(&tmpl), Some(0x3346_9889));
        assert_eq!(tail2_from_90_template(&[0u8; 20]), None);
        assert_eq!(v9_from_90_template(&[0u8; 20]), None);
    }

    #[test]
    fn build_92_roundtrips_its_rand() {
        let msg = build_92(0x6F72_8F47, 0x7F1E_EC34);
        assert_eq!(msg[0], 0x92);
        // The captured shape: 0x92 plus a 10-byte varint.
        assert_eq!(msg.len(), 11);
        let mut v = 0u64;
        let mut shift = 0u32;
        for &b in &msg[1..] {
            v |= u64::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        assert_eq!(v >> 33, 0x6F72_8F47);
    }

    #[test]
    fn build_8f_is_the_id_and_one_string() {
        assert_eq!(build_8f(""), vec![0x8F, 0x00]);
        assert_eq!(
            build_8f("srv-1"),
            vec![0x8F, 0x05, b's', b'r', b'v', b'-', b'1']
        );
    }

    #[test]
    fn the_9b_forms_match_the_capture() {
        // run/fresh_9b.bin: [9b][a][b], both u32 LE.
        assert_eq!(
            build_9b_step(0xBEFF_CA1B, 0x7090_A3B6),
            vec![0x9B, 0x1B, 0xCA, 0xFF, 0xBE, 0xB6, 0xA3, 0x90, 0x70]
        );
        assert_eq!(build_9b_response(2, 0xDEAD_BEEF)[0], 0x9B);
        // The server form: a 13-byte header plus a blob that fits.
        let mut challenge = vec![0x9B, 1, 0, 0, 0, 2, 0, 0, 0];
        challenge.extend_from_slice(&4u32.to_le_bytes());
        challenge.extend_from_slice(&[0xA0, 0xA1, 0xA2, 0xA3]);
        assert!(is_challenge(&challenge));
        challenge.pop();
        assert!(!is_challenge(&challenge));
        assert!(!is_challenge(&[0x9B; 12]));
    }

    fn template_90(json: &str) -> Vec<u8> {
        let mut t = vec![0x01, 0x02, 0x03];
        put_leb(&mut t, json.len() as u64);
        t.extend_from_slice(json.as_bytes());
        let mut tail = [0u8; 24];
        tail[8..12].copy_from_slice(&0x0000_ABCDu32.to_le_bytes());
        tail[12..16].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        t.extend_from_slice(&tail);
        t
    }

    #[test]
    fn build_90_without_substitutions_is_the_template() {
        // What proves the JSON re-serialisation keeps the native's exact
        // formatting: nothing to change means nothing changes.
        let tmpl = template_90(r#"{"UserId":123,"Name":"Template"}"#);
        let (body, json) = build_90(&tmpl, &Value::Null, &Value::Null).expect("builds");
        assert_eq!(body, tmpl);
        assert_eq!(json, r#"{"UserId":123,"Name":"Template"}"#);
    }

    #[test]
    fn build_90_splices_the_session_values() {
        let tmpl = template_90(r#"{"UserId":123,"Name":"Template"}"#);
        let js: Value =
            serde_json::from_str(r#"{"UserId":456,"RandomSeed1":"seed%20one"}"#).unwrap();
        let reply: Value = serde_json::from_str(
            r#"{"joinTicket":{"SerializedClientFields":"a","EncryptedServerFields":"b"}}"#,
        )
        .unwrap();
        let (body, json) = build_90(&tmpl, &js, &reply).expect("builds");
        assert!(json.contains(r#""UserId":456"#));
        assert!(json.contains(r#""RandomSeed1":"seed%20one""#));
        assert!(json.contains(r#""CharacterAppearanceId":456"#));
        assert!(json.contains(
            r#""__joinTicket":"{\"SerializedClientFields\":\"a\",\"EncryptedServerFields\":\"b\"}""#
        ));
        // Prefix, LEB and the 24-byte tail are spliced, not re-encoded.
        assert_eq!(&body[..3], &[0x01, 0x02, 0x03]);
        assert_eq!(&body[body.len() - 24..], &tmpl[tmpl.len() - 24..]);
        // The LEB in front of the JSON still decodes to the JSON length.
        let mut len = 0u64;
        let mut shift = 0u32;
        let mut i = 3usize;
        loop {
            let b = body[i];
            i += 1;
            len |= u64::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        assert_eq!(len as usize, json.len());
        assert_eq!(&body[i..i + json.len()], json.as_bytes());
        // An object reply without a joinTicket is an error, not a guess.
        let err = build_90(&tmpl, &Value::Null, &serde_json::json!({}));
        assert!(err.is_err());
    }

    #[test]
    fn build_90_keeps_template_order_and_kinds() {
        let tmpl = template_90(r#"{"UserId":1,"A":1,"B":"x","C":{"D":true,"E":2}}"#);
        let js: Value = serde_json::from_str(r#"{"C":{"E":9},"B":{"z":1},"F":0}"#).unwrap();
        let (_, json) = build_90(&tmpl, &js, &Value::Null).unwrap();
        // Order: template keys, nested; B (a string field) stringifies the
        // object; F is not in the template so it never appears; the join
        // script has no UserId, so the template's stays.
        assert_eq!(
            json,
            r#"{"UserId":1,"A":1,"B":"{\"z\":1}","C":{"D":true,"E":9}}"#
        );
    }

    #[test]
    fn build_90_handles_the_shorter_newer_tail() {
        // The 2026-10-29 capture carries a 20-byte tail; the tail is the
        // remainder after the JSON, so both builds splice verbatim.
        let json = r#"{"UserId":7,"Name":"T"}"#;
        let mut t = vec![0x01];
        put_leb(&mut t, json.len() as u64);
        t.extend_from_slice(json.as_bytes());
        t.extend_from_slice(&[0xEE; 20]);
        let (body, _) = build_90(&t, &Value::Null, &Value::Null).expect("builds");
        assert_eq!(body, t);
        let js: Value = serde_json::from_str(r#"{"UserId":8}"#).unwrap();
        let (body, json2) = build_90(&t, &js, &Value::Null).expect("builds");
        assert!(json2.contains(r#""UserId":8"#));
        assert_eq!(&body[body.len() - 20..], &[0xEE; 20]);
    }

    #[test]
    fn build_8a_roundtrips_through_the_parser() {
        let js: Value =
            serde_json::from_str(r#"{"SessionId":"{\"SessionId\":\"uuid-1\",\"GameId\":321}"}"#)
                .unwrap();
        let body = build_8a(1234, "TICKET", &[], &js).expect("builds");
        let parsed = parse_8a_payload(&body).expect("parses");
        assert_eq!(parsed.user_id, 1234);
        assert_eq!(parsed.ticket, "TICKET");
        assert_eq!(
            parsed.session_json,
            r#"{"SessionId":"uuid-1","GameId":321}"#
        );
        assert_eq!(&body[body.len() - 4..], &0xC001_CAFEu32.to_le_bytes());
    }

    #[test]
    fn build_8a_falls_back_to_the_captured_shape() {
        // First build carries a session document, then that payload becomes
        // the template for a script whose SessionId is not a JSON document.
        let doc: Value =
            serde_json::from_str(r#"{"SessionId":"{\"GameId\":8,\"PlaceId\":1818}"}"#).unwrap();
        let seed = build_8a(7, "t0", &[], &doc).expect("builds");
        let parsed = parse_8a_payload(&seed).expect("parses");
        assert_eq!(parsed.session_json, r#"{"GameId":8,"PlaceId":1818}"#);
        let js: Value = serde_json::from_str(r#"{"SessionId":"not json","GameId":9}"#).unwrap();
        let body = build_8a(7, "t0", &seed, &js).expect("builds");
        let parsed = parse_8a_payload(&body).expect("parses");
        assert_eq!(parsed.session_json, r#"{"GameId":9,"PlaceId":1818}"#);
    }
}
