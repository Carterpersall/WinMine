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

use crate::globals::{BASE_DPI, CXBORDER, UI_DPI, WINDOW_HEIGHT, WINDOW_WIDTH, global_state};
use crate::rtns::{
    BOARD_HEIGHT, BOARD_INDEX_SHIFT, BOARD_WIDTH, BOMBS_LEFT, BTN_FACE_STATE, BlockMask,
    ClearField, SECS_ELAPSED, board_mutex, preferences_mutex,
};
use crate::sound::EndTunes;

/// Width of a single board cell sprite in pixels.
pub const DX_BLK_96: i32 = 16;
/// Height of a single board cell sprite in pixels.
pub const DY_BLK_96: i32 = 16;
/// Width of an LED digit in pixels.
pub const DX_LED_96: i32 = 13;
/// Height of an LED digit in pixels.
pub const DY_LED_96: i32 = 23;
/// Width of the face button sprite in pixels.
pub const DX_BUTTON_96: i32 = 24;
/// Height of the face button sprite in pixels.
pub const DY_BUTTON_96: i32 = 24;
/// Left margin between the window frame and the board.
pub const DX_LEFT_SPACE_96: i32 = 12;
/// Right margin between the window frame and the board.
pub const DX_RIGHT_SPACE_96: i32 = 12;
/// Top margin above the LED row.
pub const DY_TOP_SPACE_96: i32 = 12;
/// Bottom margin below the grid.
pub const DY_BOTTOM_SPACE_96: i32 = 12;
// Note: Adding the offsets cause the DPI scaling to have minor rounding errors at specific DPIs.
// However, all common DPIs (100%, 125%, 150%, 175%, 200%) produce correct results.
/// Vertical offset to the LED row.
pub const DY_TOP_LED_96: i32 = DY_TOP_SPACE_96 + 4;
/// Vertical offset to the top of the grid.
pub const DY_GRID_OFF_96: i32 = DY_TOP_LED_96 + DY_LED_96 + 16;
/// X coordinate of the left edge of the bomb counter.
pub const DX_LEFT_BOMB_96: i32 = DX_LEFT_SPACE_96 + 5;
/// X coordinate offset from the right edge for the timer counter.
pub const DX_RIGHT_TIME_96: i32 = DX_RIGHT_SPACE_96 + 5;

// Classic WinMine assets and layout were authored for 96 DPI.
//
// The *resource bitmaps* remain in these fixed pixel sizes, but all *UI layout*
// must be scaled according to the current DPI.

/// Scale a 96-DPI measurement to the current UI DPI
/// # Arguments
/// * `value_96` - The measurement in pixels at 96 DPI.
/// # Returns
/// The measurement scaled to the current UI DPI.
pub fn scale_dpi(value_96: i32) -> i32 {
    w::MulDiv(value_96, UI_DPI.load(Relaxed) as i32, BASE_DPI as i32)
}

/// Number of cell sprites packed into the block bitmap sheet.
const I_BLK_MAX: usize = 16;
/// Number of digits stored in the LED bitmap sheet.
const I_LED_MAX: usize = 12;
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
const BUTTON_SPRITE_COUNT: usize = 5;
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
    /// Cached compatible DCs for each LED digit
    mem_led_dc: [Option<DeleteDCGuard>; I_LED_MAX],
    /// Cached compatible bitmaps for each LED digit
    mem_led_bitmap: [Option<DeleteObjectGuard<w::HBITMAP>>; I_LED_MAX],
    /// Cached compatible DCs for each face button sprite
    mem_button_dc: [Option<DeleteDCGuard>; BUTTON_SPRITE_COUNT],
    /// Cached compatible bitmaps for each face button sprite
    mem_button_bitmap: [Option<DeleteObjectGuard<w::HBITMAP>>; BUTTON_SPRITE_COUNT],
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
            mem_led_dc: [const { None }; I_LED_MAX],
            mem_led_bitmap: [const { None }; I_LED_MAX],
            mem_button_dc: [const { None }; BUTTON_SPRITE_COUNT],
            mem_button_bitmap: [const { None }; BUTTON_SPRITE_COUNT],
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

    for i in 0..I_LED_MAX {
        if state.mem_led_dc[i].is_some() {
            let _ = state.mem_led_dc[i].take();
        }
        if state.mem_led_bitmap[i].is_some() {
            let _ = state.mem_led_bitmap[i].take();
        }
    }

    for i in 0..BUTTON_SPRITE_COUNT {
        if state.mem_button_dc[i].is_some() {
            let _ = state.mem_button_dc[i].take();
        }
        if state.mem_button_bitmap[i].is_some() {
            let _ = state.mem_button_bitmap[i].take();
        }
    }
}

