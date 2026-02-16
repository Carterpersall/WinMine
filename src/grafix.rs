//! Graphics handling for the Minesweeper game, including bitmap loading,
//! scaling, and rendering of game elements.

use core::mem::size_of;
use core::ptr::null;

use windows_sys::Win32::Graphics::Gdi::{GDI_ERROR, GetLayout, SetDIBitsToDevice, SetLayout};

use winsafe::co::{BI, DIB, LAYOUT, PS, ROP, RT, STRETCH_MODE};
use winsafe::guard::{DeleteDCGuard, DeleteObjectGuard, ReleaseDCGuard};
use winsafe::{
    AnyResult, BITMAPINFO, BITMAPINFOHEADER, COLORREF, HBITMAP, HDC, HINSTANCE, HPEN, HRSRCMEM,
    HWND, IdStr, POINT, RGBQUAD, RtStr, SIZE, prelude::*,
};

use crate::globals::BASE_DPI;
use crate::rtns::{BlockInfo, GameState, MAX_X_BLKS, MAX_Y_BLKS};
use crate::util::{ResourceId, scale_dpi};

/*
    Constants defining pixel dimensions and offsets for various UI elements at 96 DPI.
    These are scaled at runtime to match the current UI DPI.
*/
/// Width of a single board cell sprite in pixels.
const DX_BLK_96: i32 = 16;
/// Height of a single board cell sprite in pixels.
const DY_BLK_96: i32 = 16;
/// Width of an LED digit in pixels.
const DX_LED_96: i32 = 13;
/// Height of an LED digit in pixels.
const DY_LED_96: i32 = 23;
/// Width of the face button sprite in pixels.
const DX_BUTTON_96: i32 = 24;
/// Height of the face button sprite in pixels.
const DY_BUTTON_96: i32 = 24;
/// Left margin between the window frame and the board.
const DX_LEFT_SPACE_96: i32 = 12;
/// Right margin between the window frame and the board.
const DX_RIGHT_SPACE_96: i32 = 12;
/// Top margin above the LED row.
const DY_TOP_SPACE_96: i32 = 12;
/// Bottom margin below the grid.
const DY_BOTTOM_SPACE_96: i32 = 12;
// Note: Adding the offsets cause the DPI scaling to have minor rounding errors at specific DPIs.
// However, all common DPIs (100%, 125%, 150%, 175%, 200%) produce correct results.
/// Vertical offset to the LED row.
const DY_TOP_LED_96: i32 = DY_TOP_SPACE_96 + 4;
/// Vertical offset to the top of the grid.
const DY_GRID_OFF_96: i32 = DY_TOP_LED_96 + DY_LED_96 + 16;
/// X coordinate of the left edge of the bomb counter.
const DX_LEFT_BOMB_96: i32 = DX_LEFT_SPACE_96 + 5;
/// X coordinate offset from the right edge for the timer counter.
const DX_RIGHT_TIME_96: i32 = DX_RIGHT_SPACE_96 + 5;

/// Current UI dimensions and offsets, scaled from the base 96-DPI values.
#[derive(Default)]
pub struct WindowDimensions {
    /// Dimensions of a single board cell sprite.
    pub block: SIZE,
    /// Dimensions of an LED digit sprite.
    pub led: SIZE,
    /// Dimensions of the face button sprite.
    pub button: SIZE,
    /// Left margin between the window frame and the board.
    pub left_space: i32,
    /// Right margin between the window frame and the board.
    pub right_space: i32,
    /// Top margin above the LED row.
    pub top_space: i32,
    /// Bottom margin below the grid.
    pub bottom_space: i32,
    /// Vertical offset to the LED row.
    pub top_led: i32,
    /// Vertical offset to the top of the grid.
    pub grid_offset: i32,
    /// Offset to the left edge of the bomb counter.
    pub left_bomb: i32,
    /// Offset from the right edge for the timer counter.
    pub right_timer: i32,
}

