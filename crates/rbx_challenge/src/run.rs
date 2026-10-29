//! Program execution: load, call, answer.

use mlua::{Lua, Value};

use crate::sandbox;

/// Native truncation of the returned double to the 32-bit answer wraps
/// modulo 2^32 -- the same observed behavior as the Random seed cast:
/// `(unsigned int)(int)v` on a value in [-2^53, 2^53] keeps the low 32
/// bits. Outside that range the native produces INT_MIN.
fn d2ans(v: f64) -> u32 {
    if !(-9007199254740992.0..=9007199254740992.0).contains(&v) {
        return 0x8000_0000;
    }
    (v as i64) as u32
}

/// Execute a standardized-Luau bytecode challenge program.
///
/// `arg1`/`arg2` are the program's two number arguments -- the challenge
/// fields, in the order the native's answers were reconciled against
/// (the wire's u2 first). The returned double truncates to the answer.
pub fn run(code: &[u8], arg1: u32, arg2: u32, job: &str) -> Result<u32, String> {
    // Roblox production bytecode is compiled with LuauCallFeedback on
    // and carries LOP_CALLFB instructions; the upstream VM defaults the
    // flag off and miscomputes such bytecode. It is a global setting,
    // flipped before the state exists.
    let _ = Lua::set_fflag("LuauCallFeedback", true);

    let lua = Lua::new();
    sandbox::install(&lua, job).map_err(|e| format!("sandbox: {e}"))?;

    let f = lua
        .load(code)
        .set_name("=challenge")
        .into_function()
        .map_err(|e| format!("load failed: {e}"))?;

    // lua_tonumber semantics: numbers through, anything else 0. The
    // datasets always return a number, but a hostile program must not
    // turn a nil return into a solver error.
    let ret: Value = f
        .call((f64::from(arg1), f64::from(arg2)))
        .map_err(|e| format!("runtime error: {e}"))?;
    let raw = match ret {
        Value::Number(n) => n,
        Value::Integer(i) => i as f64,
        _ => 0.0,
    };
    Ok(d2ans(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_truncates_like_the_native() {
        assert_eq!(d2ans(0.0), 0);
        assert_eq!(d2ans(-1.0), 0xFFFF_FFFF);
        assert_eq!(d2ans(4294967296.0 + 5.0), 5);
        assert_eq!(d2ans(f64::NAN), 0x8000_0000);
        assert_eq!(d2ans(1e300), 0x8000_0000);
    }

    #[test]
    fn a_tiny_source_program_solves() {
        // Not bytecode -- mlua compiles this source the same way the
        // Luau CLI would; what it pins is the sandbox being callable
        // end to end with the argument order.
        let code = b"local a, b = ... return a * 2 + b";
        assert_eq!(run(code, 3, 5, "job").unwrap(), 11);
    }

    #[test]
    fn a_bad_program_reports_its_stage() {
        let err = run(b"local = ", 1, 2, "job").unwrap_err();
        assert!(err.starts_with("load failed"), "{err}");
    }
}
