//! Graphics handling for the Minesweeper game, including bitmap loading,
//! scaling, and rendering of game elements.

use core::mem::size_of;
use core::ptr::null;
use core::sync::atomic::Ordering::Relaxed;
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows_sys::Win32::Graphics::Gdi::{GDI_ERROR, GetLayout, SetDIBitsToDevice, SetLayout};
use winsafe::{
    self as w, AnyResult, BITMAPINFO, BITMAPINFOHEADER, COLORREF, HBITMAP, HDC, HINSTANCE, HPEN,
    HRSRCMEM, HWND, IdStr, POINT, RGBQUAD, RtStr, SIZE,
    co::{BI, DIB, LAYOUT, PS, ROP, RT, STRETCH_MODE},
    guard::{DeleteDCGuard, DeleteObjectGuard, ReleaseDCGuard},
    prelude::*,
};

use crate::globals::{BASE_DPI, UI_DPI, WINDOW_HEIGHT, WINDOW_WIDTH};
use crate::rtns::{BOARD_INDEX_SHIFT, BlockMask, GameState, preferences_mutex};

/*
    Constants defining pixel dimensions and offsets for various UI elements at 96 DPI.
    These are scaled at runtime to match the current UI DPI.
*/
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
    /// Cached gray pen used for drawing borders
    h_gray_pen: Option<DeleteObjectGuard<HPEN>>,
    /// Cached white pen used for drawing borders
    h_white_pen: Option<DeleteObjectGuard<HPEN>>,
    /// Cached compatible DCs for each block sprite
    mem_blk_dc: [Option<DeleteDCGuard>; I_BLK_MAX],
    /// Cached compatible bitmaps for each block sprite
    mem_blk_bitmap: [Option<DeleteObjectGuard<HBITMAP>>; I_BLK_MAX],
    /// Cached compatible DCs for each LED digit
    mem_led_dc: [Option<DeleteDCGuard>; I_LED_MAX],
    /// Cached compatible bitmaps for each LED digit
    mem_led_bitmap: [Option<DeleteObjectGuard<HBITMAP>>; I_LED_MAX],
    /// Cached compatible DCs for each face button sprite
    mem_button_dc: [Option<DeleteDCGuard>; BUTTON_SPRITE_COUNT],
    /// Cached compatible bitmaps for each face button sprite
    mem_button_bitmap: [Option<DeleteObjectGuard<HBITMAP>>; BUTTON_SPRITE_COUNT],
}

// TODO: Eliminate use of *const BITMAPINFO. This is a lot of work and requires unsafe code elsewhere, but it's better practice.
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
            h_gray_pen: None,
            h_white_pen: None,
            mem_blk_dc: [const { None }; I_BLK_MAX],
            mem_blk_bitmap: [const { None }; I_BLK_MAX],
            mem_led_dc: [const { None }; I_LED_MAX],
            mem_led_bitmap: [const { None }; I_LED_MAX],
            mem_button_dc: [const { None }; BUTTON_SPRITE_COUNT],
            mem_button_bitmap: [const { None }; BUTTON_SPRITE_COUNT],
        }
    }
}

/// Shared variable containing the graphics state
static GRAFIX_STATE: OnceLock<Mutex<GrafixState>> = OnceLock::new();

/// Accessor for the shared graphics state
/// # Returns
/// Reference to the Mutex protecting the `GrafixState`
fn grafix_state() -> MutexGuard<'static, GrafixState> {
    match GRAFIX_STATE
        .get_or_init(|| Mutex::new(GrafixState::default()))
        .lock()
    {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Draw a single block at the specified board coordinates.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `x` - The X coordinate of the block (1-based).
/// * `y` - The Y coordinate of the block (1-based).
/// * `board` - Slice representing the board state.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn draw_block(hdc: &ReleaseDCGuard, x: i32, y: i32, board: &[i8]) -> AnyResult<()> {
    let state = grafix_state();
    let Some(src) = block_dc(&state, x, y, board) else {
        return Ok(());
    };

    let dst_w = scale_dpi(DX_BLK_96);
    let dst_h = scale_dpi(DY_BLK_96);
    let dst_x = (x * dst_w) + (scale_dpi(DX_LEFT_SPACE_96) - dst_w);
    let dst_y = (y * dst_h) + (scale_dpi(DY_GRID_OFF_96) - dst_h);

    // Blocks are cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
    hdc.BitBlt(
        POINT::with(dst_x, dst_y),
        SIZE::with(dst_w, dst_h),
        src,
        POINT::new(),
        ROP::SRCCOPY,
    )?;
    Ok(())
}

