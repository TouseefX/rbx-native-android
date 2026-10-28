//! Minimal dynamic binding to ViGEmClient (`ViGEmClient.dll`), loaded by
//! absolute path so a planted DLL in the working directory is never picked up.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use super::{Ds4Report, XusbReport};

type AllocFn = unsafe extern "C" fn() -> *mut c_void;
type ClientFn = unsafe extern "C" fn(*mut c_void) -> u32;
type DropFn = unsafe extern "C" fn(*mut c_void);
type TargetFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type X360UpdateFn = unsafe extern "C" fn(*mut c_void, *mut c_void, XusbReport) -> u32;
type Ds4UpdateFn = unsafe extern "C" fn(*mut c_void, *mut c_void, Ds4Report) -> u32;

pub(super) struct Api {
    pub(super) alloc: AllocFn,
    pub(super) connect: ClientFn,
    pub(super) disconnect: DropFn,
    pub(super) free: DropFn,
    pub(super) x360_alloc: AllocFn,
    pub(super) ds4_alloc: AllocFn,
    pub(super) target_add: TargetFn,
    pub(super) target_remove: TargetFn,
    pub(super) target_free: DropFn,
    pub(super) x360_update: X360UpdateFn,
    pub(super) ds4_update: Ds4UpdateFn,
}

/// Why the library could not be used.
pub(super) enum LoadError {
    Missing,
    MissingExport,
}

impl Api {
    pub(super) fn load(dll: &Path) -> Result<Api, LoadError> {
        let wide: Vec<u16> = OsStr::new(dll.as_os_str())
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(LoadError::Missing);
        }
        macro_rules! export {
            ($name:literal, $ty:ty) => {{
                // SAFETY: the name is NUL-terminated; the signature matches ViGEmClient's Client.h.
                match unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) } {
                    Some(f) => unsafe {
                        std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f)
                    },
                    None => return Err(LoadError::MissingExport),
                }
            }};
        }
        // The module stays loaded for the process's life: its functions are
        // copied out here.
        Ok(Api {
            alloc: export!("vigem_alloc", AllocFn),
            connect: export!("vigem_connect", ClientFn),
            disconnect: export!("vigem_disconnect", DropFn),
            free: export!("vigem_free", DropFn),
            x360_alloc: export!("vigem_target_x360_alloc", AllocFn),
            ds4_alloc: export!("vigem_target_ds4_alloc", AllocFn),
            target_add: export!("vigem_target_add", TargetFn),
            target_remove: export!("vigem_target_remove", TargetFn),
            target_free: export!("vigem_target_free", DropFn),
            x360_update: export!("vigem_target_x360_update", X360UpdateFn),
            ds4_update: export!("vigem_target_ds4_update", Ds4UpdateFn),
        })
    }
}
