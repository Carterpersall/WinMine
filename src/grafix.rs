//! Graphics handling for the Minesweeper game, including bitmap loading,
//! scaling, and rendering of game elements.

use core::ops::Index;

use strum_macros::VariantArray;
use windows_sys::Win32::Graphics::Gdi::{GDI_ERROR, GetLayout, SetLayout};

use winsafe::co::{BI, DIB, LAYOUT, PS, ROP, STRETCH_MODE};
use winsafe::guard::{DeleteDCGuard, DeleteObjectGuard, ReleaseDCGuard, SelectObjectGuard};
use winsafe::{
    AnyResult, BITMAPFILEHEADER, BITMAPINFO, BITMAPINFOHEADER, COLORREF, HBITMAP, HDC, HPEN, POINT,
    SIZE,
};

use crate::globals::BASE_DPI;
use crate::rtns::{BlockCell, BlockInfo, MAX_X_BLKS, MAX_Y_BLKS};
use crate::util::impl_index_enum;

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
pub(crate) struct WindowDimensions {
    /// Current UI DPI, used for scaling dimensions and offsets.
    pub dpi: u32,
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
    /// Scale a 96-DPI measurement to the current UI DPI
    /// # Arguments
    /// - `val` - The measurement in pixels at 96 DPI.
    /// # Returns
    /// - The measurement scaled to the current DPI.
    /// # Notes
    /// This function replicates the functionality of the `MulDiv` Win32 API function, with a few differences:
    /// - It takes a signed and an unsigned integer and returns a signed integer, while `MulDiv` operates on only signed integers.
    /// - It assumes that the denominator is always non-zero, which can be safely assumed in this context since `BASE_DPI` is a constant
    ///   and should never be zero.
    const fn scale_dpi(&self, val: i32) -> i32 {
        // Perform multiplication in u64 to prevent overflow
        let product = val as u64 * self.dpi as u64;
        // Perform division with rounding
        ((product + (BASE_DPI as u64 / 2)) / BASE_DPI as u64) as i32
    }

    /// Update the stored DPI and rescale all dimensions and offsets accordingly.
    /// # Arguments
    /// - `dpi` - The new UI DPI to apply.
    pub(crate) const fn update_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
        self.block.cx = self.scale_dpi(DX_BLK_96);
        self.block.cy = self.scale_dpi(DY_BLK_96);
        self.led.cx = self.scale_dpi(DX_LED_96);
        self.led.cy = self.scale_dpi(DY_LED_96);
        self.button.cx = self.scale_dpi(DX_BUTTON_96);
        self.button.cy = self.scale_dpi(DY_BUTTON_96);
        self.left_space = self.scale_dpi(DX_LEFT_SPACE_96);
        self.right_space = self.scale_dpi(DX_RIGHT_SPACE_96);
        self.top_space = self.scale_dpi(DY_TOP_SPACE_96);
        self.bottom_space = self.scale_dpi(DY_BOTTOM_SPACE_96);
        self.top_led = self.scale_dpi(DY_TOP_LED_96);
        self.grid_offset = self.scale_dpi(DY_GRID_OFF_96);
        self.left_bomb = self.scale_dpi(DX_LEFT_BOMB_96);
        self.right_timer = self.scale_dpi(DX_RIGHT_TIME_96);
    }
}

/// Number of cell sprites packed into the block bitmap sheet.
const I_BLK_MAX: usize = 16;

/// Number of digits stored in the LED bitmap sheet.
const I_LED_MAX: usize = 12;
/// Face button sprites available in the bitmap sheet.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq, VariantArray)]
pub(crate) enum ButtonSprite {
    Happy = 0,
    Caution = 1,
    Lose = 2,
    Win = 3,
    Down = 4,
}
/// Number of face button sprites.
const BUTTON_SPRITE_COUNT: usize = 5;

// Implement indexing for the button sprite cache array, allowing access by `ButtonSprite` enum variants.
impl_index_enum!(
    ButtonSprite,
    [Option<CachedBitmapGuard>; BUTTON_SPRITE_COUNT]
);

/// LED digit sprites used in the bomb counter and timer.
#[repr(u8)]
#[derive(Clone, Copy, VariantArray)]
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

