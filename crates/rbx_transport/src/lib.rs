//! The RakNet transport a Boblox-style client speaks, ported.
//!
//! Stage three of the Team Create port described by
//! `replication-and-place-guide.md`: the UDP transport that carries the
//! replication stream `rbx_replication` decodes. The port target is
//! Boblox's `client/src/raknet/` (the reference's C++ sources, vendored
//! here as `Boblox-main.zip` for reading) — a 1:1 port, the same wire
//! bytes for the same inputs.
//!
//! Wire facts carried over from the reference's `raknet.h`, all
//! **[GAME]** — observed against live servers:
//!
//! * the game connection lives on the Udmux port, and every datagram is
//!   RUPP-wrapped;
//! * the offline handshake uses Roblox's own packet ids (`0x7b` Req1,
//!   `0x7e` Reply1, `0x78` Req2, `0x7d` Reply2), not stock RakNet's;
//! * everything after Reply1 is encrypted by the native SessionCrypto —
//!   X25519, a BLAKE2b-512 KDF, ChaCha20-Poly1305 for the handshake and
//!   AES-256-GCM for datagrams — which is this crate's [`crypto`]
//!   module;
//! * the join script's RUPP token (TLV1 subtype 1) is what opens the
//!   session: Reply2's own `01 11 02 <16B>` TLV is not a token the
//!   server accepts from the client.
//!
//! What is in so far: the session crypto (keypairs, both key
//! derivations, the datagram AEAD with its counter reconstruction, and
//! the handshake AEAD with its in-buffer nonce layout), the RUPP
//! datagram wrapper (both client header shapes, the server's short
//! headers, and the TLV walk), and the offline handshake's byte layer —
//! Req1's padded probe, Reply1's GUID and MTU, Req2's sealed join
//! material, and Reply2's verified tail with the data peer key
//! reconstructed from its two halves. Not yet ported: the socket and
//! handshake state machine, the RakNet reliable/split/ACK data layer,
//! and the join flow itself.

#![forbid(unsafe_code)]

pub mod crypto;
pub mod offline;
pub mod rupp;