/// Draw the entire minefield grid onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `width` - The width of the board in blocks.
/// * `height` - The height of the board in blocks.
/// * `board` - Slice representing the board state.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn draw_grid(hdc: &ReleaseDCGuard, width: i32, height: i32, board: &[i8]) -> AnyResult<()> {
    let state = grafix_state();
    let dst_w = scale_dpi(DX_BLK_96);
    let dst_h = scale_dpi(DY_BLK_96);

    let mut dy = scale_dpi(DY_GRID_OFF_96);
    for y in 1..=height {
        let mut dx = scale_dpi(DX_LEFT_SPACE_96);
        for x in 1..=width {
            if let Some(src) = block_dc(&state, x, y, board) {
                hdc.BitBlt(
                    POINT::with(dx, dy),
                    SIZE::with(dst_w, dst_h),
                    src,
                    POINT::new(),
                    ROP::SRCCOPY,
                )?;
            }
            dx += dst_w;
        }
        dy += dst_h;
    }
    Ok(())
}

/// LED digit sprites used in the bomb counter and timer.
#[repr(u8)]
enum LEDSprite {
    /// Digit 0
    Zero = 0,
    /// Digit 1
    One = 1,
    /// Digit 2
    Two = 2,
    /// Digit 3
    Three = 3,
    /// Digit 4
    Four = 4,
    /// Digit 5
    Five = 5,
    /// Digit 6
    Six = 6,
    /// Digit 7
    Seven = 7,
    /// Digit 8
    Eight = 8,
    /// Digit 9
    Nine = 9,
    /// No digit (blank)
    Blank = 10,
    /// Negative sign
    Negative = 11,
}

impl From<u16> for LEDSprite {
    /// Create an `LEDSprite` from a `u16` value.
    /// # Arguments
    /// * `value` - The `u16` value to convert.
    fn from(value: u16) -> Self {
        match value.into() {
            0 => LEDSprite::Zero,
            1 => LEDSprite::One,
            2 => LEDSprite::Two,
            3 => LEDSprite::Three,
            4 => LEDSprite::Four,
            5 => LEDSprite::Five,
            6 => LEDSprite::Six,
            7 => LEDSprite::Seven,
            8 => LEDSprite::Eight,
            9 => LEDSprite::Nine,
            10 => LEDSprite::Blank,
            11 => LEDSprite::Negative,
            _ => LEDSprite::Blank,
        }
    }
}
impl From<i16> for LEDSprite {
    /// Create an `LEDSprite` from an `i16` value.
    /// # Arguments
    /// * `value` - The `i16` value to convert.
    fn from(value: i16) -> Self {
        LEDSprite::from(value.unsigned_abs())
    }
}

/// Draw a single LED digit at the specified X coordinate.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `x` - The X coordinate to draw the LED digit.
/// * `led_index` - The index of the LED digit to draw.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
fn draw_led(hdc: &HDC, x: i32, led_index: LEDSprite) -> AnyResult<()> {
    // LEDs are cached into compatible bitmaps so we can scale them with StretchBlt.
    let state = grafix_state();
    let Some(src) = state.mem_led_dc[led_index as usize].as_ref() else {
        return Ok(());
    };

    hdc.SetStretchBltMode(STRETCH_MODE::COLORONCOLOR)?;
    hdc.StretchBlt(
        POINT::with(x, scale_dpi(DY_TOP_LED_96)),
        SIZE::with(scale_dpi(DX_LED_96), scale_dpi(DY_LED_96)),
        src,
        POINT::new(),
        SIZE::with(DX_LED_96, DY_LED_96),
        ROP::SRCCOPY,
    )?;
    Ok(())
}

