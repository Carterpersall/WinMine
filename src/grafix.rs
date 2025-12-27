use core::mem::size_of;
use core::ptr::null;
use core::sync::atomic::Ordering::Relaxed;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Graphics::Gdi::{
    GDI_ERROR, GetLayout, R2_COPYPEN, R2_WHITE, SetDIBitsToDevice, SetLayout, SetROP2,
};
use winsafe::{
    self as w, BITMAPINFO, BITMAPINFOHEADER, HRSRCMEM, IdStr, RtStr,
    co::{DIB, LAYOUT, PS, ROP, RT, STOCK_PEN},
    guard::{DeleteDCGuard, DeleteObjectGuard},
    prelude::*,
};

use crate::globals::{CXBORDER, WINDOW_HEIGHT, WINDOW_WIDTH, global_state};
use crate::rtns::{
    BOARD_HEIGHT, BOARD_INDEX_SHIFT, BOARD_WIDTH, BOMBS_LEFT, BTN_FACE_STATE, BlockMask,
    ClearField, SECS_ELAPSED, board_mutex, preferences_mutex,
};
use crate::sound::EndTunes;

/// Width of a single board cell sprite in pixels.
pub const DX_BLK: i32 = 16;
/// Height of a single board cell sprite in pixels.
pub const DY_BLK: i32 = 16;
/// Width of an LED digit in pixels.
pub const DX_LED: i32 = 13;
/// Height of an LED digit in pixels.
pub const DY_LED: i32 = 23;
/// Width of the face button sprite in pixels.
pub const DX_BUTTON: i32 = 24;
/// Height of the face button sprite in pixels.
pub const DY_BUTTON: i32 = 24;
/// Left margin between the window frame and the board.
pub const DX_LEFT_SPACE: i32 = 12;
/// Right margin between the window frame and the board.
pub const DX_RIGHT_SPACE: i32 = 12;
/// Top margin above the LED row.
pub const DY_TOP_SPACE: i32 = 12;
/// Bottom margin below the grid.
pub const DY_BOTTOM_SPACE: i32 = 12;
/// Horizontal offset to the first cell, accounting for the left margin.
pub const DX_GRID_OFF: i32 = DX_LEFT_SPACE;
/// Vertical offset to the LED row.
pub const DY_TOP_LED: i32 = DY_TOP_SPACE + 4;
/// Vertical offset to the top of the grid.
pub const DY_GRID_OFF: i32 = DY_TOP_LED + DY_LED + 16;
/// X coordinate of the left edge of the bomb counter.
pub const DX_LEFT_BOMB: i32 = DX_LEFT_SPACE + 5;
/// X coordinate offset from the right edge for the timer counter.
pub const DX_RIGHT_TIME: i32 = DX_RIGHT_SPACE + 5;

/// Number of cell sprites packed into the block bitmap sheet.
pub const I_BLK_MAX: usize = 16;
/// Number of digits stored in the LED bitmap sheet.
pub const I_LED_MAX: usize = 12;
/// Face button sprites available in the bitmap sheet.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum ButtonSprite {
    Happy = 0,
    Caution = 1,
    Lose = 2,
    Win = 3,
    Down = 4,
}
/// Number of face button sprites.
pub const BUTTON_SPRITE_COUNT: usize = 5;
/// Bitmap resources embedded in the executable.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum BitmapId {
    /// Packed block spritesheet (color + monochrome variants).
    Blocks = 410,
    /// LED digit spritesheet (color + monochrome variants).
    Led = 420,
    /// Face button spritesheet (color + monochrome variants).
    Button = 430,
}

/// Debug string emitted when a compatible DC cannot be created.
const DEBUG_CREATE_DC: &[u8] = b"FLoad failed to create compatible dc\n";
/// Debug string emitted when a compatible bitmap cannot be created.
const DEBUG_CREATE_BITMAP: &[u8] = b"Failed to create Bitmap\n";

