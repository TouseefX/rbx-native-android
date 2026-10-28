//! The pad as a ViGEmBus target.

use std::ffi::c_void;
use std::io;

use super::ffi::{Api, LoadError};
use super::{ds4_report, error_text, load_error_text, target, xusb_report, Target, DLL, VIGEM_OK};
use crate::gamepad::{Button, Device, PadError, PadState, Side};

/// A connected virtual pad; dropping it unplugs it.
pub struct VirtualPad {
    api: Api,
    client: *mut c_void,
    target: *mut c_void,
    kind: Target,
    device: Device,
    sent: PadState,
}

impl VirtualPad {
    /// Connects to ViGEmBus and plugs in the target standing in for `device`.
    pub fn create(device: Device) -> Result<VirtualPad, PadError> {
        let kind = target(device)
            .ok_or_else(|| PadError::Unavailable(device.unavailable().unwrap_or("").into()))?;
        let dll = std::env::current_exe()
            .map_err(PadError::Io)?
            .with_file_name(DLL);
        let api = Api::load(&dll).map_err(|error| {
            PadError::Unavailable(load_error_text(matches!(error, LoadError::MissingExport)))
        })?;
        // SAFETY (every call below): ViGEmClient's documented sequence, each
        // handle used only after its alloc succeeded and freed once.
        let client = unsafe { (api.alloc)() };
        if client.is_null() {
            return Err(PadError::Io(io::Error::from(io::ErrorKind::OutOfMemory)));
        }
        let connected = unsafe { (api.connect)(client) };
        if connected != VIGEM_OK {
            unsafe { (api.free)(client) };
            return Err(PadError::Unavailable(error_text(connected)));
        }
        let target = unsafe {
            match kind {
                Target::Xbox360 => (api.x360_alloc)(),
                Target::DualShock4 => (api.ds4_alloc)(),
            }
        };
        let added = if target.is_null() {
            0
        } else {
            unsafe { (api.target_add)(client, target) }
        };
        if added != VIGEM_OK {
            unsafe {
                if !target.is_null() {
                    (api.target_free)(target);
                }
                (api.disconnect)(client);
                (api.free)(client);
            }
            return Err(PadError::Unavailable(error_text(added)));
        }
        Ok(VirtualPad {
            api,
            client,
            target,
            kind,
            device,
            sent: PadState::default(),
        })
    }

    pub fn device(&self) -> Device {
        self.device
    }

    /// What the pad last reported.
    pub fn state(&self) -> PadState {
        self.sent
    }

    /// Reports `next` as one whole report.
    pub fn send(&mut self, next: PadState) -> io::Result<()> {
        // SAFETY: both handles are live until `Drop`.
        let result = unsafe {
            match self.kind {
                Target::Xbox360 => {
                    (self.api.x360_update)(self.client, self.target, xusb_report(&next))
                }
                Target::DualShock4 => {
                    (self.api.ds4_update)(self.client, self.target, ds4_report(&next))
                }
            }
        };
        if result != VIGEM_OK {
            return Err(io::Error::other(error_text(result)));
        }
        self.sent = next;
        Ok(())
    }

    pub fn press(&mut self, button: Button) -> io::Result<()> {
        self.with(|state| state.set(button, true))
    }

    pub fn release(&mut self, button: Button) -> io::Result<()> {
        self.with(|state| state.set(button, false))
    }

    pub fn stick(&mut self, side: Side, x: f32, y: f32) -> io::Result<()> {
        self.with(|state| state.set_stick(side, x, y))
    }

    pub fn trigger(&mut self, side: Side, value: f32) -> io::Result<()> {
        self.with(|state| state.set_trigger(side, value))
    }

    fn with(&mut self, change: impl FnOnce(&mut PadState)) -> io::Result<()> {
        let mut next = self.sent;
        change(&mut next);
        self.send(next)
    }

    /// Only a Linux pad has a device node.
    pub fn event_node(&self) -> Option<std::path::PathBuf> {
        None
    }
}

impl Drop for VirtualPad {
    fn drop(&mut self) {
        // SAFETY: the handles came from `create` and are released once.
        unsafe {
            (self.api.target_remove)(self.client, self.target);
            (self.api.target_free)(self.target);
            (self.api.disconnect)(self.client);
            (self.api.free)(self.client);
        }
    }
}
