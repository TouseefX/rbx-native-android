//! The RUPP datagram wrapper — the Roblox UDP framing around everything
//! this transport sends and receives, ported from Boblox's `rupp.h` and
//! the `ruppView`/`buildClientHeader` pair in `raknet.cpp` **[GAME]**.
//!
//! ```text
//! [01][00][u16 BE total] TLV1 [TLV2] <payload>
//! ```
//!
//! `total` counts the 4-byte prefix itself, so the client header is
//! 4 + 19 + 8 = 31 bytes on the game path. TLV1 is the token
//! (`01 11 <sub> <16B>`: type 1, len 17, subtype 1 = the join script's
//! NetStackTokenValue, 2 = the session's own token); TLV2 is the RCC
//! this session belongs to (`02 06 <IPv4 network order> <port BE16>`) —
//! the server the join log names, which the RCC validates even on Req1.
//! The server wraps its replies the same way, with short headers (total
//! 4 or 23), and datagrams that do not parse as RUPP are dropped.

/// The client header length with the RCC TLV — the native game path.
pub const HEADER_LEN: u16 = 31;

/// The header length without the RCC TLV.
pub const HEADER_LEN_NO_RCC: u16 = 23;

/// One RUPP header's worth of state: the token we present and the RCC we
/// belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// TLV1's 16-byte token value.
    pub token: [u8; 16],
    /// TLV1's subtype: 1 = the join token, 2 = the session's own token.
    pub subtype: u8,
    /// TLV2: the RCC's IPv4 (network order) and port (host order).
    pub rcc: Option<([u8; 4], u16)>,
}

