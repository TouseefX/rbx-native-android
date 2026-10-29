//! The native client's RakNet session crypto, ported from Boblox's
//! `session_crypto.cpp` (reverse-engineered from `libroblox.so`; see the
//! reference's FINDINGS.md "RakNet session crypto") **[GAME]**.
//!
//! ```text
//! shared  = X25519(our ephemeral secret, peer EphemeralEarlyPubKey)
//! digest  = BLAKE2b-512( shared || our public || peer public )
//! rx key  = digest[0..32]     (server -> client)
//! tx key  = digest[32..64]    (client -> server)
//! ```
//!
//! The same derivation runs a second time for the post-handshake datagram
//! keys, against the data peer key that arrives inside Reply2 — see
//! [`SessionKeys::install_data`].
//!
//! Two AEADs sit on top: every handshake packet is IETF ChaCha20-Poly1305
//! with the fixed 12-byte ASCII nonce "UniqueNumber" and the wire layout
//! `[aad][ciphertext][nonce][tag]` ([`aead_seal`]/[`aead_open`]); every
//! post-handshake datagram is AES-256-GCM with nonce `LE64(counter) ||
//! "mbeR"`, no AAD, and the wire layout `[ciphertext][counter low16 LE]
//! [tag]` ([`datagram_seal`]/[`datagram_open`]). Both directions were
//! verified byte-for-byte against a live session by the reference.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use blake2::digest::Digest;
use chacha20poly1305::ChaCha20Poly1305;
use rand_core::RngCore;
use x25519_dalek::{PublicKey, StaticSecret};

/// The 12-byte handshake nonce (ASCII "UniqueNumber"), transmitted
/// verbatim inside every handshake packet.
pub const HANDSHAKE_NONCE: [u8; 12] = *b"UniqueNumber";

/// The post-handshake datagram counter base: `LE64("UniqueNu")`. Both
/// directions start here and increment by one per datagram; only the low
/// 16 bits travel on the wire.
pub const DATA_COUNTER_BASE: u64 = 0x754e_6575_7169_6e55;

/// One session's key material: our X25519 keypair, the handshake AEAD
/// keys, and (once Reply2 is in) the datagram AEAD keys.
///
/// The C++ original keeps the secret in a plain buffer; here it is a
/// [`StaticSecret`], which zeroizes on drop.
pub struct SessionKeys {
    secret: StaticSecret,
    public: [u8; 32],
    /// Client -> server, ChaCha20-Poly1305 (handshake).
    tx: [u8; 32],
    /// Server -> client, ChaCha20-Poly1305 (handshake).
    rx: [u8; 32],
    /// Client -> server, AES-256-GCM (datagrams).
    data_tx: [u8; 32],
    /// Server -> client, AES-256-GCM (datagrams).
    data_rx: [u8; 32],
}

impl SessionKeys {
    /// A fresh random X25519 keypair — the native's per-session ephemeral
    /// pair. The AEAD keys stay zero until [`install`](Self::install).
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        rand_core::OsRng.fill_bytes(&mut seed);
        Self::from_seed(seed)
    }

    /// A keypair from an explicit 32-byte seed — the test and replay path.
    /// X25519 clamps the scalar internally, so the public key is
    /// `X25519(seed, 9)` exactly as the native's key exchange computes it.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let secret = StaticSecret::from(seed);
        let public = PublicKey::from(&secret);
        SessionKeys {
            secret,
            public: *public.as_bytes(),
            tx: [0u8; 32],
            rx: [0u8; 32],
            data_tx: [0u8; 32],
            data_rx: [0u8; 32],
        }
    }

    /// Our ephemeral public key — the 32 bytes that go out in Req2.
    pub fn public(&self) -> [u8; 32] {
        self.public
    }

    /// The handshake AEAD keys, client -> server.
    pub fn tx(&self) -> &[u8; 32] {
        &self.tx
    }

    /// The handshake AEAD keys, server -> client.
    pub fn rx(&self) -> &[u8; 32] {
        &self.rx
    }

    /// The datagram AEAD keys, client -> server (installed by
    /// [`install_data`](Self::install_data)).
    pub fn data_tx(&self) -> &[u8; 32] {
        &self.data_tx
    }

    /// The datagram AEAD keys, server -> client.
    pub fn data_rx(&self) -> &[u8; 32] {
        &self.data_rx
    }

    /// Install the peer's handshake public key (the join script's
    /// `EphemeralEarlyPubKey`) and derive the handshake tx/rx pair.
    pub fn install(&mut self, peer_public: [u8; 32]) {
        let (rx, tx) = derive_pair(&self.secret, &self.public, &peer_public);
        self.rx = rx;
        self.tx = tx;
    }

    /// Install the peer's data public key and derive the datagram
    /// tx/rx pair. The peer key is reconstructed from Reply2 — 23 bytes
    /// of its AAD tail plus the first 9 bytes of the decrypted plaintext
    /// — and the KDF is the same BLAKE2b-512 over the second DH.
    pub fn install_data(&mut self, peer_public: [u8; 32]) {
        let (data_rx, data_tx) = derive_pair(&self.secret, &self.public, &peer_public);
        self.data_rx = data_rx;
        self.data_tx = data_tx;
    }
}

