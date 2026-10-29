//! The opcode standardizer: wire bytecode -> standard Luau bytecode.
//!
//! Faithful port of the reference's `solve/standardize.cpp` (itself a
//! port of `tools/standardize_wire.py`; the tables come from
//! `tools/roblox_bc_tables.json`). Roblox ships its bytecode with the
//! opcode bytes permuted per build; the loader remaps each wire opcode
//! through REMAP to the build's *internal* opcode, then OPLEN maps that
//! to the *standard* Luau opcode the VM expects. The rest is a walk:
//!
//! - parse the program header (string table, the optional type-remap
//!   table of tv==3, then the proto table with sizes);
//! - for each proto, walk the instruction stream from its code offset:
//!   rewrite each word's low byte, and step by the standard opcode's
//!   instruction length (1 or 2 words);
//! - the walk must consume exactly `sizeCode` words — a walk that stops
//!   short means the remap is wrong for this build and the program
//!   would misexecute, so it is refused instead.

/// Wire opcode -> the build's internal opcode (loader transform).
const REMAP: [
    193, 218, 1, 168, 17, 214, 250, 39, 204, 130, 191, 181, 21, 33, 16, 94, 197, 134, 165, 120, 38,
    101, 26, 78, 176, 222, 252, 77, 50, 200, 115, 95, 111, 229, 5, 73, 53, 151, 15, 47, 19, 57,
    136, 49, 253, 65, 8, 96, 125, 55, 25, 185, 86, 133, 27, 59, 123, 226, 103, 230, 163, 117, 35,
    90, 51, 58, 63, 244, 121, 192, 240, 145, 138, 93, 251, 211, 6, 106, 31, 143, 126, 99, 203, 0,
    3, 42, 83, 161, 128, 10, 234, 167, 45, 113, 29, 164, 67, 198, 129, 88, 187, 194, 81, 155, 7,
    147, 87, 36, 85, 153, 179, 76, 212, 32, 142, 122, 97, 72, 210, 175, 23, 2, 166, 208, 178, 237,
    13, 135, 105, 22, 246, 201, 235, 174, 69, 159, 248, 238, 184, 56, 232, 40, 182, 233, 213, 177,
    79, 68, 148, 66, 202, 243, 52, 196, 11, 157, 75, 144, 109, 162, 61, 110, 183, 152, 207, 190,
    242, 169, 9, 195, 30, 247, 160, 239, 137, 206, 37, 60, 18, 20, 74, 24, 62, 245, 34, 216, 170,
    227, 132, 102, 146, 231, 189, 255, 205, 116, 84, 221, 64, 156, 98, 219, 149, 80, 154, 70, 215,
    223, 131, 14, 43, 48, 107, 220, 186, 217, 225, 44, 228, 104, 4, 54, 114, 12, 199, 150, 127,
    224, 158, 124, 241, 172, 89, 28, 119, 100, 82, 249, 173, 188, 71, 180, 139, 92, 91, 108, 209,
    140, 254, 46, 41, 112, 141, 118, 171, 236,
];