impl WindowDimensions {
    /// Update the dimensions based on the current DPI by scaling the base 96-DPI values.
    /// # Arguments
    /// - `dpi` - The current UI DPI to scale the dimensions for.
    pub const fn update_dpi(&mut self, dpi: u32) {
        self.block.cx = scale_dpi(DX_BLK_96, dpi);
        self.block.cy = scale_dpi(DY_BLK_96, dpi);
        self.led.cx = scale_dpi(DX_LED_96, dpi);
        self.led.cy = scale_dpi(DY_LED_96, dpi);
        self.button.cx = scale_dpi(DX_BUTTON_96, dpi);
        self.button.cy = scale_dpi(DY_BUTTON_96, dpi);
        self.left_space = scale_dpi(DX_LEFT_SPACE_96, dpi);
        self.right_space = scale_dpi(DX_RIGHT_SPACE_96, dpi);
        self.top_space = scale_dpi(DY_TOP_SPACE_96, dpi);
        self.bottom_space = scale_dpi(DY_BOTTOM_SPACE_96, dpi);
        self.top_led = scale_dpi(DY_TOP_LED_96, dpi);
        self.grid_offset = scale_dpi(DY_GRID_OFF_96, dpi);
        self.left_bomb = scale_dpi(DX_LEFT_BOMB_96, dpi);
        self.right_timer = scale_dpi(DX_RIGHT_TIME_96, dpi);
    }
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
    /// - `value` - The `u16` value to convert.
    fn from(value: u16) -> Self {
        match value {
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
    /// - `value` - The `i16` value to convert.
    fn from(value: i16) -> Self {
        LEDSprite::from(value.unsigned_abs())
    }
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

/// Guard for a cached bitmap resource.
///
/// When an instance of this struct is dropped, it automatically restores the previous bitmap into the DC
/// using `SelectObject`, and then deletes the cached bitmap resource. This ensures that GDI resources are
/// managed correctly and prevents leaks.
struct CachedBitmapGuard {
    /// Guard for the compatible DC with the bitmap selected into it
    dc: DeleteDCGuard,
    /// Guard for the cached bitmap resource
    bitmap: DeleteObjectGuard<HBITMAP>,
    /// The previous bitmap that was selected into the DC before the cached bitmap, which will be restored on drop
    prev_bitmap: HBITMAP,
}

impl CachedBitmapGuard {
    /// Create a new `CachedBitmapGuard` by selecting the provided bitmap into the provided DC.
    /// # Arguments
    /// - `dc` - The compatible DC to select the bitmap into.
    /// - `bitmap` - The bitmap to select into the DC.
    /// # Returns
    /// - `Ok(CachedBitmapGuard)` - A new `CachedBitmapGuard` instance
    /// - `Err` - If selecting the bitmap into the DC fails
    fn new(dc: DeleteDCGuard, bitmap: DeleteObjectGuard<HBITMAP>) -> AnyResult<Self> {
        let prev_bitmap = {
            // Select the cached bitmap into the DC
            let mut guard = dc.SelectObject(&*bitmap)?;
            // Leak the guard to keep the bitmap selected until this struct is dropped
            guard.leak()
        };
        Ok(Self {
            dc,
            bitmap,
            prev_bitmap,
        })
    }

    /// Get a reference to the DC with the cached bitmap selected into it.
    /// # Returns
    /// - A reference to the DC that can be used for drawing operations.
    fn hdc(&self) -> &HDC {
        &self.dc
    }
}

impl Drop for CachedBitmapGuard {
    /// When the `CachedBitmapGuard` is dropped, restore the previous bitmap into the DC,
    /// and allow the bitmap and DC guards to clean up their resources.
    fn drop(&mut self) {
        // Bring the bitmap into scope, which ensures it will be dropped at the end of this function
        let _ = self.bitmap.as_opt();
        if let Ok(mut guard) = self.dc.SelectObject(&self.prev_bitmap) {
            let _ = guard.leak();
        }
    }
}

/// Internal state tracking loaded graphics resources and cached DCs
pub struct GrafixState {
    /// Current UI DPI
    ///
    /// TODO: Should this be moved into `WindowDimensions`?
    pub dpi: u32,
    /// Current window position
    pub wnd_pos: POINT,
    /// Current UI dimensions and offsets, scaled from the base 96-DPI values.
    pub dims: WindowDimensions,
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
    /// Cached compatible DCs/bitmaps for each block sprite
    mem_blk_cache: [Option<CachedBitmapGuard>; I_BLK_MAX],
    /// Cached compatible DCs/bitmaps for each LED digit
    mem_led_cache: [Option<CachedBitmapGuard>; I_LED_MAX],
    /// Cached compatible DCs/bitmaps for each face button sprite
    mem_button_cache: [Option<CachedBitmapGuard>; BUTTON_SPRITE_COUNT],
}

impl Default for GrafixState {
    fn default() -> Self {
        Self {
            dpi: BASE_DPI,
            wnd_pos: POINT::new(),
            dims: WindowDimensions::default(),
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
            mem_blk_cache: [const { None }; I_BLK_MAX],
            mem_led_cache: [const { None }; I_LED_MAX],
            mem_button_cache: [const { None }; BUTTON_SPRITE_COUNT],
        }
    }
}

impl GrafixState {
    /// Draw a single block at the specified board coordinates.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `x` - The X coordinate of the block.
    /// - `y` - The Y coordinate of the block.
    /// - `board` - Array slice containing the board state.
    /// # Returns
    /// - `Ok(())` - If the block was drawn successfully.
    /// - `Err` - If drawing the block failed.
    pub fn draw_block(
        &self,
        hdc: &ReleaseDCGuard,
        x: usize,
        y: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> AnyResult<()> {
        let Some(src) = self.block_dc(x, y, board) else {
            return Ok(());
        };

        let dst_w = self.dims.block.cx;
        let dst_h = self.dims.block.cy;
        let dst_x = (x as i32 * dst_w) + (self.dims.left_space - dst_w);
        let dst_y = (y as i32 * dst_h) + (self.dims.grid_offset - dst_h);

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
    /// - `hdc` - The device context to draw on.
    /// - `width` - The width of the board in blocks.
    /// - `height` - The height of the board in blocks.
    /// - `board` - Array slice containing the board state.
    /// # Returns
    /// - `Ok(())` - If the grid was drawn successfully.
    /// - `Err` - If `BitBlt` failed for any block.
    pub fn draw_grid(
        &self,
        hdc: &HDC,
        width: usize,
        height: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> AnyResult<()> {
        let dst_w = self.dims.block.cx;
        let dst_h = self.dims.block.cy;

        let mut dy = self.dims.grid_offset;
        for y in 1..=height {
            let mut dx = self.dims.left_space;
            for x in 1..=width {
                if let Some(src) = self.block_dc(x, y, board) {
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

    /// Draw a single LED digit at the specified X coordinate.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `x` - The X coordinate to draw the LED digit.
    /// - `led_index` - The index of the LED digit to draw.
    /// # Returns
    /// - `Ok(())` - If the LED digit was drawn successfully.
    /// - `Err` - If drawing the LED digit failed.
    fn draw_led(&self, hdc: &HDC, x: i32, led_index: LEDSprite) -> AnyResult<()> {
        // LEDs are cached into compatible bitmaps so we can scale them with StretchBlt.
        let Some(src) = self
            .mem_led_cache
            .get(led_index as usize)
            .and_then(|cache| cache.as_ref())
        else {
            return Ok(());
        };

        hdc.SetStretchBltMode(STRETCH_MODE::COLORONCOLOR)?;
        hdc.StretchBlt(
            POINT::with(x, self.dims.top_led),
            self.dims.led,
            src.hdc(),
            POINT::new(),
            SIZE::with(DX_LED_96, DY_LED_96),
            ROP::SRCCOPY,
        )?;
        Ok(())
    }

    /// Draw the bomb counter onto the provided device context.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `bombs` - The number of bombs left to display.
    /// # Returns
    /// - `Ok(())` - If the bomb count was drawn successfully.
    /// - `Err` - If drawing the bomb count LEDs failed.
    pub fn draw_bomb_count(&self, hdc: &HDC, bombs: i16) -> AnyResult<()> {
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
        let x0 = self.dims.left_bomb;
        let dx = self.dims.led.cx;
        // Hundreds place or negative sign
        self.draw_led(
            hdc,
            x0,
            LEDSprite::from(u16::try_from(bombs).map_or(11, |b| b / 100)),
        )?;
        // Tens place
        self.draw_led(hdc, x0 + dx, LEDSprite::from((bombs % 100) / 10))?;
        // Ones place
        self.draw_led(hdc, x0 + dx * 2, LEDSprite::from(bombs % 10))?;

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
    /// - `hdc` - The device context to draw on.
    /// - `time` - The time in seconds to display.
    /// # Returns
    /// - `Ok(())` - If the timer was drawn successfully.
    /// - `Err` - If drawing the timer LEDs failed.
    pub fn draw_timer(&self, hdc: &HDC, time: u16) -> AnyResult<()> {
        // The timer uses the same mirroring trick as the bomb counter.
        let layout = unsafe { GetLayout(hdc.ptr()) };
        let mirrored = layout != GDI_ERROR as u32 && (layout & LAYOUT::RTL.raw()) != 0;
        if mirrored {
            unsafe {
                SetLayout(hdc.ptr(), 0);
            }
        }

        let dx_window = self.wnd_pos.x;
        let dx_led = self.dims.led.cx;
        let dx_led_right = self.dims.right_timer;
        // Hundreds place
        self.draw_led(
            hdc,
            dx_window - (dx_led_right + 3 * dx_led),
            LEDSprite::from(time / 100),
        )?;
        // Tens place
        self.draw_led(
            hdc,
            dx_window - (dx_led_right + 2 * dx_led),
            LEDSprite::from((time % 100) / 10),
        )?;
        // Ones place
        self.draw_led(
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
    /// - `hdc` - The device context to draw on.
    /// - `sprite` - The button sprite to draw.
    /// # Returns
    /// - `Ok(())` - If the face button was drawn successfully.
    /// - `Err` - If drawing the face button failed.
    pub fn draw_button(&self, hdc: &HDC, sprite: ButtonSprite) -> AnyResult<()> {
        // The face button is cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
        let dx_window = self.wnd_pos.x;
        let dst_w = self.dims.button.cx;
        let dst_h = self.dims.button.cy;
        let x = (dx_window - dst_w) / 2;

        let idx = sprite as usize;
        if idx >= BUTTON_SPRITE_COUNT {
            return Ok(());
        }

        let Some(src) = self.mem_button_cache[idx].as_ref() else {
            return Ok(());
        };

        hdc.BitBlt(
            POINT::with(x, self.dims.top_led),
            SIZE::with(dst_w, dst_h),
            src.hdc(),
            POINT::new(),
            ROP::SRCCOPY,
        )?;

        Ok(())
    }
}

/// Create a resampled bitmap using area averaging to avoid aliasing artifacts when using fractional scaling.
/// This function reads the source bitmap bits, performs area averaging, and creates a new bitmap with the resampled data.
/// # Arguments
/// - `hdc` - The device context used for bitmap operations.
/// - `src_bmp` - The source bitmap to be resampled.
/// - `src_w` - The width of the source bitmap in pixels.
/// - `src_h` - The height of the source bitmap in pixels.
/// - `dst_w` - The desired width of the destination bitmap in pixels.
/// - `dst_h` - The desired height of the destination bitmap in pixels.
/// # Returns
/// - `Ok(DeleteObjectGuard<HBITMAP>)` - A guard containing the newly created resampled bitmap, which will be automatically deleted when dropped.
/// - `Err` - If any step of the bitmap creation or resampling process fails.
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

impl GrafixState {
    /// Set the pen for drawing based on the normal flag.
    /// # Arguments
    /// - `hdc` - The device context to set the pen on.
    /// - `border_style` - The border style determining the pen to use.
    /// # Returns
    /// - `Ok(())` - If the pen was successfully selected
    /// - `Err` - If selecting the pen into the device context failed or if the required pen is not initialized
    fn select_border_pen(&self, hdc: &HDC, border_style: BorderStyle) -> AnyResult<()> {
        // Select the appropriate pen based on the border style
        let pen = if border_style == BorderStyle::Sunken {
            // Use cached white pen for sunken borders
            self.h_white_pen
                .as_ref()
                .ok_or("White pen is not initialized")?
        } else {
            // Use cached gray pen for raised and flat borders
            self.h_gray_pen
                .as_ref()
                .ok_or("Gray pen is not initialized")?
        };

        // Note: This does not leak since both pens are stored in `GrafixState`
        // TODO: Could the `leak()` be avoided?
        hdc.SelectObject(&**pen).map(|mut guard| guard.leak())?;
        Ok(())
    }

    /// Draw a beveled border rectangle onto the provided device context.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `x1` - The left X coordinate of the rectangle.
    /// - `y1` - The top Y coordinate of the rectangle.
    /// - `x2` - The right X coordinate of the rectangle.
    /// - `y2` - The bottom Y coordinate of the rectangle.
    /// - `width` - The width of the border in pixels.
    /// - `border_style` - The border style determining the border appearance.
    /// # Returns
    /// - `Ok(())` - If the border was drawn successfully
    /// - `Err` - If drawing the border failed
    fn draw_border(
        &self,
        hdc: &HDC,
        mut point1: POINT,
        mut point2: POINT,
        width: i32,
        border_style: BorderStyle,
    ) -> AnyResult<()> {
        let mut i = 0;
        // Set the initial pen based on the border style
        self.select_border_pen(hdc, border_style)?;

        // Draw the top and left edges
        while i < width {
            point2.y -= 1;
            hdc.MoveToEx(point1.x, point2.y, None)?;
            hdc.LineTo(point1.x, point1.y)?;
            point1.x += 1;
            hdc.LineTo(point2.x, point1.y)?;
            point2.x -= 1;
            point1.y += 1;
            i += 1;
        }

        // Switch pen style for bottom and right edges if not flat
        if border_style != BorderStyle::Flat {
            if border_style == BorderStyle::Sunken {
                self.select_border_pen(hdc, BorderStyle::Raised)?;
            } else {
                self.select_border_pen(hdc, BorderStyle::Sunken)?;
            };
        }

        // Draw the bottom and right edges
        while i > 0 {
            point2.y += 1;
            hdc.MoveToEx(point1.x, point2.y, None)?;
            point1.x -= 1;
            point2.x += 1;
            hdc.LineTo(point2.x, point2.y)?;
            point1.y -= 1;
            hdc.LineTo(point2.x, point1.y)?;
            i -= 1;
        }
        Ok(())
    }

    /// Draw the entire window background and chrome elements onto the provided device context.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// # Returns
    /// - `Ok(())` - If the background was drawn successfully
    /// - `Err` - If drawing any of the background borders failed
    fn draw_background(&self, hdc: &HDC) -> AnyResult<()> {
        let dx_window = self.wnd_pos.x;
        let dy_window = self.wnd_pos.y;

        // Scale the border widths based on the current DPI
        let b3 = scale_dpi(3, self.dpi);
        let b2 = scale_dpi(2, self.dpi);
        let b1 = scale_dpi(1, self.dpi);

        // Outer sunken border
        let mut x = dx_window - 1;
        let mut y = dy_window - 1;
        self.draw_border(
            hdc,
            POINT::with(0, 0),
            POINT::with(x, y),
            b3,
            BorderStyle::Sunken,
        )?;

        // Inner raised borders
        x -= self.dims.right_space - b3;
        y -= self.dims.bottom_space - b3;
        self.draw_border(
            hdc,
            POINT::with(self.dims.left_space - b3, self.dims.grid_offset - b3),
            POINT::with(x, y),
            b3,
            BorderStyle::Raised,
        )?;

        // LED area border
        self.draw_border(
            hdc,
            POINT::with(self.dims.left_space - b3, self.dims.top_space - b3),
            POINT::with(
                x,
                self.dims.top_led
                    + self.dims.led.cy
                    + (self.dims.bottom_space - scale_dpi(6, self.dpi)),
            ),
            b2,
            BorderStyle::Raised,
        )?;

        // LED borders
        let x_left_bomb = self.dims.left_bomb;
        let dx_led = self.dims.led.cx;
        x = x_left_bomb + dx_led * 3;
        y = self.dims.top_led + self.dims.led.cy;
        self.draw_border(
            hdc,
            POINT::with(x_left_bomb - b1, self.dims.top_led - b1),
            POINT::with(x, y),
            b1,
            BorderStyle::Raised,
        )?;

        // Timer borders
        x = dx_window - (self.dims.right_timer + 3 * dx_led + b1);
        self.draw_border(
            hdc,
            POINT::with(x, self.dims.top_led - b1),
            POINT::with(x + (dx_led * 3 + b1), y),
            b1,
            BorderStyle::Raised,
        )?;

        // Button border
        let dx_button = self.dims.button.cx;
        let dy_button = self.dims.button.cy;
        x = ((dx_window - dx_button) / 2) - b1;
        self.draw_border(
            hdc,
            POINT::with(x, self.dims.top_led - b1),
            POINT::with(x + dx_button + b1, self.dims.top_led + dy_button),
            b1,
            BorderStyle::Flat,
        )?;
        Ok(())
    }

    /// Draw the entire screen (background, counters, button, timer, grid) onto the provided device context.
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `state` - The current game state containing board and UI information.
    /// # Returns
    /// - `Ok(())` - If the screen was drawn successfully
    /// - `Err` - If drawing any of the screen elements failed
    pub fn draw_screen(&self, hdc: &HDC, state: &GameState) -> AnyResult<()> {
        // 1. Draw background and borders
        self.draw_background(hdc)?;
        // 2. Draw bomb counter
        self.draw_bomb_count(hdc, state.bombs_left)?;
        // 3. Draw face button
        self.draw_button(hdc, state.btn_face_state)?;
        // 4. Draw timer
        self.draw_timer(hdc, state.secs_elapsed)?;
        // 5. Draw minefield grid
        self.draw_grid(
            hdc,
            state.board_width,
            state.board_height,
            &state.board_cells,
        )?;

        Ok(())
    }

    /// Load the bitmap resources and prepare cached DCs for rendering.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// - `color` - Whether to load color or monochrome resources.
    /// # Returns
    /// - `Ok(())` - If the bitmaps were loaded and cached successfully
    /// - `Err` - If loading any of the bitmap resources or creating cached DCs failed
    pub fn load_bitmaps(&mut self, hwnd: &HWND, color: bool) -> AnyResult<()> {
        let (h_blks, lp_blks) =
            self.load_bitmap_resource(&hwnd.hinstance(), ResourceId::BlocksBmp, color)?;
        let (h_led, lp_led) =
            self.load_bitmap_resource(&hwnd.hinstance(), ResourceId::LedBmp, color)?;
        let (h_button, lp_button) =
            self.load_bitmap_resource(&hwnd.hinstance(), ResourceId::ButtonBmp, color)?;

        self.h_res_blks = h_blks;
        self.h_res_led = h_led;
        self.h_res_button = h_button;

        self.lp_dib_blks = lp_blks;
        self.lp_dib_led = lp_led;
        self.lp_dib_button = lp_button;

        self.h_gray_pen = if color {
            HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(128, 128, 128))?.into()
        } else {
            HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(0, 0, 0))?.into()
        };

        self.h_white_pen = HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(255, 255, 255))?.into();

        let header = self.dib_header_size(color);

        let cb_blk = self.cb_bitmap(color, DX_BLK_96, DY_BLK_96);
        for (i, off) in self.rg_dib_off.iter_mut().enumerate() {
            *off = header + i * cb_blk;
        }

        let cb_led = self.cb_bitmap(color, DX_LED_96, DY_LED_96);
        for (i, off) in self.rg_dib_led_off.iter_mut().enumerate() {
            *off = header + i * cb_led;
        }

        let cb_button = self.cb_bitmap(color, DX_BUTTON_96, DY_BUTTON_96);
        for (i, off) in self.rg_dib_button_off.iter_mut().enumerate() {
            *off = header + i * cb_button;
        }

        let hdc = hwnd.GetDC()?;

        // Build a dedicated compatible DC + bitmap for every block sprite to speed up drawing.
        //
        // For fractional DPI scaling, simple StretchBlt produced unpleasant artifacts.
        // We therefore create the classic 96-DPI bitmap first, then resample it using
        // `create_resampled_bitmap` into a cached, DPI-sized bitmap.
        let dst_blk_w = self.dims.block.cx;
        let dst_blk_h = self.dims.block.cy;
        for i in 0..I_BLK_MAX {
            let dc_guard = hdc.CreateCompatibleDC()?;

            let base_bmp = hdc.CreateCompatibleBitmap(DX_BLK_96, DY_BLK_96)?;

            // Paint the sprite into the 96-DPI bitmap.
            {
                let _sel_guard = dc_guard.SelectObject(&*base_bmp)?;
                let scan_lines = unsafe {
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
                        self.lp_dib_blks.byte_add(self.rg_dib_off[i]).cast(),
                        self.lp_dib_blks as *const _,
                        DIB::RGB_COLORS.raw(),
                    )
                };
                if scan_lines == 0 {
                    return Err("Failed to paint block bitmap".into());
                }
            }

            let final_bmp = if dst_blk_w != DX_BLK_96 || dst_blk_h != DY_BLK_96 {
                create_resampled_bitmap(&hdc, &base_bmp, DX_BLK_96, DY_BLK_96, dst_blk_w, dst_blk_h)
                    .unwrap_or(base_bmp)
            } else {
                base_bmp
            };

            self.mem_blk_cache[i] = Some(CachedBitmapGuard::new(dc_guard, final_bmp)?);
        }

        // Cache LED digits in compatible bitmaps.
        for i in 0..I_LED_MAX {
            let dc_guard = hdc.CreateCompatibleDC()?;
            let bmp_guard = hdc.CreateCompatibleBitmap(DX_LED_96, DY_LED_96)?;
            {
                let _sel_guard = dc_guard.SelectObject(&*bmp_guard)?;
                let scan_lines = unsafe {
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
                        self.lp_dib_led.byte_add(self.rg_dib_led_off[i]).cast(),
                        self.lp_dib_led as *const _,
                        DIB::RGB_COLORS.raw(),
                    )
                };
                if scan_lines == 0 {
                    return Err("Failed to paint LED bitmap".into());
                }
            }
            self.mem_led_cache[i] = Some(CachedBitmapGuard::new(dc_guard, bmp_guard)?);
        }

        // Cache face button sprites in compatible bitmaps.
        //
        // Like the blocks, the face button looks best when we resample once and cache.
        let dst_btn_w = self.dims.button.cx;
        let dst_btn_h = self.dims.button.cy;
        for i in 0..BUTTON_SPRITE_COUNT {
            let dc_guard = hdc.CreateCompatibleDC()?;

            let base_bmp = hdc.CreateCompatibleBitmap(DX_BUTTON_96, DY_BUTTON_96)?;

            // Paint the sprite into the 96-DPI bitmap.
            {
                let _sel_guard = dc_guard.SelectObject(&*base_bmp)?;
                let scan_lines = unsafe {
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
                        self.lp_dib_button
                            .byte_add(self.rg_dib_button_off[i])
                            .cast(),
                        self.lp_dib_button as *const _,
                        DIB::RGB_COLORS.raw(),
                    )
                };
                if scan_lines == 0 {
                    return Err("Failed to paint button bitmap".into());
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

            self.mem_button_cache[i] = Some(CachedBitmapGuard::new(dc_guard, final_bmp)?);
        }

        Ok(())
    }

    /// Load a bitmap resource from the application resources.
    /// # Arguments
    /// - `id` - The bitmap resource ID to load.
    /// - `color_on` - Whether color mode is enabled.
    /// # Returns
    /// - `Some((HRSRCMEM, *const BITMAPINFO))` - The resource handle and a pointer to the bitmap data if successful.
    /// - `None` - If loading the resource failed at any step (finding, loading, locking).
    fn load_bitmap_resource(
        &self,
        hinst: &HINSTANCE,
        id: ResourceId,
        color_on: bool,
    ) -> AnyResult<(HRSRCMEM, *const BITMAPINFO)> {
        let offset = if color_on { 0 } else { 1 };
        let resource_id = (id as u16) + offset;
        // Colorless devices load the grayscale resource IDs immediately following the color ones.
        let res_info = hinst.FindResource(IdStr::Id(resource_id), RtStr::Rt(RT::BITMAP))?;
        let res_loaded = hinst.LoadResource(&res_info)?;
        let lp = hinst.LockResource(&res_info, &res_loaded)?.as_ptr();
        // The cast to `BITMAPINFO` should be safe because `LockResource` returns a pointer to the first byte of the resource data,
        // which is structured as a `BITMAPINFO` according to the resource format.
        Ok((res_loaded, lp as *const BITMAPINFO))
    }

    /// Calculate the size of the DIB header plus color palette
    /// # Arguments
    /// - `color_on` - Whether color mode is enabled
    /// # Returns
    /// - Size in bytes of the DIB header and palette
    const fn dib_header_size(&self, color_on: bool) -> usize {
        let palette_entries = if color_on { 16 } else { 2 };
        size_of::<BITMAPINFOHEADER>() + palette_entries * 4
    }

    /// Calculate the byte size of a bitmap given its dimensions and color mode
    /// # Arguments
    /// - `color_on` - Whether color mode is enabled
    /// - `x` - Width of the bitmap in pixels
    /// - `y` - Height of the bitmap in pixels
    /// # Returns
    /// - Size in bytes of the bitmap data
    const fn cb_bitmap(&self, color_on: bool, x: i32, y: i32) -> usize {
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
    /// - `x` - X coordinate on the board
    /// - `y` - Y coordinate on the board
    /// - `board` - Slice representing the board state
    /// # Returns
    /// - `Some(&HDC)` - The compatible DC containing the block sprite
    /// - `None` - If the block type is out of range or if the cached DC is not available
    fn block_dc(
        &self,
        x: usize,
        y: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> Option<&HDC> {
        let idx = self.block_sprite_index(x, y, board);
        if idx >= I_BLK_MAX {
            return None;
        }

        self.mem_blk_cache[idx].as_ref().map(CachedBitmapGuard::hdc)
    }

    /// Determine the sprite index for the block at the given board coordinates.
    /// # Arguments
    /// - `x` - X coordinate on the board
    /// - `y` - Y coordinate on the board
    /// - `board` - Array slice containing the board state
    /// # Returns
    /// - The sprite index for the block at the specified coordinates
    const fn block_sprite_index(
        &self,
        x: usize,
        y: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> usize {
        board[x][y].block_type as usize
    }
}