/// Internal state tracking loaded graphics resources and cached DCs
struct GrafixState {
    /// Precalculated byte offsets to each block sprite within the DIB
    rg_dib_off: [usize; I_BLK_MAX],
    /// Precalculated byte offsets to each LED digit within the DIB
    rg_dib_led_off: [usize; I_LED_MAX],
    /// Precalculated byte offsets to each button sprite within the DIB
    rg_dib_button_off: [usize; BUTTON_SPRITE_COUNT],
    /// Resource handle for the block spritesheet
    h_res_blks: HRSRCMEM,
    /// Resource handle for the LED digits spritesheet
    h_res_led: HRSRCMEM,
    /// Resource handle for the button spritesheet
    h_res_button: HRSRCMEM,
    /// Pointer to the loaded block sprites DIB
    lp_dib_blks: *const BITMAPINFO,
    /// Pointer to the loaded LED digits DIB
    lp_dib_led: *const BITMAPINFO,
    /// Pointer to the loaded button sprites DIB
    lp_dib_button: *const BITMAPINFO,
    /// Cached gray pen used for monochrome rendering
    h_gray_pen: w::HPEN,
    /// Cached compatible DCs for each block sprite
    mem_blk_dc: [Option<DeleteDCGuard>; I_BLK_MAX],
    /// Cached compatible bitmaps for each block sprite
    mem_blk_bitmap: [Option<DeleteObjectGuard<w::HBITMAP>>; I_BLK_MAX],
}

unsafe impl Send for GrafixState {}
unsafe impl Sync for GrafixState {}

impl Default for GrafixState {
    fn default() -> Self {
        Self {
            rg_dib_off: [0; I_BLK_MAX],
            rg_dib_led_off: [0; I_LED_MAX],
            rg_dib_button_off: [0; BUTTON_SPRITE_COUNT],
            h_res_blks: HRSRCMEM::NULL,
            h_res_led: HRSRCMEM::NULL,
            h_res_button: HRSRCMEM::NULL,
            lp_dib_blks: null(),
            lp_dib_led: null(),
            lp_dib_button: null(),
            h_gray_pen: w::HPEN::NULL,
            mem_blk_dc: [const { None }; I_BLK_MAX],
            mem_blk_bitmap: [const { None }; I_BLK_MAX],
        }
    }
}

static GRAFIX_STATE: OnceLock<Mutex<GrafixState>> = OnceLock::new();

fn grafix_state() -> &'static Mutex<GrafixState> {
    GRAFIX_STATE.get_or_init(|| Mutex::new(GrafixState::default()))
}

fn current_color_flag() -> bool {
    let prefs = match preferences_mutex().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    prefs.fColor
}

fn main_window() -> Option<w::HWND> {
    let state = global_state();
    let guard = match state.hwnd_main.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .as_opt()
        .map(|hwnd| unsafe { w::HWND::from_ptr(hwnd.ptr()) })
}

pub fn FInitLocal() -> Result<(), Box<dyn std::error::Error>> {
    // Load the sprite resources and reset the minefield before gameplay starts.
    FLoadBitmaps()?;
    ClearField();
    Ok(())
}

pub fn FLoadBitmaps() -> Result<(), Box<dyn std::error::Error>> {
    // Wrapper retained for compatibility with the original export table.
    load_bitmaps_impl()
}

pub fn FreeBitmaps() {
    // Tear down cached pens, handles, and scratch DCs when leaving the app.
    let mut state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if state.h_gray_pen != w::HPEN::NULL {
        unsafe {
            let pen = w::HPEN::from_ptr(state.h_gray_pen.ptr());
            let _ = DeleteObjectGuard::new(pen);
        }
        state.h_gray_pen = w::HPEN::NULL;
    }

    state.h_res_blks = HRSRCMEM::NULL;
    state.h_res_led = HRSRCMEM::NULL;
    state.h_res_button = HRSRCMEM::NULL;

    state.lp_dib_blks = null();
    state.lp_dib_led = null();
    state.lp_dib_button = null();

    for i in 0..I_BLK_MAX {
        if state.mem_blk_dc[i].is_some() {
            let _ = state.mem_blk_dc[i].take();
        }
        if state.mem_blk_bitmap[i].is_some() {
            let _ = state.mem_blk_bitmap[i].take();
        }
    }
}

pub fn CleanUp() {
    // Matching the C code, graphics cleanup also silences any outstanding audio.
    FreeBitmaps();
    EndTunes();
}