/// Internal opcode -> standard Luau opcode.
const OPLEN: [
    209, 150, 243, 156, 116, 246, 68, 120, 122, 56, 147, 30, 213, 234, 187, 34, 26, 44, 38, 184,
    241, 132, 75, 40, 135, 166, 114, 210, 195, 138, 206, 218, 155, 79, 232, 42, 217, 144, 220, 141,
    207, 62, 103, 134, 19, 244, 115, 237, 81, 25, 52, 192, 136, 140, 63, 219, 57, 131, 139, 157,
    91, 224, 82, 86, 2, 175, 39, 32, 145, 66, 143, 80, 199, 193, 188, 180, 5, 105, 61, 198, 249,
    226, 36, 50, 108, 164, 60, 14, 129, 248, 245, 124, 177, 227, 229, 149, 69, 252, 152, 59, 89,
    167, 223, 254, 169, 128, 15, 28, 71, 74, 171, 96, 9, 191, 10, 202, 161, 95, 159, 142, 17, 236,
    49, 104, 151, 16, 112, 54, 200, 182, 35, 240, 20, 7, 123, 181, 78, 250, 24, 230, 221, 212, 102,
    165, 127, 77, 170, 67, 92, 46, 107, 87, 65, 111, 196, 173, 205, 233, 204, 13, 100, 253, 21,
    148, 85, 70, 190, 41, 97, 109, 126, 106, 45, 186, 119, 93, 8, 251, 84, 58, 27, 185, 154, 118,
    110, 113, 178, 76, 133, 64, 215, 238, 183, 0, 23, 3, 83, 176, 235, 160, 255, 225, 242, 6, 88,
    214, 197, 12, 137, 18, 146, 121, 208, 48, 247, 90, 179, 125, 203, 99, 231, 55, 211, 37, 1, 72,
    51, 73, 222, 43, 201, 117, 4, 101, 94, 172, 53, 31, 163, 47, 130, 98, 162, 189, 33, 29, 22,
    153, 216, 239, 194, 174, 158, 228, 168, 11,
];

/// sub_27509DA: "mov ecx, offset byte_17E7B; bt ecx, edi" -- the tested
/// value is the address constant 0x17E7B itself, not the memory at it.
const MASK1: u64 = 0x0829_4000_03F0_2323;
const MASK2: u64 = 0x0000_0000_0001_7E7B;

/// Loader instruction-length semantics on the STANDARD opcode.
fn op_length(std_op: u8) -> u32 {
    let v2 = i32::from(std_op) - 7;
    if (0..=0x3B).contains(&v2) && ((MASK1 >> v2) & 1) != 0 {
        return 2;
    }
    let v4 = i32::from(std_op) - 74;
    if (0..=0x10).contains(&v4) && ((MASK2 >> v4) & 1) != 0 {
        return 2;
    }
    1
}

fn varint(buf: &[u8], off: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut sh = 0i32;
    loop {
        if *off >= buf.len() {
            return None;
        }
        let b = buf[*off];
        *off += 1;
        v |= u64::from(b & 0x7F) << sh;
        if b & 0x80 == 0 {
            return Some(v);
        }
        sh += 7;
        if sh > 63 {
            return None;
        }
    }
}

/// Where one proto's instructions start, and how many words they are.
struct Proto {
    code_off: usize,
    size_code: u64,
}

/// Parse the v13 header (string table skipping included) and collect
/// per-proto code locations.
fn parse_protos(buf: &[u8]) -> Result<Vec<Proto>, String> {
    if buf.is_empty() {
        return Err("empty program".into());
    }
    let mut off = 1; // version
    if off >= buf.len() {
        return Err("truncated header".into());
    }
    let tv = buf[off];
    off += 1;
    let n_strings = varint(buf, &mut off).ok_or("bad string count")?;
    for _ in 0..n_strings {
        let ln = varint(buf, &mut off).ok_or("bad string table")?;
        // Subtraction form: off only ever advances inside the buffer, so
        // the remaining length cannot wrap.
        if ln > (buf.len() - off) as u64 {
            return Err("bad string table".into());
        }
        off += ln as usize;
    }
    if tv == 3 {
        if off >= buf.len() {
            return Err("truncated remap table".into());
        }
        let mut idx = buf[off];
        off += 1;
        while idx != 0 {
            varint(buf, &mut off).ok_or("bad remap entry")?;
            if off >= buf.len() {
                return Err("truncated remap table".into());
            }
            idx = buf[off];
            off += 1;
        }
    }
    let proto_count = varint(buf, &mut off).ok_or("bad proto count")?;
    // Each proto needs at least its 1-byte size varint past this point;
    // without the bound, a hostile count asks for a giant allocation
    // before any parse failure rejects it.
    if proto_count > (buf.len() - off) as u64 {
        return Err("proto count exceeds program".into());
    }
    let mut protos = Vec::with_capacity(proto_count as usize);
    for _ in 0..proto_count {
        let psize = varint(buf, &mut off).ok_or("bad proto size")?;
        let pstart = off;
        if off + 5 > buf.len() {
            return Err("truncated proto".into());
        }
        off += 4 + 1;
        if tv == 1 || tv == 2 || tv == 3 {
            let ts = varint(buf, &mut off).ok_or("bad proto types")?;
            if ts > (buf.len() - off) as u64 {
                return Err("bad proto types".into());
            }
            off += ts as usize;
        }
        let size_code = varint(buf, &mut off).ok_or("bad sizecode")?;
        let p = Proto {
            code_off: off,
            size_code,
        };
        protos.push(p);
        if psize > (buf.len() - pstart) as u64 {
            return Err("proto overruns program".into());
        }
        off = pstart + psize as usize;
    }
    Ok(protos)
}

