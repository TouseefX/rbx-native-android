//! The 0x9B solver: challenge message in, uint32 answer out, in-process.
//!
//! The pipeline, stage for stage the reference's `solve/solve.cpp`:
//!
//! 1. **extract** — `[9b][u1 u32][u2 u32][len u32][blob]`
//!    ([`rbx_transport::challenge`], shared with the transport);
//! 2. **decode** — RSB1 xor + xxhash32(seed 0x2A) verify + zstd
//!    ([`blob`]);
//! 3. **standardize** — the wire opcodes rewritten to standard Luau
//!    opcodes ([`standardize`]);
//! 4. **run** — the program executed on the vendored Luau VM under the
//!    production sandbox, called with `(u2, u1)` — the SECOND challenge
//!    field is the FIRST program argument, the order the native's
//!    answers were reconciled against ([`run`]).
//!
//! The sandbox is the frozen Challenge profile: six modules over the
//! standard libraries, byte-exact against the native client (the
//! regression vectors in `tests/` carry the answers the native itself
//! sent). Roblox's `Random` is the exact PCG port from libroblox.so.
//!
//! The VM is mlua's vendored Luau (`luau0-src`), the same upstream the
//! reference pins as a submodule; between the reference's pin and the
//! vendored one the only bytecode-visible changes are comment fixes,
//! so the standardized programs load identically.

pub mod blob;
pub mod run;
mod sandbox;
pub mod standardize;

use rbx_transport::client::Solver;

/// Solve a raw 0x9B challenge message for the given JobId.
///
/// `Err` carries the stage and its reason; a challenge that cannot be
/// solved is answered with nothing (the transport's contract: a missing
/// answer leaves the join gated, a wrong one gets the session kicked).
pub fn solve(msg: &[u8], job: &str) -> Result<u32, String> {
    let ch = rbx_transport::challenge::extract(msg).ok_or("not a 0x9B message")?;
    let mut wire = blob::decode(ch.blob)?;
    standardize::standardize(&mut wire)?;
    // Arg order: the wire's u2 field is the program's first argument.
    run::run(&wire, ch.u2, ch.u1, job)
}

/// The solver as [`rbx_transport::Client::set_solver`] wants it.
pub fn solver() -> Solver {
    Box::new(|msg, job| solve(msg, job).ok())
}