/// The KDF both key derivations share:
/// `BLAKE2b-512(shared || our_pub || peer_pub)`, split into an rx half and
/// a tx half.
fn derive_pair(
    secret: &StaticSecret,
    our_public: &[u8; 32],
    peer_public: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let shared = secret.diffie_hellman(&PublicKey::from(*peer_public));
    let mut hasher = blake2::Blake2b512::new();
    hasher.update(shared.as_bytes());
    hasher.update(our_public);
    hasher.update(peer_public);
    let digest = hasher.finalize();
    let mut rx = [0u8; 32];
    let mut tx = [0u8; 32];
    rx.copy_from_slice(&digest[0..32]);
    tx.copy_from_slice(&digest[32..64]);
    (rx, tx)
}

/// Rebuild a full 64-bit datagram counter from the 16-bit wire hint,
/// nearest to the last counter seen in that direction. A hint more than
/// `0x8000` behind the last counter has wrapped forward, and one that far
/// ahead has wrapped back.
pub fn counter_from_hint(hint: u16, last: u64) -> u64 {
    let mut counter = (last & !0xffff_u64) | u64::from(hint);
    if counter < last && last - counter > 0x8000 {
        counter += 0x10000;
    } else if counter > last && counter - last > 0x8000 {
        counter -= 0x10000;
    }
    counter
}

/// The datagram nonce: `LE64(counter) || "mbeR"`.
fn datagram_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0..8].copy_from_slice(&counter.to_le_bytes());
    nonce[8..12].copy_from_slice(b"mbeR");
    nonce
}

/// Seal one post-handshake datagram: `[ciphertext][counter low16 LE]
/// [tag]`, AES-256-GCM, no AAD. The counter only rides as its low 16
/// bits — [`datagram_open`] reconstructs the rest.
pub fn datagram_seal(key: &[u8; 32], counter: u64, plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce = datagram_nonce(counter);
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .expect("AES-256-GCM over in-memory buffers cannot fail");
    let ciphertext_len = plaintext.len();
    let mut frame = Vec::with_capacity(ciphertext_len + 18);
    frame.extend_from_slice(&sealed[..ciphertext_len]);
    frame.extend_from_slice(&(counter as u16).to_le_bytes());
    frame.extend_from_slice(&sealed[ciphertext_len..]);
    frame
}

/// Open and authenticate one post-handshake datagram against the last
/// counter seen in that direction. The wire hint can land next to the
/// true counter, or one either side of it, so all three are tried —
/// exactly the native's candidate set.
pub fn datagram_open(key: &[u8; 32], frame: &[u8], last_counter: u64) -> Option<(Vec<u8>, u64)> {
    if frame.len() < 18 {
        return None;
    }
    let ciphertext_len = frame.len() - 18;
    let hint = u16::from_le_bytes([frame[ciphertext_len], frame[ciphertext_len + 1]]);
    let near = counter_from_hint(hint, last_counter);
    let cipher = Aes256Gcm::new(key.into());
    // Reassemble the sealed buffer the AEAD expects: ciphertext || tag.
    let mut sealed = Vec::with_capacity(frame.len() - 2);
    sealed.extend_from_slice(&frame[..ciphertext_len]);
    sealed.extend_from_slice(&frame[ciphertext_len + 2..]);
    for counter in [near, near.wrapping_sub(1), near.wrapping_add(1)] {
        let nonce = datagram_nonce(counter);
        if let Ok(plaintext) = cipher.decrypt(Nonce::from_slice(&nonce), sealed.as_slice()) {
            return Some((plaintext, counter));
        }
    }
    None
}

