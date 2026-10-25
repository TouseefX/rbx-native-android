//! `Embedded::capture`: the running game's pixels, for `screen_capture`.
//!
//! `PrintWindow(PW_RENDERFULLCONTENT)` asks the client to draw itself into
//! a bitmap, GPU-drawn content included on most drivers, and is not fooled
//! by an editor popup over the window. A swap chain it cannot reach comes
//! back black; the pixels are then copied off the screen with `BitBlt`,
//! where the window shows inside the editor's viewport.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HDC, SRCCOPY,
};
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

use super::{failed, hwnd, EmbedError, Embedded};

const PW_RENDERFULLCONTENT: u32 = 2;

/// Where a capture's pixels come from.
#[derive(Clone, Copy)]
enum Source {
    /// The window drawing itself (`PrintWindow`).
    Print,
    /// The screen under the window's client area (`BitBlt`).
    Screen,
}

impl Embedded {
    /// The window's client area as `(width, height, RGBA)`.
    pub fn capture(&self) -> Result<(u32, u32, Vec<u8>), EmbedError> {
        let window = hwnd(self.window);
        let (width, height) = client_size(window)?;
        let mut pixels = grab(window, width, height, Source::Print)?;
        if pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3] == [0, 0, 0])
        {
            pixels = grab(window, width, height, Source::Screen)?;
        }
        Ok((
            width as u32,
            height as u32,
            super::super::bgrx_to_rgba(&pixels),
        ))
    }
}

fn client_size(window: HWND) -> Result<(i32, i32), EmbedError> {
    let mut client = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `client` is a valid out pointer.
    if unsafe { GetClientRect(window, &mut client) } == 0 {
        return Err(failed("GetClientRect"));
    }
    let size = (client.right - client.left, client.bottom - client.top);
    if size.0 <= 0 || size.1 <= 0 {
        return Err(failed("capture of an empty window"));
    }
    Ok(size)
}

/// `width` x `height` BGRX pixels of `window`'s client area from `source`.
fn grab(window: HWND, width: i32, height: i32, source: Source) -> Result<Vec<u8>, EmbedError> {
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: every DC and bitmap obtained here is released on all paths
    // below; `pixels` holds width*height 32-bit pixels for GetDIBits.
    unsafe {
        let window_dc = GetDC(window);
        if window_dc.is_null() {
            return Err(failed("GetDC"));
        }
        let dc = CreateCompatibleDC(window_dc);
        let bitmap = CreateCompatibleBitmap(window_dc, width, height);
        let result = if dc.is_null() || bitmap.is_null() {
            Err(failed("CreateCompatibleBitmap"))
        } else {
            let old = SelectObject(dc, bitmap);
            let drawn = draw(window, dc, width, height, source);
            let result = drawn.and_then(|()| read(dc, bitmap, width, height, &mut pixels));
            SelectObject(dc, old);
            result
        };
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        ReleaseDC(window, window_dc);
        result?;
    }
    Ok(pixels)
}

/// Puts the window's pixels into the bitmap selected in `dc`.
///
/// # Safety
/// `dc` is a valid memory DC with a `width` x `height` bitmap selected.
unsafe fn draw(
    window: HWND,
    dc: HDC,
    width: i32,
    height: i32,
    source: Source,
) -> Result<(), EmbedError> {
    match source {
        Source::Print => {
            // SAFETY: `window` and `dc` are valid per the caller.
            if unsafe { PrintWindow(window, dc, PW_RENDERFULLCONTENT) } == 0 {
                return Err(failed("PrintWindow"));
            }
        }
        Source::Screen => {
            let mut origin = POINT { x: 0, y: 0 };
            // SAFETY: `origin` is a valid in/out pointer; the screen DC is
            // released before returning.
            unsafe {
                if ClientToScreen(window, &mut origin) == 0 {
                    return Err(failed("ClientToScreen"));
                }
                let screen = GetDC(null_mut());
                if screen.is_null() {
                    return Err(failed("GetDC"));
                }
                let copied = BitBlt(dc, 0, 0, width, height, screen, origin.x, origin.y, SRCCOPY);
                ReleaseDC(null_mut(), screen);
                if copied == 0 {
                    return Err(failed("BitBlt"));
                }
            }
        }
    }
    Ok(())
}

/// Reads the bitmap's rows, top first, into `pixels`.
///
/// # Safety
/// `dc` and `bitmap` are valid, and `pixels` holds `width` x `height`
/// 32-bit pixels.
unsafe fn read(
    dc: HDC,
    bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    width: i32,
    height: i32,
    pixels: &mut [u8],
) -> Result<(), EmbedError> {
    // SAFETY: plain-data struct, every field set or zero.
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    // SAFETY: per the caller; `info` describes a top-down 32-bit DIB.
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if rows == 0 {
        return Err(failed("GetDIBits"));
    }
    Ok(())
}
