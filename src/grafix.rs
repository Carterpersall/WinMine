use core::ffi::c_void;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut, null, null_mut};
use core::sync::atomic::Ordering::Relaxed;

use windows_sys::core::{PCSTR, PCWSTR};
use windows_sys::Win32::Foundation::{HGLOBAL, HINSTANCE, HRSRC};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteDC, DeleteObject, GetLayout, SetDIBitsToDevice, SetLayout, SetROP2, BITMAPINFO,
    BITMAPINFOHEADER, GDI_ERROR, R2_COPYPEN, R2_WHITE,
};
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;
use windows_sys::Win32::System::LibraryLoader::{FindResourceW, LoadResource, LockResource};
use winsafe::{
    self as w,
    co::{DIB, LAYOUT, PS, ROP, RT, STOCK_PEN},
    prelude::*,
};

use crate::globals::{dxWindow, dxpBorder, dyWindow, hInst, hwndMain};
use crate::pref::Pref;
use crate::rtns::{cBombLeft, cSec, iButtonCur, rgBlk, xBoxMac, yBoxMac, ClearField, Preferences};
use crate::sound::EndTunes;

const DX_BLK: i32 = 16;
const DY_BLK: i32 = 16;
const DX_LED: i32 = 13;
const DY_LED: i32 = 23;
const DX_BUTTON: i32 = 24;
const DY_BUTTON: i32 = 24;
const DX_LEFT_SPACE: i32 = 12;
const DX_RIGHT_SPACE: i32 = 12;
const DY_TOP_SPACE: i32 = 12;
const DY_BOTTOM_SPACE: i32 = 12;
const DX_GRID_OFF: i32 = DX_LEFT_SPACE;
const DY_TOP_LED: i32 = DY_TOP_SPACE + 4;
const DY_GRID_OFF: i32 = DY_TOP_LED + DY_LED + 16;
const DX_LEFT_BOMB: i32 = DX_LEFT_SPACE + 5;
const DX_RIGHT_TIME: i32 = DX_RIGHT_SPACE + 5;

const I_BLK_MAX: usize = 16;
const I_LED_MAX: usize = 12;
const I_BUTTON_MAX: usize = 5;
const MASK_DATA: i32 = 0x1F;

const ID_BMP_BLOCKS: u16 = 410;
const ID_BMP_LED: u16 = 420;
const ID_BMP_BUTTON: u16 = 430;

const DEBUG_CREATE_DC: &[u8] = b"FLoad failed to create compatible dc\n\0";
const DEBUG_CREATE_BITMAP: &[u8] = b"Failed to create Bitmap\n\0";

// Cached offsets into each sprite sheet within its DIB.
static mut RG_DIB_OFF: [i32; I_BLK_MAX] = [0; I_BLK_MAX];
static mut RG_DIB_LED_OFF: [i32; I_LED_MAX] = [0; I_LED_MAX];
static mut RG_DIB_BUTTON_OFF: [i32; I_BUTTON_MAX] = [0; I_BUTTON_MAX];

// Resource handles and locked pointers for the block, LED, and button bitmaps.
static mut H_RES_BLKS: HGLOBAL = null_mut();
static mut H_RES_LED: HGLOBAL = null_mut();
static mut H_RES_BUTTON: HGLOBAL = null_mut();

static mut LP_DIB_BLKS: *const u8 = null();
static mut LP_DIB_LED: *const u8 = null();
static mut LP_DIB_BUTTON: *const u8 = null();

static mut H_GRAY_PEN: w::HPEN = w::HPEN::NULL;

// Per-cell memory DIBs so repeated blits avoid calling SetDIBitsToDevice each time.
static mut MEM_BLK_DC: [w::HDC; I_BLK_MAX] = [w::HDC::NULL; I_BLK_MAX];
static mut MEM_BLK_BITMAP: [w::HBITMAP; I_BLK_MAX] = [w::HBITMAP::NULL; I_BLK_MAX];

#[inline]
fn prefs_ptr() -> *mut Pref {
    addr_of_mut!(Preferences)
}

fn color_enabled() -> bool {
    unsafe { (*prefs_ptr()).fColor }
}

unsafe fn main_window() -> Option<&'static w::HWND> {
    hwndMain.as_opt()
}

