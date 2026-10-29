//! The production challenge sandbox: six modules over the standard
//! Luau libraries, in the order the reference installs them.
//!
//! The profile is frozen and byte-exact against the native client --
//! the regression vectors answer only when these match:
//!
//! - `tostring(UserSettings())` yields the calibration string (byte-sum
//!   1924; any string with that sum behaves identically);
//! - `os.exit` is missing (its error is the +9001 probe term);
//! - `RunService:IsStudio()` is false (+1024);
//! - `newproxy(true)`'s `__namecall` dispatch works (+52);
//! - `Random` is Roblox's PCG, bit-for-bit;
//! - `game.JobId` is the session's job, `game:GetService` of a
//!   registered name returns the same table every call, of an unknown
//!   name a fresh empty table (identity-observable, deliberate).
//!
//! mlua loads the full standard library set for Luau (`ALL_SAFE` is
//! `ALL` under the feature), which is the reference's `luaL_openlibs`.

use std::ffi::c_int;

use mlua::ffi;
use mlua::{Lua, Table, UserData, UserDataMethods};

/// The production UserSettings calibration string: 19 a's and a Q,
/// byte-sum 1924.
const US_STRING: &str = "aaaaaaaaaaaaaaaaaaaQ";

// --- Random: Roblox's PCG, exact port of libroblox.so 2.739.691 -------

const K_MUL: u64 = 0x5851_F42D_4C95_7F2D;
const K_INC: u64 = 105;
const K_SEED_ADD: u64 = 0x399D_2694_6951_29DE;

/// PCG xsh-rr: the 32-bit output of a 64-bit state.
fn out32(s: u64) -> u32 {
    let x = ((s >> 27) ^ (s >> 45)) as u32;
    x.rotate_right((s >> 59) as u32)
}

pub(crate) struct RbxRandom {
    pub(crate) state: u64,
}

impl UserData for RbxRandom {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("NextInteger", |_, r, (lo, hi): (f64, f64)| {
            Ok(next_integer(r, d2i32(lo), d2i32(hi)))
        });
        methods.add_method_mut("NextNumber", |_, r, args: Option<(f64, f64)>| {
            let (lo, hi) = args.unwrap_or((0.0, 1.0));
            Ok(next_number(r, lo, hi))
        });
        methods.add_method("Clone", |lua, r, ()| {
            Ok(lua.create_userdata(RbxRandom { state: r.state })?)
        });
    }
}

/// The native's argument cast for min/max: doubles outside the i32
/// range go through the 64-bit conversion first.
fn d2i32(v: f64) -> i32 {
    if !(-2147483648.0..2147483648.0).contains(&v) {
        return (v as i64) as i32;
    }
    v as i32
}

/// Random.new's seed conversion: the caller guards |v| <= 2^53, so the
/// 64-bit truncation is exact.
fn seed_from_double(v: f64) -> u64 {
    v as i64 as u64
}

/// Exact port of the decompiled NextInteger (sub_2307554), wide-range
/// branch and all, statement for statement. The wide branch's first
/// term always vanishes in 32-bit arithmetic -- that is the
/// decompiler's own expression, kept so the port cannot drift from the
/// reference's validated form.
fn next_integer(r: &mut RbxRandom, a2: i32, mut a3: i32) -> i32 {
    let mut v3 = (i64::from(a2) - i64::from(a3)) as u64;
    if a2 <= a3 {
        v3 = (i64::from(a3) - i64::from(a2)) as u64;
    }
    if a2 < a3 {
        a3 = a2; // a3 becomes the low end
    }

    let v4 = K_MUL.wrapping_mul(r.state).wrapping_add(K_INC);
    let v5 = out32(r.state);

    if (v3 >> 1) > 0x7FFF_FFFE {
        let v7 = out32(v4);
        r.state = K_MUL.wrapping_mul(v4).wrapping_add(K_INC);
        let v8 = v3 + 1;
        if v8 != 0 {
            let res = (u64::from(v7.wrapping_mul(v8 as u32)) >> 32)
                + u64::from(v7) * (v8 >> 32)
                + u64::from(u32::from(a3))
                + ((u64::from(v5) * (v8 >> 32)) >> 32);
            return res as u32 as i32;
        }
        (u64::from(v5) | (u64::from(v7) << 32)) as u32 as i32
    } else {
        r.state = v4;
        u32::from(a3).wrapping_add(((u64::from(v5) * (v3 + 1)) >> 32) as u32) as i32
    }
}

/// Exact port of NextNumber: two consecutive outputs -> [0,1) -> scale.
fn next_number(r: &mut RbxRandom, lo: f64, hi: f64) -> f64 {
    let v8 = K_MUL.wrapping_mul(r.state).wrapping_add(K_INC);
    let a = out32(r.state);
    let b = out32(v8);
    r.state = K_MUL.wrapping_mul(v8).wrapping_add(K_INC);
    let frac = (u64::from(a) | (u64::from(b) << 32)) as f64 / 18446744073709551616.0;
    lo + frac * (hi - lo)
}

// --- newproxy: raw userdata with a fresh, settable metatable ---------

/// The one piece that must be raw C: `newproxy(true)` yields a userdata
/// whose metatable is a fresh empty TABLE the program can read and
/// write (the classic Luau inheritance mechanism, and the probe the
/// +52 calibration term measures). mlua's typed userdata cannot offer
/// that -- metatables there are per-Rust-type and full of internals --
/// so this is the Luau C API directly.
unsafe extern "C" fn newproxy_raw(st: *mut ffi::lua_State) -> c_int {
    unsafe {
        let with_meta = ffi::lua_toboolean(st, 1) != 0;
        ffi::lua_newuserdata(st, std::mem::size_of::<*const u8>());
        if with_meta {
            ffi::lua_newtable(st);
            ffi::lua_setmetatable(st, -2);
        }
    }
    1
}

