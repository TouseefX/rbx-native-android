//! The offline handshake — Roblox's own packet ids, not stock RakNet's,
//! ported from the `sendReq1`/`sendReq2`/`handleReply2` paths of Boblox's
//! `raknet.cpp` **[GAME]**:
//!
//! ```text
//! C->S  7b <magic> 05 <pad to 1129>              OpenConnectionRequest1
//! S->C  7e <magic> <guid 8> <sec> <mtu BE16>     OpenConnectionReply1
//! C->S  78 <magic> 03 <aadlen> <ctlen BE16> ...  OpenConnectionRequest2
//! S->C  7d <magic> 01 <aadlen> ...               OpenConnectionReply2
//! ```
//!
//! Everything after Reply1 is encrypted: Req2's tail and Reply2's tail
//! are the handshake AEAD's `[aad][ciphertext][nonce][tag]` layout, and a
//! verifying Reply2 tag is the server's acceptance of the Req2 ticket and
//! ephemeral key. This module is the pure byte layer — building and
//! parsing — with no socket; the client state machine sits on top.

use crate::crypto::{aead_open, aead_seal, HANDSHAKE_NONCE};

/// The RakNet protocol magic, sixteen bytes.
pub const MAGIC: [u8; 16] = *b"\x00\xff\xff\x00\xfe\xfe\xfe\xfe\xfd\xfd\xfd\xfd\x124Vx";

/// `OpenConnectionRequest1`.
pub const REQ1: u8 = 0x7b;
/// `OpenConnectionReply1`.
pub const REPLY1: u8 = 0x7e;
/// `OpenConnectionRequest2` — the encrypted one.
pub const REQ2: u8 = 0x78;
/// `OpenConnectionReply2` — the encrypted one.
pub const REPLY2: u8 = 0x7d;

/// What Reply1 told us: the server's GUID and the negotiated MTU
/// (native: 1200).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply1 {
    pub guid: u64,
    pub mtu: u16,
}

/// Build Req1: the id, the magic, the protocol version byte, then zero
/// padding out to the probe size (native: a 1129-byte payload in a
/// 1160-byte datagram). `None` when the probe cannot even hold the
/// 18-byte header.
pub fn build_req1(protocol_version: u8, probe_size: usize) -> Option<Vec<u8>> {
    if probe_size < 18 {
        return None;
    }
    let mut packet = vec![0u8; probe_size];
    packet[0] = REQ1;
    packet[1..17].copy_from_slice(&MAGIC);
    packet[17] = protocol_version;
    Some(packet)
}

/// Parse Reply1. `None` unless the id, the magic and the length all
/// match; the bytes between the GUID and the MTU are the reference's
/// uninterpreted security byte.
pub fn parse_reply1(payload: &[u8]) -> Option<Reply1> {
    if payload.len() < 28 || payload[0] != REPLY1 || payload[1..17] != MAGIC {
        return None;
    }
    Some(Reply1 {
        guid: u64::from_le_bytes(payload[17..25].try_into().ok()?),
        mtu: u16::from_be_bytes([payload[26], payload[27]]),
    })
}

/// The join-script material Req2's encrypted tail carries, all of it
/// straight from the reference's `RaknetConfig`.
#[derive(Clone, Copy, Debug)]
pub struct Req2Inputs<'a> {
    /// Our ephemeral X25519 public key — the 32 bytes that travel in the
    /// clear and key the exchange.
    pub public_key: &'a [u8; 32],
    /// `ClientTicket` part 2 (native: 33 bytes).
    pub ticket_pre: &'a [u8],
    /// `ClientTicket` part 3 (native: 66 bytes).
    pub ticket_auth: &'a [u8],
    /// The capability bitmask the native puts first in the plaintext,
    /// built from client FFlags — constant for a given build. The value
    /// captured from the live 2026-10-27 session, byte order as sent.
    pub capability_word: [u8; 8],
    /// The per-session connection id (native: an 8-byte NetworkClient
    /// method result, different every session).
    pub session_id: [u8; 8],
    /// Reply1's MTU.
    pub mtu: u16,
    /// Req2's protocol field (native: 5).
    pub protocol_version: u8,
    /// Req2's plaintext `[18]` (native: 2).
    pub encryption_mode: u8,
    /// The RUPP application id (native: 4).
    pub application_id: u8,
    /// The server's IPv4, network order — sent bitwise-inverted.
    pub peer_ip: [u8; 4],
    /// The server's port, host order.
    pub peer_port: u16,
    /// The probe size Req1 already used (native: 1129).
    pub probe_size: usize,
}