pub unsafe fn FInitLocal() -> bool {
    // Load the sprite resources and reset the minefield before gameplay starts.
    if !FLoadBitmaps() {
        return false;
    }

    ClearField();
    true
}

pub unsafe fn FLoadBitmaps() -> bool {
    // Wrapper retained for compatibility with the original export table.
    load_bitmaps_impl()
}

pub unsafe fn FreeBitmaps() {
    // Tear down cached pens, handles, and scratch DCs when leaving the app.
    let pen = addr_of!(H_GRAY_PEN);
    let handle = core::ptr::read(pen);
    if handle != w::HPEN::NULL {
        DeleteObject(handle.ptr());
    }
    core::ptr::write(addr_of_mut!(H_GRAY_PEN), w::HPEN::NULL);

    H_RES_BLKS = null_mut();
    H_RES_LED = null_mut();
    H_RES_BUTTON = null_mut();

    LP_DIB_BLKS = null();
    LP_DIB_LED = null();
    LP_DIB_BUTTON = null();

    for i in 0..I_BLK_MAX {
        if MEM_BLK_DC[i] != w::HDC::NULL {
            DeleteDC(MEM_BLK_DC[i].ptr());
            MEM_BLK_DC[i] = w::HDC::NULL;
        }
        if MEM_BLK_BITMAP[i] != w::HBITMAP::NULL {
            DeleteObject(MEM_BLK_BITMAP[i].ptr());
            MEM_BLK_BITMAP[i] = w::HBITMAP::NULL;
        }
    }
}

pub fn CleanUp() {
    // Matching the C code, graphics cleanup also silences any outstanding audio.
    unsafe { FreeBitmaps() };
    EndTunes();
}

pub unsafe fn DrawBlk(hdc: &w::HDC, x: i32, y: i32) {
    // Bit-blit a single cell sprite using the precalculated offsets.
    let src = block_dc(x, y);
    if src == w::HDC::NULL {
        return;
    }

    let _ = hdc.BitBlt(
        w::POINT::with(
            (x << 4) + (DX_GRID_OFF - DX_BLK),
            (y << 4) + (DY_GRID_OFF - DY_BLK),
        ),
        w::SIZE::with(DX_BLK, DY_BLK),
        &src,
        w::POINT::new(),
        ROP::SRCCOPY,
    );
}

pub unsafe fn DisplayBlk(x: i32, y: i32) {
    // Convenience wrapper that repaints one tile directly to the main window.
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawBlk(&hdc, x, y);
        }
    }
}

pub unsafe fn DrawGrid(hdc: &w::HDC) {
    // Rebuild the visible grid by iterating over the current rgBlk contents.
    let y_max = yBoxMac.load(Relaxed);
    let x_max = xBoxMac.load(Relaxed);
    let mut dy = DY_GRID_OFF;
    for y in 1..=y_max {
        let mut dx = DX_GRID_OFF;
        for x in 1..=x_max {
            let src = block_dc(x, y);
            if src != w::HDC::NULL {
                let _ = hdc.BitBlt(
                    w::POINT::with(dx, dy),
                    w::SIZE::with(DX_BLK, DY_BLK),
                    &src,
                    w::POINT::new(),
                    ROP::SRCCOPY,
                );
            }
            dx += DX_BLK;
        }
        dy += DY_BLK;
    }
}

pub unsafe fn DisplayGrid() {
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawGrid(&hdc);
        }
    }
}

pub unsafe fn DrawLed(hdc: &w::HDC, x: i32, i_led: i32) {
    // LED digits stay as packed DIBs, so we blast them straight from the resource.
    SetDIBitsToDevice(
        hdc.ptr(),
        x,
        DY_TOP_LED,
        DX_LED as u32,
        DY_LED as u32,
        0,
        0,
        0,
        DY_LED as u32,
        led_bits(i_led),
        dib_info(LP_DIB_LED),
        DIB::RGB_COLORS.raw(),
    );
}

pub unsafe fn DrawBombCount(hdc: &w::HDC) {
    // Match the C logic: handle negatives, honor RTL mirroring, then paint three digits.
    let layout = GetLayout(hdc.ptr());
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        SetLayout(hdc.ptr(), 0);
    }

    let bombs = cBombLeft.load(Relaxed);
    let (i_led, c_bombs) = if bombs < 0 {
        (11, (-bombs) % 100)
    } else {
        (bombs / 100, bombs % 100)
    };

    DrawLed(hdc, DX_LEFT_BOMB, i_led);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED, c_bombs / 10);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED * 2, c_bombs % 10);

    if mirrored {
        SetLayout(hdc.ptr(), layout);
    }
}