/// Draw the bomb counter onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `bombs` - The number of bombs left to display.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn draw_bomb_count(hdc: &ReleaseDCGuard, bombs: i16) -> AnyResult<()> {
    // Handle when the window is mirrored for RTL languages by temporarily disabling mirroring
    let layout = unsafe { GetLayout(hdc.ptr()) };
    // If the previous command succeeded and the RTL bit is set, the system is set to RTL mode
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), 0);
        }
    }

    // Draw each of the three digits in sequence
    let x0 = scale_dpi(DX_LEFT_BOMB_96);
    let dx = scale_dpi(DX_LED_96);
    // Hundreds place or negative sign
    draw_led(
        hdc,
        x0,
        LEDSprite::from(u16::try_from(bombs).map_or(11, |b| b / 100)),
    )?;
    // Tens place
    draw_led(hdc, x0 + dx, LEDSprite::from((bombs % 100) / 10))?;
    // Ones place
    draw_led(hdc, x0 + dx * 2, LEDSprite::from(bombs % 10))?;

    // Restore the original layout if it was mirrored
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), layout);
        }
    }
    Ok(())
}

/// Draw the timer onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `time` - The time in seconds to display.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn draw_timer(hdc: &ReleaseDCGuard, time: u16) -> AnyResult<()> {
    // The timer uses the same mirroring trick as the bomb counter.
    let layout = unsafe { GetLayout(hdc.ptr()) };
    let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), 0);
        }
    }

    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dx_led = scale_dpi(DX_LED_96);
    let dx_led_right = scale_dpi(DX_RIGHT_TIME_96);
    // Hundreds place
    draw_led(
        hdc,
        dx_window - (dx_led_right + 3 * dx_led),
        LEDSprite::from(time / 100),
    )?;
    // Tens place
    draw_led(
        hdc,
        dx_window - (dx_led_right + 2 * dx_led),
        LEDSprite::from((time % 100) / 10),
    )?;
    // Ones place
    draw_led(
        hdc,
        dx_window - (dx_led_right + dx_led),
        LEDSprite::from(time % 10),
    )?;

    if mirrored {
        unsafe {
            SetLayout(hdc.ptr(), layout);
        }
    }
    Ok(())
}

/// Draw the face button onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `sprite` - The button sprite to draw.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
fn draw_button(hdc: &HDC, sprite: ButtonSprite) -> AnyResult<()> {
    // The face button is cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dst_w = scale_dpi(DX_BUTTON_96);
    let dst_h = scale_dpi(DY_BUTTON_96);
    let x = (dx_window - dst_w) / 2;

    let idx = sprite as usize;
    if idx >= BUTTON_SPRITE_COUNT {
        return Ok(());
    }

    let state = grafix_state();
    let Some(src) = state.mem_button_dc[idx].as_ref() else {
        return Ok(());
    };

    hdc.BitBlt(
        POINT::with(x, scale_dpi(DY_TOP_LED_96)),
        SIZE::with(dst_w, dst_h),
        src,
        POINT::new(),
        ROP::SRCCOPY,
    )?;

    Ok(())
}