pub fn DrawBlk(hdc: &w::HDC, x: i32, y: i32) {
    // Bit-blit a single cell sprite using the precalculated offsets.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(src) = block_dc(&state, x, y) else {
        return;
    };

    let _ = hdc.BitBlt(
        w::POINT::with(
            (x << 4) + (DX_GRID_OFF - DX_BLK),
            (y << 4) + (DY_GRID_OFF - DY_BLK),
        ),
        w::SIZE::with(DX_BLK, DY_BLK),
        src,
        w::POINT::new(),
        ROP::SRCCOPY,
    );
}

pub fn DisplayBlk(x: i32, y: i32) {
    // Convenience wrapper that repaints one tile directly to the main window.
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawBlk(&hdc, x, y);
    }
}

pub fn DrawGrid(hdc: &w::HDC) {
    // Rebuild the visible grid by iterating over the current rgBlk contents.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let y_max = BOARD_HEIGHT.load(Relaxed);
    let x_max = BOARD_WIDTH.load(Relaxed);
    let mut dy = DY_GRID_OFF;
    for y in 1..=y_max {
        let mut dx = DX_GRID_OFF;
        for x in 1..=x_max {
            if let Some(src) = block_dc(&state, x, y) {
                let _ = hdc.BitBlt(
                    w::POINT::with(dx, dy),
                    w::SIZE::with(DX_BLK, DY_BLK),
                    src,
                    w::POINT::new(),
                    ROP::SRCCOPY,
                );
            }
            dx += DX_BLK;
        }
        dy += DY_BLK;
    }
}

pub fn DisplayGrid() {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawGrid(&hdc);
    }
}

pub fn DrawLed(hdc: &w::HDC, x: i32, i_led: i32) {
    // LED digits stay as packed DIBs, so we blast them straight from the resource.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    unsafe {
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
            // Get the pointer to the LED digit bits using the precalculated offset
            state
                .lp_dib_led
                .byte_add(state.rg_dib_led_off[i_led as usize] as usize)
                .cast(),
            state.lp_dib_led as *const _,
            DIB::RGB_COLORS.raw(),
        );
    }
}

pub fn DrawBombCount(hdc: &w::HDC) {
    // Handle when the window is mirrored for RTL languages by temporarily disabling mirroring
    let layout = unsafe { GetLayout(hdc.ptr()) };
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), 0);
        }
    }

    // Calculate the three LED digits to display for the bomb counter.
    let bombs = BOMBS_LEFT.load(Relaxed);
    let (i_led, c_bombs) = if bombs < 0 {
        (11, (-bombs) % 100)
    } else {
        (bombs / 100, bombs % 100)
    };

    // Draw each of the three digits in sequence.
    DrawLed(hdc, DX_LEFT_BOMB, i_led);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED, c_bombs / 10);
    DrawLed(hdc, DX_LEFT_BOMB + DX_LED * 2, c_bombs % 10);

    // Restore the original layout if it was mirrored
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), layout);
        }
    }
}

pub fn DisplayBombCount() {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawBombCount(&hdc);
    }
}

pub fn DrawTime(hdc: &w::HDC) {
    // The timer uses the same mirroring trick as the bomb counter.
    let layout = unsafe { GetLayout(hdc.ptr()) };
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), 0);
        }
    }

    let mut time = SECS_ELAPSED.load(Relaxed);
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let border = CXBORDER.load(Relaxed);
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
        unsafe {
            SetLayout(hdc.ptr(), layout);
        }
    }
}

pub fn DisplayTime() {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawTime(&hdc);
    }
}

pub fn DrawButton(hdc: &w::HDC, sprite: ButtonSprite) {
    // Center the face button and pull the requested state from the button sheet.
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let x = (dx_window - DX_BUTTON) >> 1;
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    unsafe {
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
            // Get the pointer to the button sprite bits using the precalculated offset
            state
                .lp_dib_button
                .byte_add(state.rg_dib_button_off[sprite as usize] as usize)
                .cast(),
            state.lp_dib_button as *const _,
            DIB::RGB_COLORS.raw(),
        );
    }
}

pub fn DisplayButton(sprite: ButtonSprite) {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawButton(&hdc, sprite);
    }
}