pub unsafe fn DisplayBombCount() {
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawBombCount(&hdc);
        }
    }
}

pub unsafe fn DrawTime(hdc: &w::HDC) {
    // The timer uses the same mirroring trick as the bomb counter.
    let layout = GetLayout(hdc.ptr());
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        SetLayout(hdc.ptr(), 0);
    }

    let mut time = cSec.load(Relaxed);
    let dx_window = dxWindow.load(Relaxed);
    let border = dxpBorder.load(Relaxed);
    DrawLed(
        hdc,
        dx_window - (DX_RIGHT_TIME + 3 * DX_LED + border),
        time / 100,
    );
    time %= 100;
    DrawLed(
        hdc,
        dx_window - (DX_RIGHT_TIME + 2 * DX_LED + border),
        time / 10,
    );
    DrawLed(
        hdc,
        dx_window - (DX_RIGHT_TIME + DX_LED + border),
        time % 10,
    );

    if mirrored {
        SetLayout(hdc.ptr(), layout);
    }
}

pub unsafe fn DisplayTime() {
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawTime(&hdc);
        }
    }
}

pub unsafe fn DrawButton(hdc: &w::HDC, i_button: i32) {
    // Center the face button and pull the requested state from the button sheet.
    let dx_window = dxWindow.load(Relaxed);
    let x = (dx_window - DX_BUTTON) >> 1;
    SetDIBitsToDevice(
        hdc.ptr(),
        x,
        DY_TOP_LED,
        DX_BUTTON as u32,
        DY_BUTTON as u32,
        0,
        0,
        0,
        DY_BUTTON as u32,
        button_bits(i_button),
        dib_info(LP_DIB_BUTTON),
        DIB::RGB_COLORS.raw(),
    );
}

pub unsafe fn DisplayButton(i_button: i32) {
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawButton(&hdc, i_button);
        }
    }
}

pub unsafe fn SetThePen(hdc: &w::HDC, f_normal: i32) {
    // Reproduce the old pen combos: even values use the gray pen, odd values use white.
    if (f_normal & 1) != 0 {
        SetROP2(hdc.ptr(), R2_WHITE);
    } else {
        SetROP2(hdc.ptr(), R2_COPYPEN);
        let pen_ptr = addr_of!(H_GRAY_PEN);
        let pen = core::ptr::read(pen_ptr);
        if pen != w::HPEN::NULL {
            if let Ok(mut guard) = hdc.SelectObject(&pen) {
                let _ = guard.leak();
            }
        }
        core::ptr::write(addr_of_mut!(H_GRAY_PEN), pen);
    }
}

pub unsafe fn DrawBorder(
    hdc: &w::HDC,
    mut x1: i32,
    mut y1: i32,
    mut x2: i32,
    mut y2: i32,
    width: i32,
    f_normal: i32,
) {
    let mut i = 0;
    // Draw the raised or sunken beveled rectangle one pixel at a time, just like the Win16 code.
    SetThePen(hdc, f_normal);

    while i < width {
        y2 -= 1;
        let _ = hdc.MoveToEx(x1, y2, None);
        let _ = hdc.LineTo(x1, y1);
        x1 += 1;
        let _ = hdc.LineTo(x2, y1);
        x2 -= 1;
        y1 += 1;
        i += 1;
    }

    if f_normal < 2 {
        SetThePen(hdc, f_normal ^ 1);
    }

    while i > 0 {
        y2 += 1;
        let _ = hdc.MoveToEx(x1, y2, None);
        x1 -= 1;
        x2 += 1;
        let _ = hdc.LineTo(x2, y2);
        y1 -= 1;
        let _ = hdc.LineTo(x2, y1);
        i -= 1;
    }
}