/// Create a resampled bitmap using area averaging to avoid aliasing artifacts when using fractional scaling.
/// This function reads the source bitmap bits, performs area averaging, and creates a new bitmap with the resampled data.
///
/// TODO: Why are the width and height parameters i32 instead of u32?
/// # Arguments
/// * `hdc` - The device context used for bitmap operations.
/// * `src_bmp` - The source bitmap to be resampled.
/// * `src_w` - The width of the source bitmap in pixels.
/// * `src_h` - The height of the source bitmap in pixels.
/// * `dst_w` - The desired width of the destination bitmap in pixels.
/// * `dst_h` - The desired height of the destination bitmap in pixels.
/// # Returns
/// An `AnyResult` containing a guard for the newly created resampled bitmap.
fn create_resampled_bitmap(
    hdc: &HDC,
    src_bmp: &HBITMAP,
    src_w: i32,
    src_h: i32,
    dst_w: i32,
    dst_h: i32,
) -> AnyResult<DeleteObjectGuard<HBITMAP>> {
    // 1. Prepare BITMAPINFO
    let mut bmi_header = BITMAPINFOHEADER::default();
    bmi_header.biWidth = src_w;
    bmi_header.biHeight = -src_h;
    bmi_header.biPlanes = 1;
    bmi_header.biBitCount = 32;
    bmi_header.biCompression = BI::RGB;
    let mut bmi = BITMAPINFO {
        bmiHeader: bmi_header,
        bmiColors: [RGBQUAD::default(); 1],
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
            DIB::RGB_COLORS,
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
            f32::from(src_buf[idx + 2]), // R
            f32::from(src_buf[idx + 1]), // G
            f32::from(src_buf[idx]),     // B
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

    hdc.SetDIBits(&dst_bmp, 0, dst_h as u32, &dst_buf, &bmi, DIB::RGB_COLORS)?;

    Ok(dst_bmp)
}

/// Display the face button with the specified sprite.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `sprite` - The button sprite to display.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn display_button(hwnd: &HWND, sprite: ButtonSprite) -> AnyResult<()> {
    hwnd.GetDC()
        .map_or_else(|e| Err(e.into()), |hdc| draw_button(&hdc, sprite))
}

/// Border styles for drawing beveled borders.
#[derive(Copy, Clone, Eq, PartialEq)]
enum BorderStyle {
    /// Raised beveled border.
    Raised,
    /// Sunken beveled border.
    Sunken,
    /// Flat border (no bevel).
    Flat,
}

impl BorderStyle {
    /// Set the pen for drawing based on the normal flag.
    /// # Arguments
    /// * `hdc` - The device context to set the pen on.
    /// * `f_normal` - The normal flag determining the pen style.
    /// # Returns
    /// `Ok(())` if successful, or an error if setting the pen failed.
    fn set_border_pen(self, hdc: &HDC) -> AnyResult<()> {
        // Select the appropriate pen based on the border style
        if self == BorderStyle::Sunken {
            // Use cached white pen for sunken borders
            if let Some(ref white_pen) = grafix_state().h_white_pen {
                // Note: This somehow does not cause a resource leak
                hdc.SelectObject(&**white_pen)
                    .map(|mut guard| guard.leak())?;
            }
        } else {
            // Use cached gray pen for raised and flat borders
            if let Some(ref gray_pen) = grafix_state().h_gray_pen {
                // Note: This somehow does not cause a resource leak
                hdc.SelectObject(&**gray_pen)
                    .map(|mut guard| guard.leak())?;
            }
        }
        Ok(())
    }
}

/// Draw a beveled border rectangle onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `x1` - The left X coordinate of the rectangle.
/// * `y1` - The top Y coordinate of the rectangle.
/// * `x2` - The right X coordinate of the rectangle.
/// * `y2` - The bottom Y coordinate of the rectangle.
/// * `width` - The width of the border in pixels.
/// * `border_style` - The border style determining the border appearance.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
fn draw_border(
    hdc: &HDC,
    mut x1: i32,
    mut y1: i32,
    mut x2: i32,
    mut y2: i32,
    width: i32,
    border_style: BorderStyle,
) -> AnyResult<()> {
    let mut i = 0;
    // Set the initial pen style based on given border style
    border_style.set_border_pen(hdc)?;

    // Draw the top and left edges
    while i < width {
        y2 -= 1;
        hdc.MoveToEx(x1, y2, None)?;
        hdc.LineTo(x1, y1)?;
        x1 += 1;
        hdc.LineTo(x2, y1)?;
        x2 -= 1;
        y1 += 1;
        i += 1;
    }

    // Switch pen style for bottom and right edges if not flat
    if border_style != BorderStyle::Flat {
        if border_style == BorderStyle::Sunken {
            BorderStyle::Raised
        } else {
            BorderStyle::Sunken
        }
        .set_border_pen(hdc)?;
    }

    // Draw the bottom and right edges
    while i > 0 {
        y2 += 1;
        hdc.MoveToEx(x1, y2, None)?;
        x1 -= 1;
        x2 += 1;
        hdc.LineTo(x2, y2)?;
        y1 -= 1;
        hdc.LineTo(x2, y1)?;
        i -= 1;
    }
    Ok(())
}

