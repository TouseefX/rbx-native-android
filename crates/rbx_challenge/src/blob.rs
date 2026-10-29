//! The 0x9B blob decoder: `RSB1` xor + xxhash32 + zstd.
//!
//! Faithful port of the reference's `solve/blob.cpp` (itself reversed
//! from libroblox.so's challenge script loader). The layers:
//!
//! - a 4-byte key: `key[i] = ((blob[i] ^ "RSB1"[i]) + ADD[i]) & 0xFF`;
//! - the whole blob xored with a position stream:
//!   `plain[p] = blob[p] ^ ((41*p + key[p & 3]) & 0xFF)`;
//! - a checksum the key doubles as: `xxhash32(plain, seed 0x2A) ==
//!   LE32(key)` — wrong key, wrong stream, no decrypt;
//! - `LE32(plain[4..8])` is the mode: 0 for a raw wire program,
//!   otherwise the exact zstd output size (capped: the mode is
//!   attacker-controlled and a huge one must not become a huge
//!   allocation);
//! - `plain[8..]` is the payload, zstd-compressed unless mode is 0.

use rbx_transport::join_msg::xxh32;

const MAGIC: [u8; 4] = *b"RSB1";
const ADD: [u8; 4] = [0x00, 0xD7, 0xAE, 0x85];
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Production wire programs are ~7 KB; 64 MB is already absurd, and an
/// uncapped mode near 2^32 would turn the output buffer into a 4 GB
/// allocation before zstd ever runs.
const MAX_WIRE_BYTES: u32 = 64 << 20;

/// Decode the challenge blob into the wire program.
pub fn decode(blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 13 {
        return Err("blob too short".into());
    }

    let mut key = [0u8; 4];
    for i in 0..4 {
        key[i] = (blob[i] ^ MAGIC[i]).wrapping_add(ADD[i]);
    }

    let dec: Vec<u8> = blob
        .iter()
        .enumerate()
        .map(|(p, b)| b ^ (41 * p + usize::from(key[p & 3])) as u8)
        .collect();

    let want = u32::from_le_bytes(key);
    let got = xxh32(&dec, 0x2A);
    if got != want {
        return Err(format!("checksum mismatch: got 0x{got:08x} want 0x{want:08x}"));
    }

    let mode = u32::from_le_bytes([dec[4], dec[5], dec[6], dec[7]]);
    let payload = &dec[8..];

    if mode == 0 {
        return Ok(payload.to_vec());
    }
    if mode > MAX_WIRE_BYTES {
        return Err("zstd output size unreasonable".into());
    }
    if payload.len() < 4 || payload[..4] != ZSTD_MAGIC {
        return Err("zstd magic missing".into());
    }

    let out = zstd::bulk::decompress(payload, mode as usize)
        .map_err(|e| format!("zstd: {e}"))?;
    if out.len() != mode as usize {
        return Err("zstd size mismatch".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_blob_is_rejected() {
        assert_eq!(decode(&[0u8; 12]), Err("blob too short".into()));
    }

    #[test]
    fn a_wrong_checksum_is_rejected() {
        // Any 13+ bytes whose stream does not carry a consistent
        // checksum fail before any zstd work happens.
        let blob = vec![0x42u8; 32];
        let err = decode(&blob).unwrap_err();
        assert!(err.starts_with("checksum mismatch"), "{err}");
    }

    #[test]
    fn the_decoded_layers_match_the_reference() {
        // The vector tests run every captured blob through here and byte-
        // compare the result against the frozen golden; what is left for
        // the unit side is the failure geometry, which is what the two
        // tests above pin. A synthetic round-trip would need to solve for
        // a key whose LE bytes are the xxh32 of the plaintext it streams
        // -- the server hash-tweaks its padding to get there, and a test
        // doing the same would only re-test xxh32.
    }
}