pub unsafe fn DrawBackground(hdc: &w::HDC) {
    // Repaint every chrome element (outer frame, counters, smiley bezel) before drawing content.
    let dx_window = dxWindow.load(Relaxed);
    let dy_window = dyWindow.load(Relaxed);
    let border = dxpBorder.load(Relaxed);
    let mut x = dx_window - 1;
    let mut y = dy_window - 1;
    DrawBorder(hdc, 0, 0, x, y, 3, 1);

    x -= DX_RIGHT_SPACE - 3;
    y -= DY_BOTTOM_SPACE - 3;
    DrawBorder(hdc, DX_GRID_OFF - 3, DY_GRID_OFF - 3, x, y, 3, 0);
    DrawBorder(
        hdc,
        DX_GRID_OFF - 3,
        DY_TOP_SPACE - 3,
        x,
        DY_TOP_LED + DY_LED + (DY_BOTTOM_SPACE - 6),
        2,
        0,
    );

    x = DX_LEFT_BOMB + DX_LED * 3;
    y = DY_TOP_LED + DY_LED;
    DrawBorder(hdc, DX_LEFT_BOMB - 1, DY_TOP_LED - 1, x, y, 1, 0);

    x = dx_window - (DX_RIGHT_TIME + 3 * DX_LED + border + 1);
    DrawBorder(hdc, x, DY_TOP_LED - 1, x + (DX_LED * 3 + 1), y, 1, 0);

    x = ((dx_window - DX_BUTTON) >> 1) - 1;
    DrawBorder(
        hdc,
        x,
        DY_TOP_LED - 1,
        x + DX_BUTTON + 1,
        DY_TOP_LED + DY_BUTTON,
        1,
        2,
    );
}

pub unsafe fn DrawScreen(hdc: &w::HDC) {
    // Full-screen refresh that mirrors the original InvalidateRect/WM_PAINT handler.
    DrawBackground(hdc);
    DrawBombCount(hdc);
    DrawButton(hdc, iButtonCur.load(Relaxed));
    DrawTime(hdc);
    DrawGrid(hdc);
}

pub unsafe fn DisplayScreen() {
    if let Some(hwnd) = main_window() {
        if let Ok(hdc) = hwnd.GetDC() {
            DrawScreen(&hdc);
        }
    }
}

fn load_bitmaps_impl() -> bool {
    unsafe {
        // Grab each bitmap resource (color or mono variant) and keep it locked for lifetime of the process.
        H_RES_BLKS = load_bitmap_resource(ID_BMP_BLOCKS);
        H_RES_LED = load_bitmap_resource(ID_BMP_LED);
        H_RES_BUTTON = load_bitmap_resource(ID_BMP_BUTTON);

        if H_RES_BLKS.is_null() || H_RES_LED.is_null() || H_RES_BUTTON.is_null() {
            return false;
        }

        LP_DIB_BLKS = LockResource(H_RES_BLKS) as *const u8;
        LP_DIB_LED = LockResource(H_RES_LED) as *const u8;
        LP_DIB_BUTTON = LockResource(H_RES_BUTTON) as *const u8;

        if LP_DIB_BLKS.is_null() || LP_DIB_LED.is_null() || LP_DIB_BUTTON.is_null() {
            return false;
        }

        H_GRAY_PEN = if !color_enabled() {
            match w::HPEN::GetStockObject(STOCK_PEN::BLACK) {
                Ok(pen) => pen,
                Err(_) => w::HPEN::NULL,
            }
        } else {
            match w::HPEN::CreatePen(PS::SOLID, 1, rgb(128, 128, 128)) {
                Ok(mut pen) => pen.leak(),
                Err(_) => w::HPEN::NULL,
            }
        };

        if H_GRAY_PEN == w::HPEN::NULL {
            return false;
        }

        let header = dib_header_size();

        let cb_blk = cb_bitmap(DX_BLK, DY_BLK);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_BLK_MAX {
            RG_DIB_OFF[i] = header + (i as i32) * cb_blk;
        }

        let cb_led = cb_bitmap(DX_LED, DY_LED);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_LED_MAX {
            RG_DIB_LED_OFF[i] = header + (i as i32) * cb_led;
        }

        let cb_button = cb_bitmap(DX_BUTTON, DY_BUTTON);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_BUTTON_MAX {
            RG_DIB_BUTTON_OFF[i] = header + (i as i32) * cb_button;
        }

        let hwnd = match main_window() {
            Some(hwnd) => hwnd,
            None => return false,
        };

        let hdc = match hwnd.GetDC() {
            Ok(dc) => dc,
            Err(_) => return false,
        };

        // Build a dedicated compatible DC + bitmap for every block sprite to speed up drawing.
        for i in 0..I_BLK_MAX {
            MEM_BLK_DC[i] = match hdc.CreateCompatibleDC() {
                Ok(mut dc_guard) => dc_guard.leak(),
                Err(_) => {
                    OutputDebugStringA(DEBUG_CREATE_DC.as_ptr() as PCSTR);
                    w::HDC::NULL
                }
            };

            MEM_BLK_BITMAP[i] = match hdc.CreateCompatibleBitmap(DX_BLK, DX_BLK) {
                Ok(mut bmp_guard) => bmp_guard.leak(),
                Err(_) => {
                    OutputDebugStringA(DEBUG_CREATE_BITMAP.as_ptr() as PCSTR);
                    w::HBITMAP::NULL
                }
            };

            if MEM_BLK_DC[i] != w::HDC::NULL && MEM_BLK_BITMAP[i] != w::HBITMAP::NULL {
                if let Ok(mut guard) = MEM_BLK_DC[i].SelectObject(&MEM_BLK_BITMAP[i]) {
                    let _ = guard.leak();
                }
                SetDIBitsToDevice(
                    MEM_BLK_DC[i].ptr(),
                    0,
                    0,
                    DX_BLK as u32,
                    DY_BLK as u32,
                    0,
                    0,
                    0,
                    DY_BLK as u32,
                    block_bits(i as i32),
                    dib_info(LP_DIB_BLKS),
                    DIB::RGB_COLORS.raw(),
                );
            }
        }

        true
    }
}