// Implement indexing for the LED digit cache array, allowing access by `LEDSprite` enum variants.
impl_index_enum!(LEDSprite, [Option<CachedBitmapGuard>; I_LED_MAX]);

// Implement indexing for the block cell cache array, allowing access by `BlockCell` enum variants.
impl_index_enum!(BlockCell, [Option<CachedBitmapGuard>; 16]);

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
    fn new(dc: DeleteDCGuard, bitmap: &DeleteObjectGuard<HBITMAP>) -> AnyResult<Self> {
        let prev_bitmap = {
            // Select the cached bitmap into the DC
            let mut guard = dc.SelectObject(&**bitmap)?;
            // Leak the guard to keep the bitmap selected until this struct is dropped
            guard.leak()
        };
        Ok(Self { dc, prev_bitmap })
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
        // Restore the previous bitmap into the DC. Ignore errors
        self.dc.SelectObject(&self.prev_bitmap).ok();
    }
}

/// Internal state tracking loaded graphics resources and cached DCs
pub(crate) struct GrafixState {
    /// Current window position
    pub wnd_pos: POINT,
    /// Current UI dimensions and offsets, scaled from the base 96-DPI values.
    pub dims: WindowDimensions,
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
            wnd_pos: POINT::new(),
            dims: WindowDimensions::default(),
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
    pub(crate) fn draw_block(
        &self,
        hdc: &ReleaseDCGuard,
        x: usize,
        y: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> AnyResult<()> {
        let src = self.mem_blk_cache[board[x][y].block_type]
            .as_ref()
            .map(CachedBitmapGuard::hdc)
            .ok_or("Block bitmap not loaded")?;

        let dst_w = self.dims.block.cx;
        let dst_h = self.dims.block.cy;
        let dst_x = (x as i32 * dst_w) + self.dims.left_space;
        let dst_y = (y as i32 * dst_h) + self.dims.grid_offset;

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
    #[allow(clippy::needless_range_loop)]
    pub(crate) fn draw_grid(
        &self,
        hdc: &HDC,
        width: usize,
        height: usize,
        board: &[[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    ) -> AnyResult<()> {
        let dst_w = self.dims.block.cx;
        let dst_h = self.dims.block.cy;

        let mut dy = self.dims.grid_offset;
        for y in 0..=height {
            let mut dx = self.dims.left_space;
            for x in 0..=width {
                let src = self.mem_blk_cache[board[x][y].block_type]
                    .as_ref()
                    .map(CachedBitmapGuard::hdc)
                    .ok_or("Block bitmap not loaded")?;

                hdc.BitBlt(
                    POINT::with(dx, dy),
                    SIZE::with(dst_w, dst_h),
                    src,
                    POINT::new(),
                    ROP::SRCCOPY,
                )?;

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
        let src = self.mem_led_cache[led_index]
            .as_ref()
            .ok_or("LED bitmap not loaded")?;

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
    /// # Notes
    /// - This function calls `SetLayout` to temporarily disable mirroring when the system is in RTL mode,
    ///   since the bomb counter should always be left-aligned. It restores the original layout before returning.
    ///   However, if the function fails before restoring the layout, it may leave the DC in a non-mirrored state,
    ///   which could cause drawing issues. Any future error handling for this function should account for this.
    pub(crate) fn draw_bomb_count(&self, hdc: &HDC, bombs: i16) -> AnyResult<()> {
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
    /// # Notes
    /// - This function calls `SetLayout` to temporarily disable mirroring when the system is in RTL mode,
    ///   since the timer should always be left-aligned. It restores the original layout before returning.
    ///   However, if the function fails before restoring the layout, it may leave the DC in a non-mirrored state,
    ///   which could cause drawing issues. Any future error handling for this function should account for this.
    pub(crate) fn draw_timer(&self, hdc: &HDC, time: u16) -> AnyResult<()> {
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
    pub(crate) fn draw_button(&self, hdc: &HDC, sprite: ButtonSprite) -> AnyResult<()> {
        // The face button is cached pre-scaled (see `load_bitmaps_impl`) so we can do a 1:1 blit.
        let dx_window = self.wnd_pos.x;
        let dst_w = self.dims.button.cx;
        let dst_h = self.dims.button.cy;
        let x = (dx_window - dst_w) / 2;

        let src = self.mem_button_cache[sprite]
            .as_ref()
            .ok_or("Button bitmap not loaded")?;

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

/// Parsed data for a bitmap sprite sheet.
struct BmpSheet {
    /// The DIB data starting at the BITMAPINFOHEADER, which contains the header, color table (if present), and pixel data.
    dib: &'static [u8],
    /// Byte offset from the start of the DIB to the pixel data, which is used to locate the pixel data within the DIB slice.
    pixel_offset: usize,
    /// Number of bits per pixel for the bitmap, which is used to calculate the size of each sprite and the offsets to individual sprites within the DIB.
    bit_count: u16,
}

/// A single entry in a bitmap's color palette.
#[derive(Copy, Clone)]
struct PaletteEntry {
    /// The red component of the color.
    red: u8,
    /// The green component of the color.
    green: u8,
    /// The blue component of the color.
    blue: u8,
}

impl BmpSheet {
    /// Converts a bitmap file buffer into a DIB slice and metadata.
    /// # Arguments
    /// - `bmp` - A byte slice containing the entire bitmap file data, including the `BITMAPFILEHEADER`, `BITMAPINFOHEADER`, color table (if present), and pixel data.
    /// # Returns
    /// - A `BmpSheet` struct containing a slice of the DIB data starting at the `BITMAPINFOHEADER`, the byte offset to the pixel data within that slice, and the bits per pixel of the bitmap.
    /// # Panics
    /// - If the bitmap file is too small to contain the required headers.
    /// - If the bitmap file header does not start with the "BM" signature, indicating it is not a valid bitmap file.
    /// - If the file size specified in the header is larger than the actual buffer size.
    /// - If the pixel data offset specified in the header is out of bounds of the buffer.
    /// - If the DIB header size is smaller than the size of `BITMAPINFOHEADER`.
    /// - If the DIB header extends beyond the bounds of the DIB slice.
    /// - If the pixel data offset precedes the end of the DIB header, which would indicate an invalid bitmap structure.
    const fn from_bytes(bmp: &'static [u8]) -> Self {
        let file_header_len = size_of::<BITMAPFILEHEADER>();
        let info_header_len = size_of::<BITMAPINFOHEADER>();
        if bmp.len() < file_header_len + info_header_len {
            panic!("BMP file is too small to contain headers");
        }

        // Verify the BMP signature "BM" at the start of the file header, which indicates a valid BMP file
        if bmp[0] != b'B' || bmp[1] != b'M' {
            panic!("BMP file header is missing BM signature");
        }

        // Get the file size from the header
        let bf_size = u32::from_le_bytes([bmp[2], bmp[3], bmp[4], bmp[5]]) as usize;
        // Verify that the file size specified in the header is not larger than the actual buffer size, which would indicate a malformed or truncated BMP file
        // Note: Some BMP files may set this field to 0, so we only enforce the upper bound if it is non-zero
        if bf_size != 0 && bf_size > bmp.len() {
            panic!("BMP file size field is larger than the buffer");
        }

        // Get the pixel data offset, which indicates where the pixel data starts within the file
        let bf_off_bits = u32::from_le_bytes([bmp[10], bmp[11], bmp[12], bmp[13]]) as usize;
        // Verify that the pixel data offset is within the bounds of the buffer and comes after the file header
        if bf_off_bits < file_header_len || bf_off_bits > bmp.len() {
            panic!("BMP pixel data offset is out of bounds");
        }

        // Get the DIB slice starting after the BITMAPINFOHEADER, which contains the header, color table (if present), and pixel data
        let dib = &bmp.split_at(file_header_len).1;
        let header_len = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
        // Validate the DIB header size and structure to ensure it is a well-formed bitmap
        if header_len < info_header_len {
            panic!("Bitmap header is smaller than BITMAPINFOHEADER");
        }
        if header_len > dib.len() {
            panic!("Bitmap header extends beyond the DIB data");
        }
        if bf_off_bits < file_header_len + header_len {
            panic!("BMP pixel data offset precedes the header");
        }

        // Compute the byte offset from the start of the DIB slice to the pixel data, which is used to locate the pixel data within the DIB slice
        let pixel_offset = bf_off_bits - file_header_len;

        // Get the bits per pixel from the DIB header
        let bit_count = u16::from_le_bytes([dib[14], dib[15]]);
        // Bitmaps should only have 1 or 4 bits per pixel in this application
        if bit_count != 1 && bit_count != 4 {
            panic!("Unsupported pixel size: bits/pixel must be 1 or 4");
        }

        Self {
            dib,
            pixel_offset,
            bit_count,
        }
    }
}

/// Resample a 32bpp BGRA buffer using area averaging for fractional scaling.
/// # Arguments
/// - `src_buf` - The source buffer containing pixel data in 32bpp BGRA format.
/// - `src_w` - The width of the source bitmap in pixels.
/// - `src_h` - The height of the source bitmap in pixels.
/// - `dst_w` - The desired width of the destination bitmap in pixels.
/// - `dst_h` - The desired height of the destination bitmap in pixels.
/// # Returns
/// - `Ok(Vec<u8>)` - A new buffer containing the resampled pixel data in 32bpp BGRA format, with dimensions `dst_w` x `dst_h`.
/// - `Err` - If the input dimensions are invalid, if the source buffer is too small for the specified dimensions, or if any calculations overflow.
fn resample_32bpp_buffer(
    src_buf: &[u8],
    src_w: i32,
    src_h: i32,
    dst_w: i32,
    dst_h: i32,
) -> AnyResult<Vec<u8>> {
    // 1. Validate input dimensions to prevent invalid operations
    if src_w <= 0 || src_h <= 0 || dst_w <= 0 || dst_h <= 0 {
        return Err(format!(
            "Invalid bitmap dimensions:\n
            \tsrc_w: {src_w}, src_h: {src_h}, dst_w: {dst_w}, dst_h: {dst_h}"
        )
        .into());
    }

    // 2. Read Source Bits
    // Calculate the buffer size needed for the source bitmap (width * height * 4 bytes per pixel for 32bpp)
    let src_buf_len: usize = src_w
        .checked_mul(src_h)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Source bitmap dimensions are too large")?
        .try_into()
        .map_err(|e| format!("Failed to convert source buffer length to usize: {e}"))?;
    if src_buf.len() < src_buf_len {
        return Err("Source bitmap buffer is too small".into());
    }

    // 3. Prepare Destination
    // Calculate the buffer size needed for the destination bitmap (width * height * 4 bytes per pixel for 32bpp)
    let dst_buf_len: usize = dst_w
        .checked_mul(dst_h)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Destination bitmap dimensions are too large")?
        .try_into()
        .map_err(|e| format!("Failed to convert destination buffer length to usize: {e}"))?;
    let mut dst_buf = vec![0u8; dst_buf_len];

    // Scaling factors (Destination / Source) -> How many dst pixels per src pixel?
    // Actually, we usually want (Source / Destination) -> How much source does one dst pixel cover?
    let scale_x = dst_w as f32 / src_w as f32;
    let scale_y = dst_h as f32 / src_h as f32;

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
                        // Read the source pixel color, treating out-of-bounds as black
                        let (r, g, b) = {
                            if ix < 0 || ix >= src_w || iy < 0 || iy >= src_h {
                                (0.0, 0.0, 0.0)
                            } else {
                                let idx = ((iy * src_w + ix) * 4) as usize;
                                (
                                    f32::from(src_buf[idx + 2]), // R
                                    f32::from(src_buf[idx + 1]), // G
                                    f32::from(src_buf[idx]),     // B
                                )
                            }
                        };
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

    Ok(dst_buf)
}

impl GrafixState {
    /// Set the pen for drawing based on the normal flag.
    /// # Arguments
    /// - `hdc` - The device context to set the pen on.
    /// - `border_style` - The border style determining the pen to use.
    /// # Returns
    /// - `Ok(SelectObjectGuard<HPEN>)` - A guard that will restore the previous pen when dropped
    /// - `Err` - If selecting the pen into the DC fails or if the required pen is not initialized
    fn select_border_pen<'a>(
        &self,
        hdc: &'a HDC,
        border_style: BorderStyle,
    ) -> AnyResult<SelectObjectGuard<'a, HPEN>> {
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

        // Select the pen into the DC and return a guard that will restore the previous pen when dropped
        let guard = hdc.SelectObject(&**pen)?;
        Ok(guard)
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
        // Set the initial pen based on the requested border style
        let mut _pen_guard = self.select_border_pen(hdc, border_style)?;

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

        // Switch pen style for bottom and right edges if a beveled border is requested
        if border_style != BorderStyle::Flat {
            // Drop the current pen guard to restore the previous pen before selecting the new one
            drop(_pen_guard);
            _pen_guard = if border_style == BorderStyle::Sunken {
                self.select_border_pen(hdc, BorderStyle::Raised)?
            } else {
                self.select_border_pen(hdc, BorderStyle::Sunken)?
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
    pub(crate) fn draw_background(&self, hdc: &HDC) -> AnyResult<()> {
        let dx_window = self.wnd_pos.x;
        let dy_window = self.wnd_pos.y;

        // Scale the border widths based on the current DPI
        let b3 = self.dims.scale_dpi(3);
        let b2 = self.dims.scale_dpi(2);
        let b1 = self.dims.scale_dpi(1);

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
                    + (self.dims.bottom_space - self.dims.scale_dpi(6)),
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

    /// Load the bitmap resources and prepare cached DCs for rendering.
    /// # Arguments
    /// - `hdc` - The device context used for creating compatible DCs and bitmaps.
    /// - `color` - Whether to load color or monochrome resources.
    /// # Returns
    /// - `Ok(())` - If the bitmaps were loaded and cached successfully
    /// - `Err` - If loading any of the bitmap resources or creating cached DCs failed
    pub(crate) fn load_bitmaps(&mut self, hdc: &ReleaseDCGuard, color: bool) -> AnyResult<()> {
        // The bitmap files are embedded into the binary at compile time
        const BLOCKS_BMP: &[u8] = include_bytes!("../bmp/blocks.bmp");
        const BLOCKS_BW_BMP: &[u8] = include_bytes!("../bmp/blocksbw.bmp");
        const LED_BMP: &[u8] = include_bytes!("../bmp/led.bmp");
        const LED_BW_BMP: &[u8] = include_bytes!("../bmp/ledbw.bmp");
        const BUTTON_BMP: &[u8] = include_bytes!("../bmp/button.bmp");
        const BUTTON_BW_BMP: &[u8] = include_bytes!("../bmp/buttonbw.bmp");

        // The expected number of bytes for each decoded sprite
        const BLK_SPRITE_BYTES: usize = DX_BLK_96 as usize * DY_BLK_96 as usize * 4;
        const LED_SPRITE_BYTES: usize = DX_LED_96 as usize * DY_LED_96 as usize * 4;
        const BUTTON_SPRITE_BYTES: usize = DX_BUTTON_96 as usize * DY_BUTTON_96 as usize * 4;

        // Decode the embedded bitmap sheets into arrays of 32bpp BGRA byte arrays for each sprite
        const BLOCKS_COLOR_SPRITES: [[u8; BLK_SPRITE_BYTES]; I_BLK_MAX] =
            decode_bitmap_sheet::<I_BLK_MAX, BLK_SPRITE_BYTES>(
                DX_BLK_96 as usize,
                DY_BLK_96 as usize,
                BLOCKS_BMP,
            );
        const BLOCKS_BW_SPRITES: [[u8; BLK_SPRITE_BYTES]; I_BLK_MAX] =
            decode_bitmap_sheet::<I_BLK_MAX, BLK_SPRITE_BYTES>(
                DX_BLK_96 as usize,
                DY_BLK_96 as usize,
                BLOCKS_BW_BMP,
            );
        const LED_COLOR_SPRITES: [[u8; LED_SPRITE_BYTES]; I_LED_MAX] =
            decode_bitmap_sheet::<I_LED_MAX, LED_SPRITE_BYTES>(
                DX_LED_96 as usize,
                DY_LED_96 as usize,
                LED_BMP,
            );
        const LED_BW_SPRITES: [[u8; LED_SPRITE_BYTES]; I_LED_MAX] =
            decode_bitmap_sheet::<I_LED_MAX, LED_SPRITE_BYTES>(
                DX_LED_96 as usize,
                DY_LED_96 as usize,
                LED_BW_BMP,
            );
        const BUTTON_COLOR_SPRITES: [[u8; BUTTON_SPRITE_BYTES]; BUTTON_SPRITE_COUNT] =
            decode_bitmap_sheet::<BUTTON_SPRITE_COUNT, BUTTON_SPRITE_BYTES>(
                DX_BUTTON_96 as usize,
                DY_BUTTON_96 as usize,
                BUTTON_BMP,
            );
        const BUTTON_BW_SPRITES: [[u8; BUTTON_SPRITE_BYTES]; BUTTON_SPRITE_COUNT] =
            decode_bitmap_sheet::<BUTTON_SPRITE_COUNT, BUTTON_SPRITE_BYTES>(
                DX_BUTTON_96 as usize,
                DY_BUTTON_96 as usize,
                BUTTON_BW_BMP,
            );

        let blks = if color {
            &BLOCKS_COLOR_SPRITES
        } else {
            &BLOCKS_BW_SPRITES
        };
        let leds = if color {
            &LED_COLOR_SPRITES
        } else {
            &LED_BW_SPRITES
        };
        let buttons = if color {
            &BUTTON_COLOR_SPRITES
        } else {
            &BUTTON_BW_SPRITES
        };

        self.h_gray_pen = if color {
            HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(128, 128, 128))?.into()
        } else {
            HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(0, 0, 0))?.into()
        };

        self.h_white_pen = HPEN::CreatePen(PS::SOLID, 1, COLORREF::from_rgb(255, 255, 255))?.into();

        // Build a dedicated compatible DC + bitmap for every block sprite to speed up drawing.
        //
        // For fractional DPI scaling, simple StretchBlt produced unpleasant artifacts.
        // We therefore create the classic 96-DPI buffer first, then resample it using
        // `resample_32bpp_buffer` into a cached, DPI-sized bitmap.
        let dst_blk_w = self.dims.block.cx;
        let dst_blk_h = self.dims.block.cy;
        for i in 0..I_BLK_MAX {
            let dc_guard = hdc.CreateCompatibleDC()?;

            let final_bmp = if dst_blk_w != DX_BLK_96 || dst_blk_h != DY_BLK_96 {
                let dst_buf =
                    resample_32bpp_buffer(&blks[i], DX_BLK_96, DY_BLK_96, dst_blk_w, dst_blk_h)?;
                create_bitmap_from_32bpp(hdc, dst_blk_w, dst_blk_h, &dst_buf)?
            } else {
                create_bitmap_from_32bpp(hdc, DX_BLK_96, DY_BLK_96, &blks[i])?
            };

            self.mem_blk_cache[i] = Some(CachedBitmapGuard::new(dc_guard, &final_bmp)?);
        }

        // Cache LED digits in compatible bitmaps.
        for i in 0..I_LED_MAX {
            let dc_guard = hdc.CreateCompatibleDC()?;
            let bmp_guard = create_bitmap_from_32bpp(hdc, DX_LED_96, DY_LED_96, &leds[i])?;
            self.mem_led_cache[i] = Some(CachedBitmapGuard::new(dc_guard, &bmp_guard)?);
        }

        // Cache face button sprites in compatible bitmaps.
        //
        // Like the blocks, the face button looks best when we resample once and cache.
        let dst_btn_w = self.dims.button.cx;
        let dst_btn_h = self.dims.button.cy;
        for i in 0..BUTTON_SPRITE_COUNT {
            let dc_guard = hdc.CreateCompatibleDC()?;

            let final_bmp = if dst_btn_w != DX_BUTTON_96 || dst_btn_h != DY_BUTTON_96 {
                let dst_buf = resample_32bpp_buffer(
                    &buttons[i],
                    DX_BUTTON_96,
                    DY_BUTTON_96,
                    dst_btn_w,
                    dst_btn_h,
                )?;
                create_bitmap_from_32bpp(hdc, dst_btn_w, dst_btn_h, &dst_buf)?
            } else {
                create_bitmap_from_32bpp(hdc, DX_BUTTON_96, DY_BUTTON_96, &buttons[i])?
            };

            self.mem_button_cache[i] = Some(CachedBitmapGuard::new(dc_guard, &final_bmp)?);
        }

        Ok(())
    }
}

/// Decode a sprite sheet from the bitmap data into an array of 32bpp BGRA byte arrays for each sprite.
/// # Arguments
/// - `const SPRITES` - The number of sprites packed into the bitmap sheet.
/// - `const N` - The expected byte size of each output sprite (should be w * h * 4).
/// - `w` - The width of each sprite in pixels.
/// - `h` - The height of each sprite in pixels.
/// - `bmp` - A byte slice containing the entire bitmap file data, including the `BITMAPFILEHEADER`, `BITMAPINFOHEADER`, color table (if present), and pixel data for the sprite sheet.
/// # Returns
/// - A 2D array of bytes containing the decoded sprites in 32bpp BGRA format, where the first dimension indexes the individual sprites and the second dimension contains the pixel data for each sprite.
/// # Panics
/// - If the expected byte size of each output sprite does not match w * h * 4.
/// - If the bitmap width does not match the expected sprite width w.
/// - If the bitmap height does not match the expected layout of sprites (h * SPRITES).
/// - If the bitmap does not have a supported bits per pixel value (should be 1 or 4).
/// - If the color palette entries exceed the maximum supported size.
/// - If the palette data extends beyond the bounds of the DIB slice, which would indicate a malformed bitmap file.
/// - If the calculated offset for any sprite's pixel data exceeds the bounds of the DIB slice.
const fn decode_bitmap_sheet<const SPRITES: usize, const N: usize>(
    width: usize,
    height: usize,
    bmp: &'static [u8],
) -> [[u8; N]; SPRITES] {
    /// Maximum number of entries in the bitmap color palette that we support
    const MAX_PALETTE_ENTRIES: usize = 16;

    let sheet = BmpSheet::from_bytes(bmp);
    let dib = sheet.dib;

    if N != width * height * 4 {
        panic!("Sprite byte size mismatch");
    }

    // Extract header fields from the DIB header
    let header_len = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
    let w = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let h = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let clr_used = u32::from_le_bytes([dib[32], dib[33], dib[34], dib[35]]);

    // Validate that the extracted fields match the expected dimensions and format
    if w != width as i32 {
        panic!("Bitmap width does not match sprite width");
    }
    if h.unsigned_abs() as usize != height * SPRITES {
        panic!("Bitmap height does not match sprite sheet layout");
    }
    if sheet.bit_count != 1 && sheet.bit_count != 4 {
        panic!("Unsupported pixel size: bits/pixel must be 1 or 4");
    }

    // Calculate the number of entries in the color palette based on the bits per pixel and the number of colors used
    let palette_entries = if sheet.bit_count <= 8 {
        if clr_used == 0 {
            1usize << sheet.bit_count
        } else {
            clr_used as usize
        }
    } else {
        0
    };

    // Validate inputs to ensure they are within expected bounds and do not indicate a malformed bitmap structure
    if palette_entries > MAX_PALETTE_ENTRIES {
        panic!("Bitmap palette exceeds supported size");
    }
    if header_len + (palette_entries * 4) > dib.len() {
        panic!("Bitmap palette data out of bounds");
    }

    // Initialize the palette array with default entries
    let mut palette = [PaletteEntry {
        red: 0,
        green: 0,
        blue: 0,
    }; MAX_PALETTE_ENTRIES];

    // Read each palette entry from the DIB slice, which is located immediately after the bitmap header
    let mut i = 0;
    let mut cursor = header_len;
    while i < palette_entries {
        // Each palette entry is 4 bytes: blue, green, red, and reserved (which we ignore)
        palette[i] = PaletteEntry {
            red: dib[cursor + 2],
            green: dib[cursor + 1],
            blue: dib[cursor],
        };
        // Move the "cursor" to the next palette entry
        cursor += 4;
        i += 1;
    }
    // Calculate the stride (number of bytes in each row of pixel data)
    let stride = (width * sheet.bit_count as usize).div_ceil(32) * 4;

    // Decode each sprite in the sheet
    let mut sprites = [[0u8; N]; SPRITES];
    i = 0;
    while i < SPRITES {
        // Each sprite is located sequentially in the pixel data, so we calculate the offset for
        // the current sprite based on its index and the size of each sprite's pixel data (which is stride * height)
        let sprite_offset = sheet.pixel_offset + i * stride * height;
        if sprite_offset + (stride * height) > dib.len() {
            panic!("Bitmap sprite data out of bounds");
        }

        // Create a buffer to hold the converted pixel data in a 32bpp format.
        // The buffer size is the number of pixels (width * height) multiplied by 4 bytes per pixel for 32bpp
        let mut converted = [0u8; N];

        // Iterate over each line in the sprite
        let mut y = 0;
        while y < height {
            let src_y = if h < 0 { y } else { height - 1 - y };
            let row_start = sprite_offset + src_y * stride;
            let mut x = 0;
            while x < width {
                // Extract the pixel data for the current pixel based on the bits per pixel (color enabled or disabled)
                let (blue, green, red) = match sheet.bit_count {
                    1 => {
                        //
                        let byte = dib[row_start + (x / 8)];
                        let shift = 7 - (x % 8);
                        let idx = ((byte >> shift) & 0x01) as usize;
                        let color = palette[idx];
                        (color.blue, color.green, color.red)
                    }
                    4 => {
                        let byte = dib[row_start + (x / 2)];
                        let idx = if x % 2 == 0 { byte >> 4 } else { byte & 0x0f };
                        let color = palette[idx as usize];
                        (color.blue, color.green, color.red)
                    }
                    _ => panic!("Unsupported bitmap bit depth"),
                };

                let dst_idx = (y * width + x) * 4;
                converted[dst_idx] = blue;
                converted[dst_idx + 1] = green;
                converted[dst_idx + 2] = red;
                converted[dst_idx + 3] = 0;
                x += 1;
            }
            y += 1;
        }

        sprites[i] = converted;
        i += 1;
    }
    sprites
}

/// Create a compatible bitmap from a 32bpp BGRA buffer and select it into the provided device context.
/// # Arguments
/// - `hdc` - The device context to create the bitmap for.
/// - `width` - The width of the bitmap in pixels.
/// - `height` - The height of the bitmap in pixels.
/// - `buf` - A byte slice containing the pixel data for the bitmap in 32bpp BGRA format, where each pixel is represented by 4 bytes (blue, green, red, alpha).
/// # Returns
/// - `Ok(DeleteObjectGuard<HBITMAP>)` - A guard that will delete the bitmap when dropped.
/// - `Err` - If creating the compatible bitmap or setting the DIB bits on the bitmap fails, or if the input dimensions or buffer size are invalid.
fn create_bitmap_from_32bpp(
    hdc: &ReleaseDCGuard,
    width: i32,
    height: i32,
    buf: &[u8],
) -> AnyResult<DeleteObjectGuard<HBITMAP>> {
    let bmp = hdc.CreateCompatibleBitmap(width, height)?;
    if width <= 0 || height <= 0 {
        return Err("Bitmap dimensions must be positive".into());
    }

    // Calculate the expected buffer size for the given dimensions (width * height * 4 bytes per pixel)
    let expected_len: usize = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Bitmap dimensions are too large")?
        .try_into()
        .map_err(|e| format!("Failed to convert bitmap size to usize: {e}"))?;
    if buf.len() < expected_len {
        return Err("Bitmap buffer is too small".into());
    }

    // Construct a BITMAPINFO structure to describe the format of the pixel data being set on the bitmap
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI::RGB;
    bmi.bmiHeader.biSizeImage = expected_len as u32;

    let scan_lines = hdc.SetDIBits(&bmp, 0, height as u32, buf, &bmi, DIB::RGB_COLORS)?;
    if scan_lines == 0 {
        return Err("Failed to set DIB bits on destination bitmap".into());
    }
    Ok(bmp)
}