/// Build Req2 and seal its tail with the handshake tx key. The layout:
///
/// ```text
/// 78 <magic> 03 <aadlen u8> <ctlen BE16> 00 <ver> <client pub 32B>
/// 11 <prelen> <pre blob> | plaintext: <caps 8B> <session id 8B>
/// <mtu BE16> <enc mode> <app id> <~ip BE32> <port BE16>
/// <authlen> <auth blob> | nonce 12B | tag 16B | zero padding
/// ```
///
/// `None` when the ticket parts are empty or past the one-byte lengths
/// the header can name, when the derived packet would not fit the probe
/// size, or when the AEAD rejects the buffer.
pub fn build_req2(tx_key: &[u8; 32], inputs: &Req2Inputs<'_>) -> Option<Vec<u8>> {
    let pre = inputs.ticket_pre.len();
    let auth = inputs.ticket_auth.len();
    if pre == 0 || pre > 250 || auth == 0 || auth > 250 {
        return None;
    }
    let aad_len = 57 + pre;
    let plaintext_len = 27 + auth;
    let nonce_off = aad_len + plaintext_len;
    if aad_len > u8::MAX as usize || nonce_off + 28 > inputs.probe_size {
        return None;
    }

    let mut packet = vec![0u8; inputs.probe_size];
    packet[0] = REQ2;
    packet[1..17].copy_from_slice(&MAGIC);
    packet[17] = 0x03;
    packet[18] = aad_len as u8;
    packet[19..21].copy_from_slice(&(plaintext_len as u16).to_be_bytes());
    packet[21] = 0x00;
    packet[22] = inputs.protocol_version;
    packet[23..55].copy_from_slice(inputs.public_key);
    packet[55] = 0x11;
    packet[56] = pre as u8;
    packet[57..57 + pre].copy_from_slice(inputs.ticket_pre);
    {
        let plaintext = &mut packet[aad_len..nonce_off];
        plaintext[..8].copy_from_slice(&inputs.capability_word);
        plaintext[8..16].copy_from_slice(&inputs.session_id);
        plaintext[16..18].copy_from_slice(&inputs.mtu.to_be_bytes());
        plaintext[18] = inputs.encryption_mode;
        plaintext[19] = inputs.application_id;
        for (dst, src) in plaintext[20..24].iter_mut().zip(&inputs.peer_ip) {
            *dst = !*src;
        }
        plaintext[24..26].copy_from_slice(&inputs.peer_port.to_be_bytes());
        plaintext[26] = auth as u8;
        plaintext[27..].copy_from_slice(inputs.ticket_auth);
    }
    packet[nonce_off..nonce_off + 12].copy_from_slice(&HANDSHAKE_NONCE);
    if !aead_seal(tx_key, &mut packet, nonce_off, aad_len) {
        return None;
    }
    Some(packet)
}

/// A verified Reply2: its decrypted tail, and the peer public key the
/// datagram AEAD keys derive from — 23 bytes of the authenticated header
/// plus the first 9 bytes of the plaintext, per the native's
/// reconstruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply2 {
    pub plaintext: Vec<u8>,
    /// Present when the reply carried enough of both halves to rebuild
    /// the 32-byte data peer key.
    pub peer_data_public: Option<[u8; 32]>,
}