/// Clean up graphics resources and silence audio on exit.
pub fn CleanUp() {
    FreeBitmaps();
    EndTunes();
}

fn DrawBlk(hdc: &w::HDC, x: i32, y: i32) {
    // Bit-blit a single cell sprite using the precalculated offsets.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(src) = block_dc(&state, x, y) else {
        return;
    };

    let dst_w = scale_dpi(DX_BLK_96);
    let dst_h = scale_dpi(DY_BLK_96);
    let dst_x = (x * dst_w) + (scale_dpi(DX_LEFT_SPACE_96) - dst_w);
    let dst_y = (y * dst_h) + (scale_dpi(DY_GRID_OFF_96) - dst_h);

    // Blocks are cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
    let _ = hdc.BitBlt(
        w::POINT::with(dst_x, dst_y),
        w::SIZE::with(dst_w, dst_h),
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

fn DrawGrid(hdc: &w::HDC) {
    // Rebuild the visible grid by iterating over the current rgBlk contents.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let y_max = BOARD_HEIGHT.load(Relaxed);
    let x_max = BOARD_WIDTH.load(Relaxed);
    let dst_w = scale_dpi(DX_BLK_96);
    let dst_h = scale_dpi(DY_BLK_96);

    let mut dy = scale_dpi(DY_GRID_OFF_96);
    for y in 1..=y_max {
        let mut dx = scale_dpi(DX_LEFT_SPACE_96);
        for x in 1..=x_max {
            if let Some(src) = block_dc(&state, x, y) {
                let _ = hdc.BitBlt(
                    w::POINT::with(dx, dy),
                    w::SIZE::with(dst_w, dst_h),
                    src,
                    w::POINT::new(),
                    ROP::SRCCOPY,
                );
            }
            dx += dst_w;
        }
        dy += dst_h;
    }
}

pub fn DisplayGrid() {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawGrid(&hdc);
    }
}

fn DrawLed(hdc: &w::HDC, x: i32, i_led: i32) {
    // LEDs are cached into compatible bitmaps so we can scale them with StretchBlt.
    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let idx = i_led as usize;
    if idx >= I_LED_MAX {
        return;
    }
    let Some(src) = state.mem_led_dc[idx].as_ref() else {
        return;
    };

    let _ = hdc.SetStretchBltMode(w::co::STRETCH_MODE::COLORONCOLOR);
    let _ = hdc.StretchBlt(
        w::POINT::with(x, scale_dpi(DY_TOP_LED_96)),
        w::SIZE::with(scale_dpi(DX_LED_96), scale_dpi(DY_LED_96)),
        src,
        w::POINT::new(),
        w::SIZE::with(DX_LED_96, DY_LED_96),
        ROP::SRCCOPY,
    );
}

