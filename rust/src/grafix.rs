use core::ffi::{c_int, c_void};
use core::mem::size_of;
use core::ptr::{addr_of_mut, null, null_mut};

use windows_sys::core::{BOOL, PCSTR, PCWSTR};
use windows_sys::Win32::Foundation::{HGLOBAL, HINSTANCE, HRSRC, HWND};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, DeleteDC, DeleteObject, GetDC, GetLayout,
    GetStockObject, LineTo, MoveToEx, ReleaseDC, SelectObject, SetDIBitsToDevice, SetLayout, SetROP2, HBITMAP,
    HDC, HPEN, LAYOUT_RTL, PS_SOLID, R2_COPYPEN, R2_WHITE, SRCCOPY, BLACK_PEN, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, GDI_ERROR,
};
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA;
use windows_sys::Win32::System::LibraryLoader::{FindResourceW, LoadResource, LockResource};
use windows_sys::Win32::UI::WindowsAndMessaging::RT_BITMAP;

use crate::pref::{Preferences, PREF};
use crate::sound::EndTunes;

extern "C" {
    static mut hInst: HINSTANCE;
    static mut hwndMain: HWND;
    static mut dxWindow: c_int;
    static mut dyWindow: c_int;
    static mut cBombLeft: c_int;
    static mut cSec: c_int;
    static mut iButtonCur: c_int;
    static mut dxpBorder: c_int;
    static mut xBoxMac: c_int;
    static mut yBoxMac: c_int;
    static mut rgBlk: [i8; C_BLK_MAX];

    fn ClearField();
}

const DX_BLK: c_int = 16;
const DY_BLK: c_int = 16;
const DX_LED: c_int = 13;
const DY_LED: c_int = 23;
const DX_BUTTON: c_int = 24;
const DY_BUTTON: c_int = 24;
const DX_LEFT_SPACE: c_int = 12;
const DX_RIGHT_SPACE: c_int = 12;
const DY_TOP_SPACE: c_int = 12;
const DY_BOTTOM_SPACE: c_int = 12;
const DX_GRID_OFF: c_int = DX_LEFT_SPACE;
const DY_TOP_LED: c_int = DY_TOP_SPACE + 4;
const DY_GRID_OFF: c_int = DY_TOP_LED + DY_LED + 16;
const DX_LEFT_BOMB: c_int = DX_LEFT_SPACE + 5;
const DX_RIGHT_TIME: c_int = DX_RIGHT_SPACE + 5;

const I_BLK_MAX: usize = 16;
const I_LED_MAX: usize = 12;
const I_BUTTON_MAX: usize = 5;
const C_BLK_MAX: usize = 27 * 32;
const MASK_DATA: i32 = 0x1F;

const ID_BMP_BLOCKS: u16 = 410;
const ID_BMP_LED: u16 = 420;
const ID_BMP_BUTTON: u16 = 430;

const DEBUG_CREATE_DC: &[u8] = b"FLoad failed to create compatible dc\n\0";
const DEBUG_CREATE_BITMAP: &[u8] = b"Failed to create Bitmap\n\0";

static mut RG_DIB_OFF: [c_int; I_BLK_MAX] = [0; I_BLK_MAX];
static mut RG_DIB_LED_OFF: [c_int; I_LED_MAX] = [0; I_LED_MAX];
static mut RG_DIB_BUTTON_OFF: [c_int; I_BUTTON_MAX] = [0; I_BUTTON_MAX];

static mut H_RES_BLKS: HGLOBAL = null_mut();
static mut H_RES_LED: HGLOBAL = null_mut();
static mut H_RES_BUTTON: HGLOBAL = null_mut();

static mut LP_DIB_BLKS: *const u8 = null();
static mut LP_DIB_LED: *const u8 = null();
static mut LP_DIB_BUTTON: *const u8 = null();

static mut H_GRAY_PEN: HPEN = null_mut();

static mut MEM_BLK_DC: [HDC; I_BLK_MAX] = [null_mut(); I_BLK_MAX];
static mut MEM_BLK_BITMAP: [HBITMAP; I_BLK_MAX] = [null_mut(); I_BLK_MAX];