/// Seal a handshake packet in place. `buf` is the whole packet,
/// `[aad][plaintext][nonce 12][tag 16]` — the first `aad_len` bytes are
/// the authenticated-but-not-encrypted header, the plaintext up to
/// `nonce_offset` is replaced by its ciphertext, and the 16-byte tag is
/// written at `nonce_offset + 12`. ChaCha20-Poly1305 with the packet's
/// own in-buffer nonce.
pub fn aead_seal(key: &[u8; 32], buf: &mut [u8], nonce_offset: usize, aad_len: usize) -> bool {
    if nonce_offset < aad_len || nonce_offset + 28 > buf.len() {
        return false;
    }
    let plaintext_len = nonce_offset - aad_len;
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&buf[nonce_offset..nonce_offset + 12]);
    let cipher = ChaCha20Poly1305::new(key.into());
    let sealed = {
        let (aad, rest) = buf.split_at(aad_len);
        match cipher.encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &rest[..plaintext_len],
                aad,
            },
        ) {
            Ok(sealed) => sealed,
            Err(_) => return false,
        }
    };
    buf[aad_len..nonce_offset].copy_from_slice(&sealed[..plaintext_len]);
    buf[nonce_offset + 12..nonce_offset + 28].copy_from_slice(&sealed[plaintext_len..]);
    true
}

