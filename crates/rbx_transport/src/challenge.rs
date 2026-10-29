//! The 0x9B anti-cheat exchange: the server's challenge and our answer.
//!
//! The RCC sends the challenge as one application message when it wants
//! one — and only when it wants one. Team Create servers usually do not;
//! roughly one join in four on the reference's capture setup never saw
//! one at all. So the policy here is **reactive, never a wait**:
//!
//! - the join burst goes out with no challenge of ours in it, and the
//!   session runs whether or not the RCC ever challenges;
//! - when a server-form 0x9B lands, it is solved and answered at once —
//!   [9b][u2][answer] as the next ordered message — unless the join
//!   burst is still mid-flight, in which case the answer waits for the
//!   burst's end: the answer is itself ordered, and answering early
//!   pushes the burst's later messages past the ordering indexes the
//!   native leaves them at (measured: the server accepted such an
//!   answer but the 0x97 came without its record batches and the world
//!   stayed empty);
//! - a challenge that cannot be solved is **not** answered — a missing
//!   answer only leaves the join gated, a wrong one gets the session
//!   kicked;
//! - a challenge that arrives mid-session keeps getting answered the
//!   same way.
//!
//! The solve itself — RSB1 xor + xxhash32 + zstd decode, opcode
//! standardization, then a Luau VM run under the production sandbox —
//! is not this module's business. [`crate::client::Client`] takes the
//! solver as a closure, and answers with whatever it returns.

/// The server's challenge form: `[9b][u1 u32][u2 u32][len u32][blob]`.
///
/// True when `p` is the complete server form — the 13-byte header plus
/// a blob that fits. The 9-byte client form (our own answer) is not a
/// challenge.
pub fn is_challenge(p: &[u8]) -> bool {
    if p.len() < 13 || p[0] != 0x9B {
        return false;
    }
    let len = u32::from_le_bytes([p[9], p[10], p[11], p[12]]) as usize;
    len <= p.len() - 13
}

/// A parsed server-form challenge.
#[derive(Clone, Copy, Debug)]
pub struct Challenge<'a> {
    pub u1: u32,
    pub u2: u32,
    /// The blob, from byte 13 to the end of the message.
    pub blob: &'a [u8],
}

/// Parse the server form. `None` when `p` is not a complete challenge.
pub fn extract(p: &[u8]) -> Option<Challenge<'_>> {
    if !is_challenge(p) {
        return None;
    }
    Some(Challenge {
        u1: u32::from_le_bytes([p[1], p[2], p[3], p[4]]),
        u2: u32::from_le_bytes([p[5], p[6], p[7], p[8]]),
        blob: &p[13..],
    })
}

/// The 9-byte answer: `[9b][u2 u32][answer u32]`.
///
/// `u2` is the challenge's own second field, echoed; `answer` is
/// little-endian like the reference builds it.
pub fn build_response(u2: u32, answer: u32) -> [u8; 9] {
    let mut r = [0u8; 9];
    r[0] = 0x9B;
    r[1..5].copy_from_slice(&u2.to_le_bytes());
    r[5..9].copy_from_slice(&answer.to_le_bytes());
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_form(u1: u32, u2: u32, blob: &[u8]) -> Vec<u8> {
        let mut p = Vec::with_capacity(13 + blob.len());
        p.push(0x9B);
        p.extend_from_slice(&u1.to_le_bytes());
        p.extend_from_slice(&u2.to_le_bytes());
        p.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        p.extend_from_slice(blob);
        p
    }

    #[test]
    fn the_server_form_is_a_challenge() {
        let p = server_form(0x11, 0x22, &[0xAB; 8]);
        assert!(is_challenge(&p));
        let c = extract(&p).unwrap();
        assert_eq!(c.u1, 0x11);
        assert_eq!(c.u2, 0x22);
        assert_eq!(c.blob, [0xAB; 8]);
    }

    #[test]
    fn short_and_client_forms_are_not() {
        // The 9-byte client form: our answer, not a challenge.
        assert!(!is_challenge(&build_response(1, 2)));
        // Header without the blob it promises.
        let mut p = server_form(1, 2, &[0xAB; 8]);
        p.truncate(13);
        assert!(!is_challenge(&p));
        // Wrong id, right shape.
        let mut p = server_form(1, 2, &[0xAB; 8]);
        p[0] = 0x9C;
        assert!(!is_challenge(&p));
    }

    #[test]
    fn the_answer_layout() {
        let r = build_response(0x0A0B0C0D, 0x0BAD_F00D);
        assert_eq!(r, [0x9B, 0x0D, 0x0C, 0x0B, 0x0A, 0x0D, 0xF0, 0xAD, 0x0B]);
    }
}