/// Draw the entire window background and chrome elements onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
fn draw_background(hdc: &HDC) -> AnyResult<()> {
    let dx_window = WINDOW_WIDTH.load(Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Relaxed);
    // Outer sunken border
    let mut x = dx_window - 1;
    let mut y = dy_window - 1;
    let b3 = scale_dpi(3);
    let b2 = scale_dpi(2);
    let b1 = scale_dpi(1);
    draw_border(hdc, 0, 0, x, y, b3, BorderStyle::Sunken)?;

    // Inner raised borders
    x -= scale_dpi(DX_RIGHT_SPACE_96) - b3;
    y -= scale_dpi(DY_BOTTOM_SPACE_96) - b3;
    draw_border(
        hdc,
        scale_dpi(DX_LEFT_SPACE_96) - b3,
        scale_dpi(DY_GRID_OFF_96) - b3,
        x,
        y,
        b3,
        BorderStyle::Raised,
    )?;
    // LED area border
    draw_border(
        hdc,
        scale_dpi(DX_LEFT_SPACE_96) - b3,
        scale_dpi(DY_TOP_SPACE_96) - b3,
        x,
        scale_dpi(DY_TOP_LED_96)
            + scale_dpi(DY_LED_96)
            + (scale_dpi(DY_BOTTOM_SPACE_96) - scale_dpi(6)),
        b2,
        BorderStyle::Raised,
    )?;

    // LED borders
    let x_left_bomb = scale_dpi(DX_LEFT_BOMB_96);
    let dx_led = scale_dpi(DX_LED_96);
    x = x_left_bomb + dx_led * 3;
    y = scale_dpi(DY_TOP_LED_96) + scale_dpi(DY_LED_96);
    draw_border(
        hdc,
        x_left_bomb - b1,
        scale_dpi(DY_TOP_LED_96) - b1,
        x,
        y,
        b1,
        BorderStyle::Raised,
    )?;

    // Timer borders
    x = dx_window - (scale_dpi(DX_RIGHT_TIME_96) + 3 * dx_led + b1);
    draw_border(
        hdc,
        x,
        scale_dpi(DY_TOP_LED_96) - b1,
        x + (dx_led * 3 + b1),
        y,
        b1,
        BorderStyle::Raised,
    )?;

    // Button border
    let dx_button = scale_dpi(DX_BUTTON_96);
    let dy_button = scale_dpi(DY_BUTTON_96);
    x = ((dx_window - dx_button) / 2) - b1;
    draw_border(
        hdc,
        x,
        scale_dpi(DY_TOP_LED_96) - b1,
        x + dx_button + b1,
        scale_dpi(DY_TOP_LED_96) + dy_button,
        b1,
        BorderStyle::Flat,
    )?;
    Ok(())
}

/// Draw the entire screen (background, counters, button, timer, grid) onto the provided device context.
/// # Arguments
/// * `hdc` - The device context to draw on.
/// * `state` - The current game state containing board and UI information.
/// # Returns
/// `Ok(())` if successful, or an error if drawing failed.
pub fn draw_screen(hdc: &ReleaseDCGuard, state: &GameState) -> AnyResult<()> {
    // 1. Draw background and borders
    draw_background(hdc)?;
    // 2. Draw bomb counter
    draw_bomb_count(hdc, state.bombs_left)?;
    // 3. Draw face button
    draw_button(hdc, state.btn_face_state)?;
    // 4. Draw timer
    draw_timer(hdc, state.secs_elapsed)?;
    // 5. Draw minefield grid
    draw_grid(
        hdc,
        state.board_width,
        state.board_height,
        &state.board_cells,
    )?;

    Ok(())
}