fn DrawBombCount(hdc: &w::HDC) {
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
    let x0 = scale_dpi(DX_LEFT_BOMB_96);
    let dx = scale_dpi(DX_LED_96);
    DrawLed(hdc, x0, i_led);
    DrawLed(hdc, x0 + dx, c_bombs / 10);
    DrawLed(hdc, x0 + dx * 2, c_bombs % 10);

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

fn DrawTime(hdc: &w::HDC) {
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
    let dx_led = scale_dpi(DX_LED_96);
    DrawLed(
        hdc,
        dx_window - (scale_dpi(DX_RIGHT_TIME_96) + 3 * dx_led + border),
        time / 100,
    );
    time %= 100;
    DrawLed(
        hdc,
        dx_window - (scale_dpi(DX_RIGHT_TIME_96) + 2 * dx_led + border),
        time / 10,
    );
    DrawLed(
        hdc,
        dx_window - (scale_dpi(DX_RIGHT_TIME_96) + dx_led + border),
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

fn DrawButton(hdc: &w::HDC, sprite: ButtonSprite) {
    // The face button is cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dst_w = scale_dpi(DX_BUTTON_96);
    let dst_h = scale_dpi(DY_BUTTON_96);
    let x = (dx_window - dst_w) / 2;

    let state = match grafix_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let idx = sprite as usize;
    if idx >= BUTTON_SPRITE_COUNT {
        return;
    }

    let Some(src) = state.mem_button_dc[idx].as_ref() else {
        return;
    };

    let _ = hdc.BitBlt(
        w::POINT::with(x, scale_dpi(DY_TOP_LED_96)),
        w::SIZE::with(dst_w, dst_h),
        src,
        w::POINT::new(),
        ROP::SRCCOPY,
    );
}

/// Create a resampled bitmap using area averaging to avoid aliasing artifacts when using fractional scaling.
/// This function reads the source bitmap bits, performs area averaging, and creates a new bitmap with the resampled data.
/// # Arguments
/// * `hdc` - The device context used for bitmap operations.
/// * `src_bmp` - The source bitmap to be resampled.
/// * `src_w` - The width of the source bitmap in pixels.
/// * `src_h` - The height of the source bitmap in pixels.
/// * `dst_w` - The desired width of the destination bitmap in pixels.
/// * `dst_h` - The desired height of the destination bitmap in pixels.
/// # Returns
/// A `SysResult` containing a guard for the newly created resampled bitmap.
fn create_resampled_bitmap(
    hdc: &w::HDC,
    src_bmp: &w::HBITMAP,
    src_w: i32,
    src_h: i32,
    dst_w: i32,
    dst_h: i32,
) -> w::SysResult<w::guard::DeleteObjectGuard<w::HBITMAP>> {
    // 1. Prepare BITMAPINFO
    let mut bmi_header = w::BITMAPINFOHEADER::default();
    bmi_header.biWidth = src_w;
    bmi_header.biHeight = -src_h;
    bmi_header.biPlanes = 1;
    bmi_header.biBitCount = 32;
    bmi_header.biCompression = w::co::BI::RGB;
    let mut bmi = w::BITMAPINFO {
        bmiHeader: bmi_header,
        bmiColors: [w::RGBQUAD::default(); 1],
    };

    // 2. Read Source Bits
    let mut src_buf = vec![0u8; (src_w * src_h * 4) as usize];
    unsafe {
        hdc.GetDIBits(
            src_bmp,
            0,
            src_h as u32,
            Some(&mut src_buf),
            &mut bmi,
            w::co::DIB::RGB_COLORS,
        )
    }?;

    // 3. Prepare Destination
    let mut dst_buf = vec![0u8; (dst_w * dst_h * 4) as usize];

    // Scaling factors (Destination / Source) -> How many dst pixels per src pixel?
    // Actually, we usually want (Source / Destination) -> How much source does one dst pixel cover?
    let scale_x = dst_w as f32 / src_w as f32;
    let scale_y = dst_h as f32 / src_h as f32;

    // Helper to read source pixel safely
    let get_src = |cx: i32, cy: i32| -> (f32, f32, f32) {
        if cx < 0 || cx >= src_w || cy < 0 || cy >= src_h {
            return (0.0, 0.0, 0.0);
        }
        let idx = ((cy * src_w + cx) * 4) as usize;
        (
            src_buf[idx + 2] as f32, // R
            src_buf[idx + 1] as f32, // G
            src_buf[idx] as f32,     // B
        )
    };

    // 4. Area Averaging Loop
    for y in 0..dst_h {
        // Calculate the range of source pixels this destination pixel covers
        let v_min = y as f32 / scale_y;
        let v_max = (y + 1) as f32 / scale_y;

        for x in 0..dst_w {
            let u_min = x as f32 / scale_x;
            let u_max = (x + 1) as f32 / scale_x;

            let mut r_acc = 0.0;
            let mut g_acc = 0.0;
            let mut b_acc = 0.0;
            let mut weight_acc = 0.0;

            // Loop over the source pixels covered by this destination pixel
            // We use .floor() and .ceil() to hit every integer grid cell involved
            let start_y = v_min.floor() as i32;
            let end_y = v_max.ceil() as i32;
            let start_x = u_min.floor() as i32;
            let end_x = u_max.ceil() as i32;

            for iy in start_y..end_y {
                for ix in start_x..end_x {
                    // Calculate weighting (area of overlap)
                    // The overlap is the intersection of [iy, iy+1] and [v_min, v_max]
                    let dy = (iy as f32 + 1.0).min(v_max) - (iy as f32).max(v_min);
                    let dx = (ix as f32 + 1.0).min(u_max) - (ix as f32).max(u_min);

                    let weight = dx * dy;

                    if weight > 0.0 {
                        let (r, g, b) = get_src(ix, iy);
                        r_acc += r * weight;
                        g_acc += g * weight;
                        b_acc += b * weight;
                        weight_acc += weight;
                    }
                }
            }

            // Normalize
            if weight_acc > 0.0 {
                r_acc /= weight_acc;
                g_acc /= weight_acc;
                b_acc /= weight_acc;
            }

            let dst_idx = ((y * dst_w + x) * 4) as usize;
            dst_buf[dst_idx] = b_acc as u8; // B
            dst_buf[dst_idx + 1] = g_acc as u8; // G
            dst_buf[dst_idx + 2] = r_acc as u8; // R
            dst_buf[dst_idx + 3] = 0; // Alpha
        }
    }

    // 5. Create and Set Bitmap
    let dst_bmp = hdc.CreateCompatibleBitmap(dst_w, dst_h)?;
    bmi.bmiHeader.biWidth = dst_w;
    bmi.bmiHeader.biHeight = -dst_h;

    hdc.SetDIBits(
        &dst_bmp,
        0,
        dst_h as u32,
        &dst_buf,
        &bmi,
        w::co::DIB::RGB_COLORS,
    )?;

    Ok(dst_bmp)
}

pub fn DisplayButton(sprite: ButtonSprite) {
    if let Some(hwnd) = main_window()
        && let Ok(hdc) = hwnd.GetDC()
    {
        DrawButton(&hdc, sprite);
    }
}

fn SetThePen(hdc: &w::HDC, f_normal: i32) {
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

fn DrawBorder(
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

fn DrawBackground(hdc: &w::HDC) {
    // Repaint every chrome element (outer frame, counters, smiley bezel) before drawing content.
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Relaxed);
    let border = CXBORDER.load(Relaxed);
    let mut x = dx_window - 1;
    let mut y = dy_window - 1;
    let b3 = scale_dpi(3);
    let b2 = scale_dpi(2);
    let b1 = scale_dpi(1);
    DrawBorder(hdc, 0, 0, x, y, b3, 1);

    x -= scale_dpi(DX_RIGHT_SPACE_96) - b3;
    y -= scale_dpi(DY_BOTTOM_SPACE_96) - b3;
    DrawBorder(
        hdc,
        scale_dpi(DX_LEFT_SPACE_96) - b3,
        scale_dpi(DY_GRID_OFF_96) - b3,
        x,
        y,
        b3,
        0,
    );
    DrawBorder(
        hdc,
        scale_dpi(DX_LEFT_SPACE_96) - b3,
        scale_dpi(DY_TOP_SPACE_96) - b3,
        x,
        scale_dpi(DY_TOP_LED_96)
            + scale_dpi(DY_LED_96)
            + (scale_dpi(DY_BOTTOM_SPACE_96) - scale_dpi(6)),
        b2,
        0,
    );

    let x_left_bomb = scale_dpi(DX_LEFT_BOMB_96);
    let dx_led = scale_dpi(DX_LED_96);
    x = x_left_bomb + dx_led * 3;
    y = scale_dpi(DY_TOP_LED_96) + scale_dpi(DY_LED_96);
    DrawBorder(
        hdc,
        x_left_bomb - b1,
        scale_dpi(DY_TOP_LED_96) - b1,
        x,
        y,
        b1,
        0,
    );

    x = dx_window - (scale_dpi(DX_RIGHT_TIME_96) + 3 * dx_led + border + b1);
    DrawBorder(
        hdc,
        x,
        scale_dpi(DY_TOP_LED_96) - b1,
        x + (dx_led * 3 + b1),
        y,
        b1,
        0,
    );

    let dx_button = scale_dpi(DX_BUTTON_96);
    let dy_button = scale_dpi(DY_BUTTON_96);
    x = ((dx_window - dx_button) / 2) - b1;
    DrawBorder(
        hdc,
        x,
        scale_dpi(DY_TOP_LED_96) - b1,
        x + dx_button + b1,
        scale_dpi(DY_TOP_LED_96) + dy_button,
        b1,
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

    let cb_blk = cb_bitmap(color_on, DX_BLK_96, DY_BLK_96);
    for (i, off) in state.rg_dib_off.iter_mut().enumerate() {
        *off = header + i * cb_blk;
    }

    let cb_led = cb_bitmap(color_on, DX_LED_96, DY_LED_96);
    for (i, off) in state.rg_dib_led_off.iter_mut().enumerate() {
        *off = header + i * cb_led;
    }

    let cb_button = cb_bitmap(color_on, DX_BUTTON_96, DY_BUTTON_96);
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
    //
    // For fractional DPI scaling, simple StretchBlt produced unpleasant artifacts.
    // We therefore create the classic 96-DPI bitmap first, then resample it using
    // `create_resampled_bitmap` into a cached, DPI-sized bitmap.
    let dst_blk_w = scale_dpi(DX_BLK_96);
    let dst_blk_h = scale_dpi(DY_BLK_96);
    for i in 0..I_BLK_MAX {
        let dc_guard = match hdc.CreateCompatibleDC() {
            Ok(dc_guard) => dc_guard,
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_DC) {
                    w::OutputDebugString(msg);
                }
                state.mem_blk_dc[i] = None;
                state.mem_blk_bitmap[i] = None;
                continue;
            }
        };

        let base_bmp = match hdc.CreateCompatibleBitmap(DX_BLK_96, DY_BLK_96) {
            Ok(bmp_guard) => bmp_guard,
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_BITMAP) {
                    w::OutputDebugString(msg);
                }
                state.mem_blk_dc[i] = None;
                state.mem_blk_bitmap[i] = None;
                continue;
            }
        };

        // Paint the sprite into the 96-DPI bitmap.
        {
            let bmp_h = unsafe { w::HBITMAP::from_ptr(base_bmp.ptr()) };
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&bmp_h) {
                let _ = sel_guard.leak();
            }
            unsafe {
                SetDIBitsToDevice(
                    dc_guard.ptr(),
                    0,
                    0,
                    DX_BLK_96 as u32,
                    DY_BLK_96 as u32,
                    0,
                    0,
                    0,
                    DY_BLK_96 as u32,
                    state
                        .lp_dib_blks
                        .byte_add(state.rg_dib_off[i] as usize)
                        .cast(),
                    state.lp_dib_blks as *const _,
                    DIB::RGB_COLORS.raw(),
                );
            }
        }

        let final_bmp = if dst_blk_w != DX_BLK_96 || dst_blk_h != DY_BLK_96 {
            match create_resampled_bitmap(
                &hdc, &base_bmp, DX_BLK_96, DY_BLK_96, dst_blk_w, dst_blk_h,
            ) {
                Ok(resampled) => resampled,
                Err(_) => base_bmp,
            }
        } else {
            base_bmp
        };

        // Ensure the DC holds the final bitmap.
        let bmp_h = unsafe { w::HBITMAP::from_ptr(final_bmp.ptr()) };
        if let Ok(mut sel_guard) = dc_guard.SelectObject(&bmp_h) {
            let _ = sel_guard.leak();
        }

        state.mem_blk_dc[i] = Some(dc_guard);
        state.mem_blk_bitmap[i] = Some(final_bmp);
    }

    // Cache LED digits in compatible bitmaps.
    for i in 0..I_LED_MAX {
        state.mem_led_dc[i] = match hdc.CreateCompatibleDC() {
            Ok(dc_guard) => Some(dc_guard),
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_DC) {
                    w::OutputDebugString(msg);
                }
                None
            }
        };

        state.mem_led_bitmap[i] = match hdc.CreateCompatibleBitmap(DX_LED_96, DY_LED_96) {
            Ok(bmp_guard) => Some(bmp_guard),
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_BITMAP) {
                    w::OutputDebugString(msg);
                }
                None
            }
        };

        if state.mem_led_dc[i].is_some()
            && state.mem_led_bitmap[i].is_some()
            && let Some(dc_guard) = state.mem_led_dc[i].as_ref()
            && let Some(bmp_guard) = state.mem_led_bitmap[i].as_ref()
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
                    DX_LED_96 as u32,
                    DY_LED_96 as u32,
                    0,
                    0,
                    0,
                    DY_LED_96 as u32,
                    state
                        .lp_dib_led
                        .byte_add(state.rg_dib_led_off[i] as usize)
                        .cast(),
                    state.lp_dib_led as *const _,
                    DIB::RGB_COLORS.raw(),
                );
            }
        }
    }

    // Cache face button sprites in compatible bitmaps.
    //
    // Like the blocks, the face button looks best when we resample once and cache.
    let dst_btn_w = scale_dpi(DX_BUTTON_96);
    let dst_btn_h = scale_dpi(DY_BUTTON_96);
    for i in 0..BUTTON_SPRITE_COUNT {
        let dc_guard = match hdc.CreateCompatibleDC() {
            Ok(dc_guard) => dc_guard,
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_DC) {
                    w::OutputDebugString(msg);
                }
                state.mem_button_dc[i] = None;
                state.mem_button_bitmap[i] = None;
                continue;
            }
        };

        let base_bmp = match hdc.CreateCompatibleBitmap(DX_BUTTON_96, DY_BUTTON_96) {
            Ok(bmp_guard) => bmp_guard,
            Err(_) => {
                if let Ok(msg) = core::str::from_utf8(DEBUG_CREATE_BITMAP) {
                    w::OutputDebugString(msg);
                }
                state.mem_button_dc[i] = None;
                state.mem_button_bitmap[i] = None;
                continue;
            }
        };

        // Paint the sprite into the 96-DPI bitmap.
        {
            let bmp_h = unsafe { w::HBITMAP::from_ptr(base_bmp.ptr()) };
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&bmp_h) {
                let _ = sel_guard.leak();
            }
            unsafe {
                SetDIBitsToDevice(
                    dc_guard.ptr(),
                    0,
                    0,
                    DX_BUTTON_96 as u32,
                    DY_BUTTON_96 as u32,
                    0,
                    0,
                    0,
                    DY_BUTTON_96 as u32,
                    state
                        .lp_dib_button
                        .byte_add(state.rg_dib_button_off[i] as usize)
                        .cast(),
                    state.lp_dib_button as *const _,
                    DIB::RGB_COLORS.raw(),
                );
            }
        }

        let final_bmp = if dst_btn_w != DX_BUTTON_96 || dst_btn_h != DY_BUTTON_96 {
            match create_resampled_bitmap(
                &hdc,
                &base_bmp,
                DX_BUTTON_96,
                DY_BUTTON_96,
                dst_btn_w,
                dst_btn_h,
            ) {
                Ok(resampled) => resampled,
                Err(_) => base_bmp,
            }
        } else {
            base_bmp
        };

        // Ensure the DC holds the final bitmap.
        let bmp_h = unsafe { w::HBITMAP::from_ptr(final_bmp.ptr()) };
        if let Ok(mut sel_guard) = dc_guard.SelectObject(&bmp_h) {
            let _ = sel_guard.leak();
        }

        state.mem_button_dc[i] = Some(dc_guard);
        state.mem_button_bitmap[i] = Some(final_bmp);
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
    let board = board_mutex();
    board
        .get(idx)
        .copied()
        .map(|value| (value & BlockMask::Data as i8) as usize)
        .unwrap_or(0)
}

const fn rgb(r: u8, g: u8, b: u8) -> w::COLORREF {
    w::COLORREF::from_rgb(r, g, b)
}