unsafe fn load_bitmap_resource(id: u16) -> HGLOBAL {
    let offset = if color_enabled() { 0 } else { 1 };
    let resource_id = id + offset;
    // Colorless devices load the grayscale resource IDs immediately following the color ones.
    let inst = hInst.ptr() as HINSTANCE;
    let res: HRSRC = FindResourceW(
        inst,
        make_int_resource(resource_id),
        make_int_resource(RT::BITMAP.raw()),
    );
    if res.is_null() {
        return null_mut();
    }
    LoadResource(inst, res)
}

fn dib_header_size() -> i32 {
    let palette_entries = if color_enabled() { 16 } else { 2 };
    (size_of::<BITMAPINFOHEADER>() + (palette_entries as usize) * 4) as i32
}

fn cb_bitmap(x: i32, y: i32) -> i32 {
    // Converts pixel sizes into the byte counts the SetDIBitsToDevice calls expect.
    let mut bits = x;
    if color_enabled() {
        bits *= 4;
    }
    let stride = ((bits + 31) >> 5) << 2;
    y * stride
}

fn block_bits(i: i32) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_BLK_MAX);
        // Each offset already points past the BITMAPINFOHEADER to the raw pixel data.
        LP_DIB_BLKS.add(RG_DIB_OFF[idx] as usize) as *const c_void
    }
}

fn led_bits(i: i32) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_LED_MAX);
        LP_DIB_LED.add(RG_DIB_LED_OFF[idx] as usize) as *const c_void
    }
}

fn button_bits(i: i32) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_BUTTON_MAX);
        LP_DIB_BUTTON.add(RG_DIB_BUTTON_OFF[idx] as usize) as *const c_void
    }
}

fn dib_info(ptr: *const u8) -> *const BITMAPINFO {
    ptr as *const BITMAPINFO
}

fn block_dc(x: i32, y: i32) -> w::HDC {
    unsafe {
        let idx = block_sprite_index(x, y);
        if idx < I_BLK_MAX {
            w::HDC::from_ptr(MEM_BLK_DC[idx].ptr())
        } else {
            w::HDC::NULL
        }
    }
}

fn block_sprite_index(x: i32, y: i32) -> usize {
    unsafe {
        // The board encoding packs state into rgBlk; mask out metadata to find the sprite index.
        let offset = ((y as isize) << 5) + x as isize;
        let ptr = addr_of_mut!(rgBlk).cast::<i8>();
        let value = *ptr.offset(offset) as i32;
        (value & MASK_DATA) as usize
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> w::COLORREF {
    w::COLORREF::from_rgb(r, g, b)
}

fn make_int_resource(id: u16) -> PCWSTR {
    id as usize as *const u16
}

fn clamp_index(value: i32, max: usize) -> usize {
    if value <= 0 {
        0
    } else {
        let idx = value as usize;
        idx.min(max.saturating_sub(1))
    }
}