pub fn SetThePen(hdc: &w::HDC, f_normal: i32) {
    // Reproduce the old pen combos: even values use the gray pen, odd values use white.
    if (f_normal & 1) != 0 {
        unsafe {
            SetROP2(hdc.ptr(), R2_WHITE);
        }
    } else {
        let state = match grafix_state().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe {
            SetROP2(hdc.ptr(), R2_COPYPEN);
            if state.h_gray_pen != w::HPEN::NULL {
                let pen = w::HPEN::from_ptr(state.h_gray_pen.ptr());
                let _ = hdc.SelectObject(&pen).map(|mut guard| guard.leak());
            }
        }
    }
}

pub fn DrawBorder(
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

pub fn DrawBackground(hdc: &w::HDC) {
    // Repaint every chrome element (outer frame, counters, smiley bezel) before drawing content.
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Relaxed);
    let border = CXBORDER.load(Relaxed);
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

pub fn DrawScreen(hdc: &w::HDC) {
    // Full-screen refresh that mirrors the original InvalidateRect/WM_PAINT handler.
    DrawBackground(hdc);
    DrawBombCount(hdc);
    let sprite = match BTN_FACE_STATE.load(Relaxed) {
        0 => ButtonSprite::Happy,
        1 => ButtonSprite::Caution,
        2 => ButtonSprite::Lose,
        3 => ButtonSprite::Win,
        _ => ButtonSprite::Down,
    };
    DrawButton(hdc, sprite);
    DrawTime(hdc);
    DrawGrid(hdc);
}

pub fn DisplayScreen() {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawScreen(&hdc);
    }
}

fn load_bitmaps_impl() -> Result<(), Box<dyn std::error::Error>> {
    let color_on = current_color_flag();
    let mut state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let Some((h_blks, lp_blks)) = load_bitmap_resource(BitmapId::Blocks, color_on) else {
        return Err("Failed to load block bitmap resource".into());
    };
    let Some((h_led, lp_led)) = load_bitmap_resource(BitmapId::Led, color_on) else {
        return Err("Failed to load LED bitmap resource".into());
    };
    let Some((h_button, lp_button)) = load_bitmap_resource(BitmapId::Button, color_on) else {
        return Err("Failed to load button bitmap resource".into());
    };

    state.h_res_blks = h_blks;
    state.h_res_led = h_led;
    state.h_res_button = h_button;

    state.lp_dib_blks = lp_blks as *const BITMAPINFO;
    state.lp_dib_led = lp_led as *const BITMAPINFO;
    state.lp_dib_button = lp_button as *const BITMAPINFO;

    state.h_gray_pen = if !color_on {
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

    if state.h_gray_pen == w::HPEN::NULL {
        return Err("Failed to create gray pen".into());
    }

    let header = dib_header_size(color_on);

    let cb_blk = cb_bitmap(color_on, DX_BLK, DY_BLK);
    for (i, off) in state.rg_dib_off.iter_mut().enumerate() {
        *off = header + i * cb_blk;
    }

    let cb_led = cb_bitmap(color_on, DX_LED, DY_LED);
    for (i, off) in state.rg_dib_led_off.iter_mut().enumerate() {
        *off = header + i * cb_led;
    }

    let cb_button = cb_bitmap(color_on, DX_BUTTON, DY_BUTTON);
    for (i, off) in state.rg_dib_button_off.iter_mut().enumerate() {
        *off = header + i * cb_button;
    }

    let hwnd = match main_window() {
        Some(hwnd) => hwnd,
        None => return Err("Main window not available".into()),
    };

    let hdc = match hwnd.GetDC() {
        Ok(dc) => dc,
        Err(e) => return Err(format!("Failed to get device context: {}", e).into()),
    };

    // Build a dedicated compatible DC + bitmap for every block sprite to speed up drawing.
    for i in 0..I_BLK_MAX {
        state.mem_blk_dc[i] = match hdc.CreateCompatibleDC() {
            Ok(dc_guard) => Some(dc_guard),
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_DC) {
                    w::OutputDebugString(msg);
                }
                None
            }
        };

        state.mem_blk_bitmap[i] = match hdc.CreateCompatibleBitmap(DX_BLK, DX_BLK) {
            Ok(bmp_guard) => Some(bmp_guard),
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_BITMAP) {
                    w::OutputDebugString(msg);
                }
                None
            }
        };

        if state.mem_blk_dc[i].is_some()
            && state.mem_blk_bitmap[i].is_some()
            && state.mem_blk_dc[i].is_some()
            && let Some(dc_guard) = state.mem_blk_dc[i].as_ref()
            && let Some(bmp_guard) = state.mem_blk_bitmap[i].as_ref()
        {
            let bmp_h = unsafe { w::HBITMAP::from_ptr(bmp_guard.ptr()) };
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&bmp_h) {
                let _ = sel_guard.leak();
            }
            unsafe {
                SetDIBitsToDevice(
                    dc_guard.ptr(),
                    0,
                    0,
                    DX_BLK as u32,
                    DY_BLK as u32,
                    0,
                    0,
                    0,
                    DY_BLK as u32,
                    // Get the pointer to the block sprite bits using the precalculated offset
                    state
                        .lp_dib_blks
                        .byte_add(state.rg_dib_off[i] as usize)
                        .cast(),
                    state.lp_dib_blks as *const _,
                    DIB::RGB_COLORS.raw(),
                );
            }
        }
    }

    Ok(())
}