/// Load the bitmap resources and prepare cached DCs for rendering.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// # Returns
/// Ok(()) if successful, or an error if loading resources failed.
pub fn load_bitmaps(hwnd: &HWND) -> AnyResult<()> {
    let color_on = { preferences_mutex().color };
    let mut state = grafix_state();

    let Some((h_blks, lp_blks)) =
        load_bitmap_resource(&hwnd.hinstance(), BitmapId::Blocks, color_on)
    else {
        return Err("Failed to load block bitmap resource".into());
    };
    let Some((h_led, lp_led)) = load_bitmap_resource(&hwnd.hinstance(), BitmapId::Led, color_on)
    else {
        return Err("Failed to load LED bitmap resource".into());
    };
    let Some((h_button, lp_button)) =
        load_bitmap_resource(&hwnd.hinstance(), BitmapId::Button, color_on)
    else {
        return Err("Failed to load button bitmap resource".into());
    };

    state.h_res_blks = h_blks;
    state.h_res_led = h_led;
    state.h_res_button = h_button;

    state.lp_dib_blks = lp_blks;
    state.lp_dib_led = lp_led;
    state.lp_dib_button = lp_button;

    state.h_gray_pen = if color_on {
        HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(128, 128, 128)).ok()
    } else {
        HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(0, 0, 0)).ok()
    };

    if state.h_gray_pen.is_none() {
        return Err("Failed to create gray pen".into());
    }

    state.h_white_pen = HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(255, 255, 255)).ok();

    if state.h_white_pen.is_none() {
        return Err("Failed to get white pen".into());
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

    let hdc = match hwnd.GetDC() {
        Ok(dc) => dc,
        Err(e) => return Err(format!("Failed to get device context: {e}").into()),
    };

    // Build a dedicated compatible DC + bitmap for every block sprite to speed up drawing.
    //
    // For fractional DPI scaling, simple StretchBlt produced unpleasant artifacts.
    // We therefore create the classic 96-DPI bitmap first, then resample it using
    // `create_resampled_bitmap` into a cached, DPI-sized bitmap.
    let dst_blk_w = scale_dpi(DX_BLK_96);
    let dst_blk_h = scale_dpi(DY_BLK_96);
    for i in 0..I_BLK_MAX {
        let dc_guard = hdc.CreateCompatibleDC()?;

        let base_bmp = hdc.CreateCompatibleBitmap(DX_BLK_96, DY_BLK_96)?;

        // Paint the sprite into the 96-DPI bitmap.
        {
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&*base_bmp) {
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
            create_resampled_bitmap(&hdc, &base_bmp, DX_BLK_96, DY_BLK_96, dst_blk_w, dst_blk_h)
                .unwrap_or(base_bmp)
        } else {
            base_bmp
        };

        // Ensure the DC holds the final bitmap.
        if let Ok(mut sel_guard) = dc_guard.SelectObject(&*final_bmp) {
            let _ = sel_guard.leak();
        }

        state.mem_blk_dc[i] = Some(dc_guard);
        state.mem_blk_bitmap[i] = Some(final_bmp);
    }

    // Cache LED digits in compatible bitmaps.
    for i in 0..I_LED_MAX {
        state.mem_led_dc[i] = Some(hdc.CreateCompatibleDC()?);

        state.mem_led_bitmap[i] = Some(hdc.CreateCompatibleBitmap(DX_LED_96, DY_LED_96)?);

        if state.mem_led_dc[i].is_some()
            && state.mem_led_bitmap[i].is_some()
            && let Some(dc_guard) = state.mem_led_dc[i].as_ref()
            && let Some(bmp_guard) = state.mem_led_bitmap[i].as_ref()
        {
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&**bmp_guard) {
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
        let dc_guard = hdc.CreateCompatibleDC()?;

        let base_bmp = hdc.CreateCompatibleBitmap(DX_BUTTON_96, DY_BUTTON_96)?;

        // Paint the sprite into the 96-DPI bitmap.
        {
            if let Ok(mut sel_guard) = dc_guard.SelectObject(&*base_bmp) {
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
            create_resampled_bitmap(
                &hdc,
                &base_bmp,
                DX_BUTTON_96,
                DY_BUTTON_96,
                dst_btn_w,
                dst_btn_h,
            )
            .unwrap_or(base_bmp)
        } else {
            base_bmp
        };

        // Ensure the DC holds the final bitmap.
        if let Ok(mut sel_guard) = dc_guard.SelectObject(&*final_bmp) {
            let _ = sel_guard.leak();
        }

        state.mem_button_dc[i] = Some(dc_guard);
        state.mem_button_bitmap[i] = Some(final_bmp);
    }

    Ok(())
}

/// Load a bitmap resource from the application resources.
/// # Arguments
/// * `id` - The bitmap resource ID to load.
/// * `color_on` - Whether color mode is enabled.
/// # Returns
/// Optionally, a tuple containing the resource handle and a pointer to the bitmap data.
fn load_bitmap_resource(
    hinst: &HINSTANCE,
    id: BitmapId,
    color_on: bool,
) -> Option<(HRSRCMEM, *const BITMAPINFO)> {
    let offset = if color_on { 0 } else { 1 };
    let resource_id = (id as u16) + offset;
    // Colorless devices load the grayscale resource IDs immediately following the color ones.
    let res_info = hinst
        .FindResource(IdStr::Id(resource_id), RtStr::Rt(RT::BITMAP))
        .ok()?;
    let res_loaded = hinst.LoadResource(&res_info).ok()?;
    let lp = hinst.LockResource(&res_info, &res_loaded).ok()?.as_ptr();
    // The cast to `BITMAPINFO` should be safe because `LockResource` returns a pointer to the first byte of the resource data,
    // which is structured as a `BITMAPINFO` according to the resource format.
    Some((res_loaded, lp as *const BITMAPINFO))
}

/// Calculate the size of the DIB header plus color palette
/// # Arguments
/// * `color_on` - Whether color mode is enabled
/// # Returns
/// Size in bytes of the DIB header and palette
const fn dib_header_size(color_on: bool) -> usize {
    let palette_entries = if color_on { 16 } else { 2 };
    size_of::<BITMAPINFOHEADER>() + palette_entries * 4
}

/// Calculate the byte size of a bitmap given its dimensions and color mode
/// # Arguments
/// * `color_on` - Whether color mode is enabled
/// * `x` - Width of the bitmap in pixels
/// * `y` - Height of the bitmap in pixels
/// # Returns
/// Size in bytes of the bitmap data
const fn cb_bitmap(color_on: bool, x: i32, y: i32) -> usize {
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
/// * `state` - Reference to the current `GrafixState`
/// * `x` - X coordinate on the board
/// * `y` - Y coordinate on the board
/// * `board` - Slice representing the board state
/// # Returns
/// Optionally, a reference to the compatible DC for the block sprite
fn block_dc<'a>(state: &'a GrafixState, x: i32, y: i32, board: &[i8]) -> Option<&'a DeleteDCGuard> {
    let idx = block_sprite_index(x, y, board);
    if idx >= I_BLK_MAX {
        return None;
    }

    state.mem_blk_dc[idx].as_ref()
}

/// Determine the sprite index for the block at the given board coordinates.
/// # Arguments
/// * `x` - X coordinate on the board
/// * `y` - Y coordinate on the board
/// * `board` - Slice representing the board state
/// # Returns
/// The sprite index for the block at the specified coordinates
/// # Notes
/// The x and y values are stored as `i32` due to much of the Win32 API using `i32` for coordinates. (`POINT`, `SIZE`, `RECT`, etc.)
fn block_sprite_index(x: i32, y: i32, board: &[i8]) -> usize {
    // The board encoding packs state into rgBlk; mask out metadata to find the sprite index.
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    if offset < 0 {
        return 0;
    }
    let idx = offset as usize;
    board
        .get(idx)
        .copied()
        .map_or(0, |value| (value & BlockMask::Data as i8) as usize)
}