/// Verify and open the layout [`aead_seal`] produces. `None` when the tag
/// does not verify — wrong key or tampered packet — which is the normal
/// "not the server's reply" outcome, not an error.
pub fn aead_open(
    key: &[u8; 32],
    buf: &[u8],
    nonce_offset: usize,
    aad_len: usize,
) -> Option<Vec<u8>> {
    if nonce_offset < aad_len || nonce_offset + 28 > buf.len() {
        return None;
    }
    let ciphertext_len = nonce_offset - aad_len;
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&buf[nonce_offset..nonce_offset + 12]);
    let mut tag = [0u8; 16];
    tag.copy_from_slice(&buf[nonce_offset + 12..nonce_offset + 28]);
    let mut sealed = Vec::with_capacity(ciphertext_len + 16);
    sealed.extend_from_slice(&buf[aad_len..nonce_offset]);
    sealed.extend_from_slice(&tag);
    let cipher = ChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: sealed.as_slice(),
                aad: &buf[..aad_len],
            },
        )
        .ok()?;
    Some(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a hex string — the form the public test vectors come in.
    fn unhex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn bytes32(s: &str) -> [u8; 32] {
        unhex(s).try_into().unwrap()
    }

    /// RFC 7748 §6.1: the Diffie-Hellman test vectors.
    #[test]
    fn x25519_matches_rfc7748() {
        let seed_a = bytes32("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let seed_b = bytes32("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let pub_a = bytes32("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        let pub_b = bytes32("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
        let shared = bytes32("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

        let mut alice = SessionKeys::from_seed(seed_a);
        let mut bob = SessionKeys::from_seed(seed_b);
        assert_eq!(alice.public(), pub_a);
        assert_eq!(bob.public(), pub_b);

        // The KDF's first input is the RFC 7748 shared secret; pin it
        // through the same primitive the port uses.
        let direct = StaticSecret::from(seed_a).diffie_hellman(&PublicKey::from(pub_b));
        assert_eq!(direct.as_bytes(), &shared);

        // The derived pair is directional and symmetric between peers.
        alice.install(bob.public());
        bob.install(alice.public());
        assert_eq!(alice.tx(), bob.rx());
        assert_eq!(alice.rx(), bob.tx());
        assert_ne!(alice.tx(), alice.rx());

        // The datagram derivation is independent: another peer key, another
        // pair, and it does not disturb the handshake pair.
        let third = SessionKeys::generate();
        alice.install_data(third.public());
        assert_ne!(alice.data_tx(), alice.tx());
        assert_ne!(alice.data_rx(), alice.rx());
    }

    /// BLAKE2b-512 reference digests (RFC 7693 test vectors).
    #[test]
    fn blake2b512_known_digests() {
        fn digest_of(input: &[u8]) -> Vec<u8> {
            let mut hasher = blake2::Blake2b512::new();
            hasher.update(input);
            hasher.finalize().to_vec()
        }

        let mut want = unhex("786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419");
        want.extend(unhex(
            "d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce",
        ));
        assert_eq!(digest_of(b""), want);

        let mut want = unhex("ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1");
        want.extend(unhex(
            "7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923",
        ));
        assert_eq!(digest_of(b"abc"), want);
    }

    #[test]
    fn handshake_aead_roundtrip_and_tamper() {
        let mut keys = SessionKeys::generate();
        keys.install(SessionKeys::generate().public());
        let key = *keys.tx();

        // [aad][plaintext][nonce][tag], the native handshake layout.
        let mut packet = Vec::new();
        packet.extend_from_slice(&[0x7d, 0x00, 0x11, 0x02]);
        let plaintext = b"join ticket part three";
        packet.extend_from_slice(plaintext);
        packet.extend_from_slice(&HANDSHAKE_NONCE);
        packet.extend_from_slice(&[0u8; 16]);
        let (nonce_offset, aad_len) = (4 + plaintext.len(), 4);

        let original = packet.clone();
        assert!(aead_seal(&key, &mut packet, nonce_offset, aad_len));
        assert_eq!(&packet[..aad_len], &original[..aad_len]);
        assert_eq!(packet[nonce_offset..nonce_offset + 12], HANDSHAKE_NONCE[..]);
        assert_ne!(&packet[aad_len..nonce_offset], plaintext.as_slice());
        assert_ne!(packet[nonce_offset + 12..nonce_offset + 28], [0u8; 16]);

        let opened = aead_open(&key, &packet, nonce_offset, aad_len).unwrap();
        assert_eq!(opened, plaintext);

        // One flipped ciphertext byte must fail the tag.
        let mut tampered = packet.clone();
        tampered[aad_len] ^= 0x01;
        assert!(aead_open(&key, &tampered, nonce_offset, aad_len).is_none());

        // So must a flipped AAD byte.
        let mut tampered = packet.clone();
        tampered[0] ^= 0x01;
        assert!(aead_open(&key, &tampered, nonce_offset, aad_len).is_none());

        // And a layout that leaves no room for the nonce and tag.
        assert!(!aead_seal(&key, &mut packet, packet.len(), aad_len));
    }

    #[test]
    fn datagram_roundtrip_and_counter_reconstruction() {
        let key = [7u8; 32];
        let counter = DATA_COUNTER_BASE + 5;
        let frame = datagram_seal(&key, counter, b"replication bytes");
        assert_eq!(frame.len(), b"replication bytes".len() + 18);

        let (opened, used) = datagram_open(&key, &frame, DATA_COUNTER_BASE).unwrap();
        assert_eq!(opened, b"replication bytes");
        assert_eq!(used, counter);

        // A tampered frame fails authentication.
        let mut tampered = frame.clone();
        tampered[0] ^= 0x01;
        assert!(datagram_open(&key, &tampered, DATA_COUNTER_BASE).is_none());

        // The reference's counter base really is LE64("UniqueNu").
        assert_eq!(&DATA_COUNTER_BASE.to_le_bytes(), b"UniqueNu");
    }

    #[test]
    fn counter_hints_wrap_both_ways() {
        // In range: the hint lands next to the last counter.
        assert_eq!(counter_from_hint(0x0005, 0x0010), 0x0005);
        // Forward wrap: the hint is far behind the last counter.
        assert_eq!(counter_from_hint(0x0000, 0x1fff0), 0x20000);
        assert_eq!(counter_from_hint(0x00ff, 0x1ff10), 0x200ff);
        // Backward wrap: the hint is far ahead of the last counter.
        assert_eq!(counter_from_hint(0xff00, 0x20010), 0x1ff00);
        // Exactly at the 0x8000 boundary is ambiguous and stays put; one
        // past it wraps forward.
        assert_eq!(counter_from_hint(0x0000, 0x08000), 0x0000);
        assert_eq!(counter_from_hint(0x0000, 0x08001), 0x10000);
    }
}