fn load_bitmap_resource(id: BitmapId, color_on: bool) -> Option<(HRSRCMEM, *const u8)> {
    let offset = if color_on { 0 } else { 1 };
    let resource_id = (id as u16) + offset;
    // Colorless devices load the grayscale resource IDs immediately following the color ones.
    let inst_guard = match global_state().h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let res_info = inst_guard
        .FindResource(IdStr::Id(resource_id), RtStr::Rt(RT::BITMAP))
        .ok()?;
    let res_loaded = inst_guard.LoadResource(&res_info).ok()?;
    let lp = inst_guard
        .LockResource(&res_info, &res_loaded)
        .ok()?
        .as_ptr();
    Some((res_loaded, lp))
}

/// Calculate the size of the DIB header plus color palette
/// # Arguments
/// * `color_on` - Whether color mode is enabled
/// # Returns
/// Size in bytes of the DIB header and palette
fn dib_header_size(color_on: bool) -> usize {
    let palette_entries = if color_on { 16 } else { 2 };
    size_of::<BITMAPINFOHEADER>() + (palette_entries as usize) * 4
}

/// Calculate the byte size of a bitmap given its dimensions and color mode
/// # Arguments
/// * `color_on` - Whether color mode is enabled
/// * `x` - Width of the bitmap in pixels
/// * `y` - Height of the bitmap in pixels
/// # Returns
/// Size in bytes of the bitmap data
fn cb_bitmap(color_on: bool, x: i32, y: i32) -> usize {
    // Converts pixel sizes into the byte counts the SetDIBitsToDevice calls expect.
    let mut bits = x;
    if color_on {
        bits *= 4;
    }
    let stride = ((bits + 31) >> 5) << 2;
    (y * stride) as usize
}

/// Retrieve the cached compatible DC for the block at the given board coordinates.
/// # Arguments
/// * `state` - Reference to the current GrafixState
/// * `x` - X coordinate on the board (1-based)
/// * `y` - Y coordinate on the board (1-based)
/// # Returns
/// Optionally, a reference to the compatible DC for the block sprite
fn block_dc(state: &GrafixState, x: i32, y: i32) -> Option<&DeleteDCGuard> {
    let idx = block_sprite_index(x, y);
    if idx >= I_BLK_MAX {
        return None;
    }

    state.mem_blk_dc[idx].as_ref()
}

fn block_sprite_index(x: i32, y: i32) -> usize {
    // The board encoding packs state into rgBlk; mask out metadata to find the sprite index.
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    if offset < 0 {
        return 0;
    }
    let idx = offset as usize;
    let board = match board_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    board
        .get(idx)
        .copied()
        .map(|value| (value & BlockMask::Data as i8) as usize)
        .unwrap_or(0)
}

const fn rgb(r: u8, g: u8, b: u8) -> w::COLORREF {
    w::COLORREF::from_rgb(r, g, b)
}