#[inline]
unsafe fn prefs_ptr() -> *mut PREF {
    addr_of_mut!(Preferences)
}

fn color_enabled() -> bool {
    unsafe { (*prefs_ptr()).fColor != 0 }
}

#[no_mangle]
pub unsafe extern "C" fn FInitLocal() -> BOOL {
    if FLoadBitmaps() == 0 {
        return 0;
    }

    ClearField();
    1
}

#[no_mangle]
pub unsafe extern "C" fn FLoadBitmaps() -> BOOL {
    if !load_bitmaps_impl() {
        return 0;
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn FreeBitmaps() {
    if !H_GRAY_PEN.is_null() {
        DeleteObject(H_GRAY_PEN as _);
        H_GRAY_PEN = null_mut();
    }

    H_RES_BLKS = null_mut();
    H_RES_LED = null_mut();
    H_RES_BUTTON = null_mut();

    LP_DIB_BLKS = null();
    LP_DIB_LED = null();
    LP_DIB_BUTTON = null();

    for i in 0..I_BLK_MAX {
        if !MEM_BLK_DC[i].is_null() {
            DeleteDC(MEM_BLK_DC[i]);
            MEM_BLK_DC[i] = null_mut();
        }
        if !MEM_BLK_BITMAP[i].is_null() {
            DeleteObject(MEM_BLK_BITMAP[i] as _);
            MEM_BLK_BITMAP[i] = null_mut();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn CleanUp() {
    FreeBitmaps();
    EndTunes();
}

#[no_mangle]
pub unsafe extern "C" fn DrawBlk(hdc: HDC, x: c_int, y: c_int) {
    BitBlt(
        hdc,
        (x << 4) + (DX_GRID_OFF - DX_BLK),
        (y << 4) + (DY_GRID_OFF - DY_BLK),
        DX_BLK,
        DY_BLK,
        block_dc(x, y),
        0,
        0,
        SRCCOPY,
    );
}

#[no_mangle]
pub unsafe extern "C" fn DisplayBlk(x: c_int, y: c_int) {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawBlk(hdc, x, y);
        ReleaseDC(hwndMain, hdc);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawGrid(hdc: HDC) {
    let mut dy = DY_GRID_OFF;
    for y in 1..=yBoxMac {
        let mut dx = DX_GRID_OFF;
        for x in 1..=xBoxMac {
            BitBlt(
                hdc,
                dx,
                dy,
                DX_BLK,
                DY_BLK,
                block_dc(x, y),
                0,
                0,
                SRCCOPY,
            );
            dx += DX_BLK;
        }
        dy += DY_BLK;
    }
}

#[no_mangle]
pub unsafe extern "C" fn DisplayGrid() {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawGrid(hdc);
        ReleaseDC(hwndMain, hdc);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawLed(hdc: HDC, x: c_int, i_led: c_int) {
    SetDIBitsToDevice(
        hdc,
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
        DIB_RGB_COLORS,
    );
}

#[no_mangle]
pub unsafe extern "C" fn DrawBombCount(hdc: HDC) {
    let layout = GetLayout(hdc);
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT_RTL) != 0;
    if mirrored {
        SetLayout(hdc, 0);
    }

    let (i_led, c_bombs) = if cBombLeft < 0 {
        (11, (-cBombLeft) % 100)
    } else {
        (cBombLeft / 100, cBombLeft % 100)
    };

    DrawLed(hdc, DX_LEFT_BOMB, i_led);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED, c_bombs / 10);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED * 2, c_bombs % 10);

    if mirrored {
        SetLayout(hdc, layout);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DisplayBombCount() {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawBombCount(hdc);
        ReleaseDC(hwndMain, hdc);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawTime(hdc: HDC) {
    let layout = GetLayout(hdc);
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT_RTL) != 0;
    if mirrored {
        SetLayout(hdc, 0);
    }

    let mut time = cSec;
    DrawLed(hdc, dxWindow - (DX_RIGHT_TIME + 3 * DX_LED + dxpBorder), time / 100);
    time %= 100;
    DrawLed(hdc, dxWindow - (DX_RIGHT_TIME + 2 * DX_LED + dxpBorder), time / 10);
    DrawLed(hdc, dxWindow - (DX_RIGHT_TIME + DX_LED + dxpBorder), time % 10);

    if mirrored {
        SetLayout(hdc, layout);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DisplayTime() {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawTime(hdc);
        ReleaseDC(hwndMain, hdc);
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawButton(hdc: HDC, i_button: c_int) {
    let x = (dxWindow - DX_BUTTON) >> 1;
    SetDIBitsToDevice(
        hdc,
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
        DIB_RGB_COLORS,
    );
}

#[no_mangle]
pub unsafe extern "C" fn DisplayButton(i_button: c_int) {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawButton(hdc, i_button);
        ReleaseDC(hwndMain, hdc);
    }
}

#[no_mangle]
pub unsafe extern "C" fn SetThePen(hdc: HDC, f_normal: c_int) {
    if (f_normal & 1) != 0 {
        SetROP2(hdc, R2_WHITE);
    } else {
        SetROP2(hdc, R2_COPYPEN);
        if !H_GRAY_PEN.is_null() {
            SelectObject(hdc, H_GRAY_PEN as _);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawBorder(
    hdc: HDC,
    mut x1: c_int,
    mut y1: c_int,
    mut x2: c_int,
    mut y2: c_int,
    width: c_int,
    f_normal: c_int,
) {
    let mut i = 0;
    SetThePen(hdc, f_normal);

    while i < width {
        y2 -= 1;
        MoveToEx(hdc, x1, y2, null_mut());
        LineTo(hdc, x1, y1);
        x1 += 1;
        LineTo(hdc, x2, y1);
        x2 -= 1;
        y1 += 1;
        i += 1;
    }

    if f_normal < 2 {
        SetThePen(hdc, f_normal ^ 1);
    }

    while i > 0 {
        y2 += 1;
        MoveToEx(hdc, x1, y2, null_mut());
        x1 -= 1;
        x2 += 1;
        LineTo(hdc, x2, y2);
        y1 -= 1;
        LineTo(hdc, x2, y1);
        i -= 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn DrawBackground(hdc: HDC) {
    let mut x = dxWindow - 1;
    let mut y = dyWindow - 1;
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

    x = dxWindow - (DX_RIGHT_TIME + 3 * DX_LED + dxpBorder + 1);
    DrawBorder(hdc, x, DY_TOP_LED - 1, x + (DX_LED * 3 + 1), y, 1, 0);

    x = ((dxWindow - DX_BUTTON) >> 1) - 1;
    DrawBorder(hdc, x, DY_TOP_LED - 1, x + DX_BUTTON + 1, DY_TOP_LED + DY_BUTTON, 1, 2);
}

#[no_mangle]
pub unsafe extern "C" fn DrawScreen(hdc: HDC) {
    DrawBackground(hdc);
    DrawBombCount(hdc);
    DrawButton(hdc, iButtonCur);
    DrawTime(hdc);
    DrawGrid(hdc);
}

#[no_mangle]
pub unsafe extern "C" fn DisplayScreen() {
    let hdc = GetDC(hwndMain);
    if !hdc.is_null() {
        DrawScreen(hdc);
        ReleaseDC(hwndMain, hdc);
    }
}

fn load_bitmaps_impl() -> bool {
    unsafe {
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
            GetStockObject(BLACK_PEN) as HPEN
        } else {
            CreatePen(PS_SOLID, 1, rgb(128, 128, 128))
        };

        if H_GRAY_PEN.is_null() {
            return false;
        }

        let header = dib_header_size();

        let cb_blk = cb_bitmap(DX_BLK, DY_BLK);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_BLK_MAX {
                RG_DIB_OFF[i] = header + (i as c_int) * cb_blk;
            }

        let cb_led = cb_bitmap(DX_LED, DY_LED);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_LED_MAX {
                RG_DIB_LED_OFF[i] = header + (i as c_int) * cb_led;
            }

        let cb_button = cb_bitmap(DX_BUTTON, DY_BUTTON);
        #[allow(clippy::needless_range_loop)]
        for i in 0..I_BUTTON_MAX {
                RG_DIB_BUTTON_OFF[i] = header + (i as c_int) * cb_button;
            }

        let hdc = GetDC(hwndMain);
        if hdc.is_null() {
            return false;
        }

        for i in 0..I_BLK_MAX {
            MEM_BLK_DC[i] = CreateCompatibleDC(hdc);
            if MEM_BLK_DC[i].is_null() {
                OutputDebugStringA(DEBUG_CREATE_DC.as_ptr() as PCSTR);
            }

            MEM_BLK_BITMAP[i] = CreateCompatibleBitmap(hdc, DX_BLK, DX_BLK);
            if MEM_BLK_BITMAP[i].is_null() {
                OutputDebugStringA(DEBUG_CREATE_BITMAP.as_ptr() as PCSTR);
            }

            if !MEM_BLK_DC[i].is_null() && !MEM_BLK_BITMAP[i].is_null() {
                SelectObject(MEM_BLK_DC[i], MEM_BLK_BITMAP[i] as _);
                SetDIBitsToDevice(
                    MEM_BLK_DC[i],
                    0,
                    0,
                    DX_BLK as u32,
                    DY_BLK as u32,
                    0,
                    0,
                    0,
                    DY_BLK as u32,
                    block_bits(i as c_int),
                    dib_info(LP_DIB_BLKS),
                    DIB_RGB_COLORS,
                );
            }
        }

        ReleaseDC(hwndMain, hdc);
        true
    }
}

unsafe fn load_bitmap_resource(id: u16) -> HGLOBAL {
    let offset = if color_enabled() { 0 } else { 1 };
    let resource_id = id + offset;
    let res: HRSRC = FindResourceW(hInst, make_int_resource(resource_id), RT_BITMAP);
    if res.is_null() {
        return null_mut();
    }
    LoadResource(hInst, res)
}

fn dib_header_size() -> c_int {
    let palette_entries = if color_enabled() { 16 } else { 2 };
    (size_of::<BITMAPINFOHEADER>() + (palette_entries as usize) * 4) as c_int
}

fn cb_bitmap(x: c_int, y: c_int) -> c_int {
    let mut bits = x;
    if color_enabled() {
        bits *= 4;
    }
    let stride = ((bits + 31) >> 5) << 2;
    y * stride
}

fn block_bits(i: c_int) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_BLK_MAX);
        LP_DIB_BLKS.add(RG_DIB_OFF[idx] as usize) as *const c_void
    }
}

fn led_bits(i: c_int) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_LED_MAX);
        LP_DIB_LED.add(RG_DIB_LED_OFF[idx] as usize) as *const c_void
    }
}

fn button_bits(i: c_int) -> *const c_void {
    unsafe {
        let idx = clamp_index(i, I_BUTTON_MAX);
        LP_DIB_BUTTON.add(RG_DIB_BUTTON_OFF[idx] as usize) as *const c_void
    }
}

fn dib_info(ptr: *const u8) -> *const BITMAPINFO {
    ptr as *const BITMAPINFO
}

fn block_dc(x: c_int, y: c_int) -> HDC {
    unsafe {
        let idx = block_sprite_index(x, y);
            if idx < I_BLK_MAX {
            MEM_BLK_DC[idx]
        } else {
            null_mut()
        }
    }
}

fn block_sprite_index(x: c_int, y: c_int) -> usize {
    unsafe {
        let offset = ((y as isize) << 5) + x as isize;
        let ptr = addr_of_mut!(rgBlk).cast::<i8>();
        let value = *ptr.offset(offset) as i32;
        (value & MASK_DATA) as usize
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

fn make_int_resource(id: u16) -> PCWSTR {
    id as usize as *const u16
}

fn clamp_index(value: c_int, max: usize) -> usize {
    if value <= 0 {
        0
    } else {
        let idx = value as usize;
        idx.min(max.saturating_sub(1))
    }
}
