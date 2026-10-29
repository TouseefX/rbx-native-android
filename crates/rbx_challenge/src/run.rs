//! Program execution: load, call, answer.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;

use mlua::ffi;
use mlua::Lua;

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

    // The challenge wire is Luau bytecode version 13; mlua-sys 0.12's
    // chunk sniffing caps "bytecode" at version 11 and would hand the
    // program to the source compiler. The VM itself loads versions
    // [3..14], and the reference's runner calls luau_load directly --
    // which is what this does too.
    unsafe {
        let st = lua.state();
        let r = ffi::luau_load(
            st,
            c"=challenge".as_ptr(),
            code.as_ptr() as *const c_char,
            code.len(),
            0,
        );
        if r != 0 {
            return Err(format!("load failed: {}", err_on_stack(st)));
        }

        // Two number args, one return -- the reference's call shape.
        ffi::lua_pushnumber(st, f64::from(arg1));
        ffi::lua_pushnumber(st, f64::from(arg2));
        if ffi::lua_pcall(st, 2, 1, 0) != 0 {
            return Err(format!("runtime error: {}", err_on_stack(st)));
        }

        // lua_tonumber semantics: numbers through, anything else 0. The
        // datasets always return a number, but a hostile program must
        // not turn a nil return into a solver error.
        let raw = if ffi::lua_type(st, -1) == ffi::LUA_TNUMBER {
            ffi::lua_tonumberx(st, -1, ptr::null_mut())
        } else {
            0.0
        };
        ffi::lua_settop(st, 0);
        Ok(d2ans(raw))
    }
}

/// The error message the VM left on top of the stack.
unsafe fn err_on_stack(st: *mut ffi::lua_State) -> String {
    let msg = ffi::lua_tolstring(st, -1, ptr::null_mut());
    if msg.is_null() {
        return "(?)".to_string();
    }
    CStr::from_ptr(msg).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Value;

    #[test]
    fn the_answer_truncates_like_the_native() {
        assert_eq!(d2ans(0.0), 0);
        assert_eq!(d2ans(-1.0), 0xFFFF_FFFF);
        assert_eq!(d2ans(4294967296.0 + 5.0), 5);
        assert_eq!(d2ans(f64::NAN), 0x8000_0000);
        assert_eq!(d2ans(1e300), 0x8000_0000);
    }

    #[test]
    fn a_tiny_source_program_solves_through_the_sandbox() {
        // run() takes bytecode only now, so this pins the sandbox and
        // the argument order through mlua's own source path; the vector
        // tests exercise run()'s bytecode path with the real wire.
        let _ = Lua::set_fflag("LuauCallFeedback", true);
        let lua = Lua::new();
        sandbox::install(&lua, "job").unwrap();
        let f = lua
            .load(b"local a, b = ... return a * 2 + b")
            .set_name("=tiny")
            .into_function()
            .unwrap();
        let ret: Value = f.call((f64::from(3), f64::from(5))).unwrap();
        assert_eq!(ret, Value::Number(11.0));
    }

    #[test]
    fn a_bad_program_reports_its_stage() {
        // Source where bytecode belongs: the first byte reads as
        // bytecode version 'l', far outside [3..14].
        let err = run(b"local = ", 1, 2, "job").unwrap_err();
        assert!(err.starts_with("load failed"), "{err}");
    }
}