// --- the installer ------------------------------------------------------

pub(crate) fn install(lua: &Lua, job: &str) -> Result<(), mlua::Error> {
    let globals = lua.globals();

    // 1. Random
    let random = lua.create_table()?;
    random.set(
        "new",
        lua.create_function(|lua, seed: Option<f64>| {
            let state = match seed {
                Some(v) if v.abs() <= 9007199254740992.0 => {
                    K_MUL.wrapping_mul(seed_from_double(v)).wrapping_add(K_SEED_ADD)
                }
                _ => K_SEED_ADD,
            };
            Ok(lua.create_userdata(RbxRandom { state })?)
        })?,
    )?;
    globals.set("Random", random)?;

    // 2. game + the service registry it dispatches through. GetService
    //    of a registered name returns the same table every call; of an
    //    unknown name, a fresh empty table per call.
    let services = lua.create_table()?;
    let run_service = lua.create_table()?;
    run_service.set("IsStudio", lua.create_function(|_, ()| Ok(false))?)?;
    services.set("RunService", run_service)?;
    let game = lua.create_table()?;
    game.set("JobId", job)?;
    let registry = services.clone();
    game.set(
        "GetService",
        lua.create_function(move |lua, (_self, name): (Table, String)| {
            let found: Option<Table> = registry.raw_get(name.as_str()).unwrap_or(None);
            match found {
                Some(svc) => Ok(svc),
                None => Ok(lua.create_table()?),
            }
        })?,
    )?;
    globals.set("game", game)?;

    // 3. RunService itself lives in the registry (installed above,
    //    before game, so GetService can find it).

    // 4. UserSettings: a fresh table per call whose metatable's
    //    __tostring is the calibration string.
    globals.set(
        "UserSettings",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            let mt = lua.create_table()?;
            mt.set(
                "__tostring",
                lua.create_function(|_, ()| Ok(US_STRING.to_string()))?,
            )?;
            t.set_metatable(Some(mt))?;
            Ok(t)
        })?,
    )?;

    // 5. os.exit: missing in production -- calling it errors.
    let os: Table = match globals.get("os") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            globals.set("os", t.clone())?;
            t
        }
    };
    os.set(
        "exit",
        lua.create_function(|_, ()| -> Result<(), mlua::Error> {
            Err(mlua::Error::runtime("os.exit unavailable"))
        })?,
    )?;

    // 6. newproxy, raw.
    unsafe {
        let st = lua.state();
        ffi::lua_pushcclosure(st, Some(newproxy_raw), c"newproxy".as_ptr(), 0);
        ffi::lua_setglobal(st, c"newproxy".as_ptr());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use mlua::Lua;

    use super::*;

    fn probe(source: &str) -> String {
        let lua = Lua::new();
        install(&lua, "job-1234").unwrap();
        let v: String = lua.load(source).call(()).unwrap();
        v
    }

    #[test]
    fn usersettings_string_is_the_calibration() {
        assert_eq!(probe("return tostring(UserSettings())"), "aaaaaaaaaaaaaaaaaaaQ");
        // And the byte-sum the challenge actually measures: 1924.
        let sum: u32 = probe("return tostring(UserSettings())")
            .bytes()
            .map(u32::from)
            .sum();
        assert_eq!(sum, 1924);
    }

    #[test]
    fn isstudio_is_false_and_jobid_passes_through() {
        assert_eq!(
            probe("return tostring(game:GetService('RunService'):IsStudio())"),
            "false"
        );
        assert_eq!(probe("return game.JobId"), "job-1234");
    }

    #[test]
    fn unknown_services_are_fresh_tables() {
        assert_eq!(
            probe("return tostring(game:GetService('Nope') == game:GetService('Nope'))"),
            "false"
        );
        assert_eq!(
            probe(
                "return tostring(game:GetService('RunService') == \
                 game:GetService('RunService'))"
            ),
            "true"
        );
    }

    #[test]
    fn os_exit_is_missing() {
        assert_eq!(
            probe("local ok = pcall(function() os.exit(0) end) return tostring(ok)"),
            "false"
        );
    }

    #[test]
    fn newproxy_true_has_a_settable_metatable() {
        // The +52 calibration probe itself: a namecall dispatched
        // through the program's own metatable on the proxy.
        assert_eq!(
            probe(
                "local p = newproxy(true) \
                 local mt = getmetatable(p) \
                 mt.__namecall = function() return 52 end \
                 return p:answer()"
            ),
            "52"
        );
        assert_eq!(
            probe(
                "local p = newproxy(true) \
                 local mt = getmetatable(p) \
                 mt.__index = {x = 7} return p.x"
            ),
            "7"
        );
        assert_eq!(
            probe("return tostring(newproxy(true) == newproxy(true))"),
            "false"
        );
    }

    #[test]
    fn random_is_deterministic_per_seed() {
        // Fixed draws (computed against the PCG port) so a regression
        // in the stream moves the number.
        assert_eq!(
            probe("local r = Random.new(42) return tostring(r:NextInteger(1, 100))"),
            "56"
        );
        assert_eq!(
            probe("local r = Random.new(0) return tostring(math.floor(r:NextNumber() * 1e9))"),
            "551455690"
        );
    }
}
