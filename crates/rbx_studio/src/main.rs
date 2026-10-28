//! The `rbxstudio` binary: `rbx_studio::run_desktop`, the desktop launch
//! with its command line and environment aids. The editor itself lives in
//! the library this wraps, which is also what `crates/rbx_android` builds
//! against for the Android host (see `rbx_studio::boot_mobile`).

fn main() {
    rbx_studio::run_desktop();
}