/// Verify and open Reply2 with the handshake rx key. `None` when the id,
/// magic or lengths do not match, or when the tag does not verify — the
/// normal "not the server's acceptance" outcome.
pub fn parse_reply2(rx_key: &[u8; 32], payload: &[u8]) -> Option<Reply2> {
    if payload.len() < 18 + 28 || payload[0] != REPLY2 || payload[1..17] != MAGIC {
        return None;
    }
    let aad_len = usize::from(payload[18]);
    if aad_len + 28 > payload.len() {
        return None;
    }
    let nonce_off = payload.len() - 28;
    let plaintext = aead_open(rx_key, payload, nonce_off, aad_len)?;
    let peer_data_public = if plaintext.len() >= 9 && aad_len >= 44 {
        let mut peer = [0u8; 32];
        peer[..23].copy_from_slice(&payload[21..44]);
        peer[23..].copy_from_slice(&plaintext[..9]);
        Some(peer)
    } else {
        None
    };
    Some(Reply2 {
        plaintext,
        peer_data_public,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::SessionKeys;

    #[test]
    fn req1_is_the_padded_probe() {
        let packet = build_req1(0x05, 1129).unwrap();
        assert_eq!(packet.len(), 1129);
        assert_eq!(packet[0], REQ1);
        assert_eq!(packet[1..17], MAGIC);
        assert_eq!(packet[17], 0x05);
        assert!(packet[18..].iter().all(|b| *b == 0));
        assert!(build_req1(0x05, 17).is_none());
    }

    #[test]
    fn reply1_parses_and_rejects() {
        let mut payload = [0u8; 28];
        payload[0] = REPLY1;
        payload[1..17].copy_from_slice(&MAGIC);
        payload[17..25].copy_from_slice(&0x0123_4567_89ab_cdefu64.to_le_bytes());
        payload[25] = 0x58; // the security byte, uninterpreted.
        payload[26..28].copy_from_slice(&1200u16.to_be_bytes());
        assert_eq!(
            parse_reply1(&payload),
            Some(Reply1 {
                guid: 0x0123_4567_89ab_cdef,
                mtu: 1200
            })
        );

        let mut wrong = payload;
        wrong[0] = 0x7f;
        assert!(parse_reply1(&wrong).is_none());
        let mut wrong = payload;
        wrong[3] ^= 0x01;
        assert!(parse_reply1(&wrong).is_none());
        assert!(parse_reply1(&payload[..27]).is_none());
    }

    #[test]
    fn req2_carries_the_join_material_and_seals() {
        let keys = SessionKeys::from_seed([9u8; 32]);
        let inputs = Req2Inputs {
            public_key: &keys.public(),
            ticket_pre: &[0x33u8; 33],
            ticket_auth: &[0x44u8; 66],
            capability_word: [0x00, 0x2d, 0xd3, 0xc7, 0x50, 0xb7, 0x08, 0x92],
            session_id: [1, 2, 3, 4, 5, 6, 7, 8],
            mtu: 1200,
            protocol_version: 5,
            encryption_mode: 2,
            application_id: 4,
            peer_ip: [10, 190, 0, 5],
            peer_port: 50105,
            probe_size: 1129,
        };
        let packet = build_req2(keys.tx(), &inputs).unwrap();
        assert_eq!(packet.len(), 1129);
        assert_eq!(packet[0], REQ2);
        assert_eq!(packet[1..17], MAGIC);
        assert_eq!(packet[17], 0x03);
        assert_eq!(packet[18], 57 + 33);
        assert_eq!(packet[19..21], (27 + 66u16).to_be_bytes());
        assert_eq!(packet[21], 0x00);
        assert_eq!(packet[22], 5);
        assert_eq!(packet[23..55], keys.public());
        assert_eq!(packet[55], 0x11);
        assert_eq!(packet[56], 33);
        assert_eq!(packet[57..90], [0x33u8; 33]);

        // The sealed tail opens back to exactly the join material.
        let aad_len = 57 + 33;
        let nonce_off = aad_len + 27 + 66;
        assert_eq!(packet[nonce_off..nonce_off + 12], HANDSHAKE_NONCE[..]);
        assert_ne!(packet[nonce_off + 12..nonce_off + 28], [0u8; 16]);
        assert!(packet[nonce_off + 28..].iter().all(|b| *b == 0));
        let plaintext = aead_open(keys.tx(), &packet, nonce_off, aad_len).unwrap();
        assert_eq!(plaintext[..8], inputs.capability_word);
        assert_eq!(plaintext[8..16], inputs.session_id);
        assert_eq!(plaintext[16..18], 1200u16.to_be_bytes());
        assert_eq!(plaintext[18], 2);
        assert_eq!(plaintext[19], 4);
        assert_eq!(plaintext[20..24], [!10, !190, !0, !5]);
        assert_eq!(plaintext[24..26], 50105u16.to_be_bytes());
        assert_eq!(plaintext[26], 66);
        assert_eq!(plaintext[27..], [0x44u8; 66]);

        // The one-byte lengths bound the ticket parts.
        let mut bad = Req2Inputs {
            ticket_pre: &[],
            ..inputs
        };
        assert!(build_req2(keys.tx(), &bad).is_none());
        bad.ticket_pre = &[7u8; 251];
        assert!(build_req2(keys.tx(), &bad).is_none());
        bad.ticket_pre = &[7u8; 33];
        bad.ticket_auth = &[7u8; 251];
        assert!(build_req2(keys.tx(), &bad).is_none());
        // And the packet has to fit the probe.
        let tight = Req2Inputs {
            probe_size: nonce_off + 27,
            ..inputs
        };
        assert!(build_req2(keys.tx(), &tight).is_none());
    }

    /// The full exchange, both directions of the handshake AEAD, with the
    /// server side simulated: Reply2's tag verifies under our rx key, and
    /// the data peer key it yields derives the same datagram keys the
    /// server's own second keypair would.
    #[test]
    fn reply2_verifies_and_yields_the_data_peer_key() {
        let mut alice = SessionKeys::from_seed([1u8; 32]);
        let mut server = SessionKeys::from_seed([2u8; 32]);
        let mut server_data = SessionKeys::from_seed([3u8; 32]);
        alice.install(server.public());
        server.install(alice.public());

        // The server builds Reply2: its fields authenticated as AAD, with
        // the data peer key spread across [21:44] and the plaintext head.
        let server_pub = server_data.public();
        let plaintext = &server_pub[23..];
        let aad_len = 60usize;
        let nonce_off = aad_len + plaintext.len();
        let mut payload = vec![0u8; nonce_off + 28];
        payload[0] = REPLY2;
        payload[1..17].copy_from_slice(&MAGIC);
        payload[17] = 0x01;
        payload[18] = aad_len as u8;
        payload[21..44].copy_from_slice(&server_pub[..23]);
        payload[aad_len..nonce_off].copy_from_slice(plaintext);
        payload[nonce_off..nonce_off + 12].copy_from_slice(&HANDSHAKE_NONCE);
        assert!(aead_seal(server.tx(), &mut payload, nonce_off, aad_len));

        let reply = parse_reply2(alice.rx(), &payload).unwrap();
        assert_eq!(reply.plaintext, plaintext);
        let peer = reply.peer_data_public.unwrap();
        assert_eq!(peer, server_pub);

        // The datagram keys the two sides derive from the two halves
        // agree, in both directions.
        alice.install_data(peer);
        server_data.install(alice.public());
        assert_eq!(alice.data_rx(), server_data.tx());
        assert_eq!(alice.data_tx(), server_data.rx());

        // A tampered tail fails the tag, and so do wrong id and magic.
        let mut tampered = payload.clone();
        tampered[nonce_off] ^= 0x01;
        assert!(parse_reply2(alice.rx(), &tampered).is_none());
        let mut wrong = payload.clone();
        wrong[0] = 0x7c;
        assert!(parse_reply2(alice.rx(), &wrong).is_none());
        let mut wrong = payload;
        wrong[4] ^= 0x01;
        assert!(parse_reply2(alice.rx(), &wrong).is_none());
    }
}