/// Wrap one payload: the 4-byte prefix, the token TLV, the RCC TLV when
/// the header carries one, then the payload verbatim.
pub fn wrap(header: &Header, payload: &[u8]) -> Vec<u8> {
    let total = if header.rcc.is_some() {
        HEADER_LEN
    } else {
        HEADER_LEN_NO_RCC
    };
    let mut out = Vec::with_capacity(total as usize + payload.len());
    out.extend_from_slice(&[0x01, 0x00]);
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(&[0x01, 0x11, header.subtype]);
    out.extend_from_slice(&header.token);
    if let Some((ip, port)) = header.rcc {
        out.extend_from_slice(&[0x02, 0x06]);
        out.extend_from_slice(&ip);
        out.extend_from_slice(&port.to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

/// One parsed RUPP datagram: everything past the header, plus whatever
/// the TLV area said. Accepts the short server headers (total 4 or 23)
/// as well as the 31-byte client header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View<'a> {
    /// The payload — the bytes from `total` to the end of the datagram.
    pub payload: &'a [u8],
    /// The whole TLV area, from byte 4 to `total`.
    pub tlvs: &'a [u8],
    /// The header's `total` field.
    pub total: u16,
    /// The token TLV, when the header carries a well-formed one.
    pub token: Option<([u8; 16], u8)>,
}

/// Parse one received datagram. `None` when it is not a RUPP datagram —
/// the caller drops it, the way the reference does.
pub fn view(pkt: &[u8]) -> Option<View<'_>> {
    if pkt.len() < 4 || pkt[0] != 0x01 || pkt[1] != 0x00 {
        return None;
    }
    let total = u16::from_be_bytes([pkt[2], pkt[3]]);
    if usize::from(total) < 4 || usize::from(total) > pkt.len() {
        return None;
    }
    let tlvs = &pkt[4..usize::from(total)];
    let payload = &pkt[usize::from(total)..];
    let mut token = None;
    let mut p = 0;
    while p + 2 <= tlvs.len() {
        let kind = tlvs[p];
        let len = usize::from(tlvs[p + 1]);
        if p + 2 + len > tlvs.len() {
            break;
        }
        if kind == 1 && len == 17 && (tlvs[p + 2] == 1 || tlvs[p + 2] == 2) {
            let mut value = [0u8; 16];
            value.copy_from_slice(&tlvs[p + 3..p + 19]);
            token = Some((value, tlvs[p + 2]));
        }
        p += 2 + len;
    }
    Some(View {
        payload,
        tlvs,
        total,
        token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from the reference's own header comment: token
    /// TLV plus the RCC TLV, exactly the 31 bytes the native sends.
    #[test]
    fn game_path_header_is_31_bytes() {
        let header = Header {
            token: [0xab; 16],
            subtype: 1,
            rcc: Some(([10, 37, 8, 91], 61269)),
        };
        let wrapped = wrap(&header, b"payload");
        assert_eq!(wrapped.len(), 31 + b"payload".len());
        assert_eq!(wrapped[..4], [0x01, 0x00, 0x00, 0x1f]);
        assert_eq!(wrapped[4..7], [0x01, 0x11, 0x01]);
        assert_eq!(wrapped[7..23], [0xab; 16]);
        assert_eq!(wrapped[23..25], [0x02, 0x06]);
        assert_eq!(wrapped[25..29], [10, 37, 8, 91]);
        assert_eq!(wrapped[29..31], [0xef, 0x55]); // 61269, big-endian
        assert_eq!(wrapped[31..], *b"payload");
    }

    #[test]
    fn header_without_rcc_is_23_bytes() {
        let header = Header {
            token: [1u8; 16],
            subtype: 2,
            rcc: None,
        };
        let wrapped = wrap(&header, b"x");
        assert_eq!(wrapped.len(), 24);
        assert_eq!(wrapped[..4], [0x01, 0x00, 0x00, 0x17]);
        assert_eq!(wrapped[23..], *b"x");
    }

    #[test]
    fn view_roundtrips_both_subtypes() {
        for subtype in [1u8, 2] {
            let header = Header {
                token: [0x5a; 16],
                subtype,
                rcc: Some(([10, 190, 0, 5], 50105)),
            };
            let wrapped = wrap(&header, b"the inner datagram");
            let parsed = view(&wrapped).unwrap();
            assert_eq!(parsed.total, 31);
            assert_eq!(parsed.payload[..], *b"the inner datagram");
            assert_eq!(parsed.token, Some(([0x5a; 16], subtype)));
            assert_eq!(parsed.tlvs[..3], [0x01, 0x11, subtype]);
        }
    }

    /// The server's short headers: total 4 (prefix only) and 23 (a token
    /// TLV, no RCC).
    #[test]
    fn view_accepts_short_server_headers() {
        let bare = [0x01u8, 0x00, 0x00, 0x04, 0x7e, 0x00, 0xff];
        let parsed = view(&bare).unwrap();
        assert_eq!(parsed.total, 4);
        assert_eq!(parsed.payload[..], [0x7e, 0x00, 0xff]);
        assert_eq!(parsed.token, None);

        let mut with_token = vec![0x01u8, 0x00, 0x00, 0x17, 0x01, 0x11, 0x02];
        with_token.extend_from_slice(&[0x99; 16]);
        with_token.extend_from_slice(b"server tail");
        let parsed = view(&with_token).unwrap();
        assert_eq!(parsed.total, 23);
        assert_eq!(parsed.payload[..], *b"server tail");
        assert_eq!(parsed.token, Some(([0x99; 16], 2)));
    }

    #[test]
    fn non_rupp_and_truncated_datagrams_are_rejected() {
        assert!(view(&[]).is_none());
        assert!(view(&[0x02, 0x00, 0x00, 0x04]).is_none());
        assert!(view(&[0x01, 0x01, 0x00, 0x04]).is_none());
        // A total beyond the datagram's length is not a header we can trust.
        assert!(view(&[0x01, 0x00, 0x00, 0x20, 0x7e]).is_none());
    }

    /// A TLV area that stops mid-record still parses — the token stays
    /// unset and the payload is unaffected, matching the reference's
    /// break-out of the TLV walk.
    #[test]
    fn view_tolerates_a_malformed_tlv_area() {
        // A TLV record that claims 17 bytes but has none of them.
        let mut pkt = vec![0x01u8, 0x00, 0x00, 0x06, 0x01, 0x11];
        pkt.extend_from_slice(b"inner");
        let parsed = view(&pkt).unwrap();
        assert_eq!(parsed.total, 6);
        assert_eq!(parsed.payload[..], *b"inner");
        assert_eq!(parsed.token, None);
    }
}