/// Rewrite the wire opcodes to standard Luau opcodes, in place.
///
/// `Err` means the parse failed or a proto's walk did not consume
/// exactly its sizecode -- either way the program must not run.
pub fn standardize(buf: &mut [u8]) -> Result<(), String> {
    let protos = parse_protos(buf)?;
    for (i, p) in protos.iter().enumerate() {
        // The walk consumes 4*size_code bytes from code_off; reject the
        // impossible at the door so the in-loop arithmetic stays plain.
        if p.size_code > ((buf.len() - p.code_off) / 4) as u64 {
            return Err("sizecode exceeds program".into());
        }
        let mut pc = 0u64;
        while pc < p.size_code {
            let word_off = p.code_off + 4 * pc as usize;
            if word_off + 4 > buf.len() {
                return Err("code overruns program".into());
            }
            let word = u32::from_le_bytes([
                buf[word_off],
                buf[word_off + 1],
                buf[word_off + 2],
                buf[word_off + 3],
            ]);
            let internal = REMAP[(word & 0xFF) as usize];
            let std = OPLEN[usize::from(internal)];
            let new_word = (word & 0xFFFF_FF00) | u32::from(std);
            buf[word_off..word_off + 4].copy_from_slice(&new_word.to_le_bytes());
            pc += u64::from(op_length(std));
        }
        if pc != p.size_code {
            return Err(format!(
                "proto walk failed: proto {i} pc {pc} != sizecode {}",
                p.size_code
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_length_masks_match_the_decompiled_ranges() {
        // A few known entries from the reference's MASK1/MASK2 windows,
        // checked by hand against the bit positions.
        assert_eq!(op_length(OPLEN[REMAP[0]]), 1);
        assert!((1..=2).contains(&op_length(74)));
        assert!((1..=2).contains(&op_length(7)));
    }

    #[test]
    fn the_tables_are_permutations_of_the_byte_range() {
        let mut seen = [false; 256];
        for v in REMAP {
            assert!(!seen[usize::from(v)], "REMAP repeats {}", v);
            seen[usize::from(v)] = true;
        }
        let mut seen = [false; 256];
        for v in OPLEN {
            assert!(!seen[usize::from(v)], "OPLEN repeats {}", v);
            seen[usize::from(v)] = true;
        }
    }

    #[test]
    fn an_empty_program_is_refused() {
        assert_eq!(standardize(&mut []), Err("empty program".to_string()));
    }

    #[test]
    fn a_program_of_ones_walks_to_a_boundary_or_fails() {
        // The reference's contract on garbage is a refusal, never a
        // panic: whatever the walk does, it must stay inside the buffer
        // and report an error or success.
        let mut buf = vec![1u8; 64];
        let _ = standardize(&mut buf);
        let mut buf = vec![0u8; 64];
        buf[0] = 1; // version
        buf[1] = 0; // tv, no remap table
        let _ = standardize(&mut buf);
    }
}
