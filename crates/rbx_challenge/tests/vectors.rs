//! The golden vectors: every captured native challenge, through every
//! stage, against the frozen goldens — and the answers the native
//! client itself sent.
//!
//! The fixtures are the reference's `tests/vectors/` (per-stage goldens)
//! plus the tracked captures they were derived from. A failure here
//! pinpoints WHICH stage drifted, independently of the sandbox probes.

use rbx_challenge::{blob, run, standardize};

struct Vector {
    name: &'static str,
    u1: u32,
    u2: u32,
    /// The answer the native sent for this challenge, when recorded.
    answer: Option<u32>,
    job: Option<&'static str>,
}

const VECTORS: &[Vector] = &[
    Vector {
        name: "comb1",
        u1: 0x8059_da68,
        u2: 0xcb04_7de0,
        answer: Some(2603781223),
        job: Some("472bd6de-39e8-44f7-9fe9-de91abce9dc2"),
    },
    Vector {
        name: "comb2",
        u1: 0xd8fb_7f57,
        u2: 0x149a_567f,
        answer: Some(4181992127),
        job: Some("472bd6de-39e8-44f7-9fe9-de91abce9dc2"),
    },
    Vector {
        name: "comb3",
        u1: 759_272_077,
        u2: 2_258_879_425,
        answer: Some(1697138922),
        job: Some("472bd6de-39e8-44f7-9fe9-de91abce9dc2"),
    },
    Vector {
        name: "full",
        u1: 0x8029_dd95,
        u2: 0x2955_3a8e,
        answer: Some(2193615038),
        job: Some("e62f5d1f-24fc-464c-8d20-71431adc95b6"),
    },
    Vector {
        name: "oracle",
        u1: 3_558_163_496,
        u2: 3_131_075_234,
        answer: None,
        job: None,
    },
    Vector {
        name: "rounds_r1",
        u1: 975_564_607,
        u2: 223_372_620,
        answer: Some(1902623592),
        job: Some("23c2e471-7b39-44bd-82b8-cd25661f9355"),
    },
];

/// The goldens live on disk next to this test; include_bytes! needs a
/// literal path, so the per-case files are read at runtime instead.
fn vector_file(name: &str, kind: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("vectors")
        .join(format!("{name}.{kind}.bin"));
    match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => panic!("cannot read {}: {e}", path.display()),
    }
}

#[test]
fn golden_vectors_extract_decode_standardize_solve() {
    // A path mistake must not read as "all passed".
    assert!(VECTORS.len() >= 6);
    for v in VECTORS {
        let msg = vector_file(v.name, "msg");
        let wire_gold = vector_file(v.name, "wire");
        let std_gold = vector_file(v.name, "standard");

        // Stage 1: the header.
        let ch = rbx_transport::challenge::extract(&msg)
            .unwrap_or_else(|| panic!("{}: extract failed", v.name));
        assert_eq!(ch.u1, v.u1, "{}: u1", v.name);
        assert_eq!(ch.u2, v.u2, "{}: u2", v.name);

        // Stage 2: RSB1 xor + xxhash32 + zstd.
        let mut wire = blob::decode(ch.blob).unwrap_or_else(|e| panic!("{}: decode: {e}", v.name));
        assert_eq!(wire.as_slice(), &wire_gold[..], "{}: wire", v.name);

        // Stage 3: opcode standardization, in place.
        standardize::standardize(&mut wire)
            .unwrap_or_else(|e| panic!("{}: standardize: {e}", v.name));
        assert_eq!(wire.as_slice(), &std_gold[..], "{}: standard", v.name);

        // Stage 4: the full solve vs the answer the native sent.
        if let (Some(answer), Some(job)) = (v.answer, v.job) {
            let got = run::run(&wire, ch.u2, ch.u1, job)
                .unwrap_or_else(|e| panic!("{}: run: {e}", v.name));
            assert_eq!(got, answer, "{}: answer", v.name);
        }
    }
}

#[test]
fn the_top_level_solve_agrees_with_the_stages() {
    for v in VECTORS {
        if let (Some(answer), Some(job)) = (v.answer, v.job) {
            let msg = vector_file(v.name, "msg");
            let got = rbx_challenge::solve(&msg, job)
                .unwrap_or_else(|e| panic!("{}: solve: {e}", v.name));
            assert_eq!(got, answer, "{}: answer", v.name);
        }
    }
}
