//! Handling for the XYZZY cheat code.
//!
//! The XYZZY code is a classic Minesweeper cheat that reveals
//! whether the cell under the mouse cursor is a mine or not
//! by changing the color of the top left pixel of the screen.
//!
//! The code is activated by entering the sequence of keys
//! 'X', 'Y', 'Z', 'Z', 'Y' in order. Once activated, the
//! code can be toggled on and off by pressing Shift or can
//! be temporarily enabled by holding Ctrl.

use core::sync::atomic::{AtomicI32, Ordering};

use winsafe::co::{MK, PS, VK};
use winsafe::{AnyResult, COLORREF, HPEN, HWND, POINT};

use crate::winmine::WinMineMainWindow;

/// Length of the XYZZY cheat code sequence.
const CCH_XYZZY: i32 = 5;
/// Atomic counter tracking the progress of the XYZZY cheat code entry.
static I_XYZZY: AtomicI32 = AtomicI32::new(0);
/// The expected sequence of virtual key codes for the XYZZY cheat code.
const XYZZY_SEQUENCE: [VK; 5] = [VK::CHAR_X, VK::CHAR_Y, VK::CHAR_Z, VK::CHAR_Z, VK::CHAR_Y];

impl WinMineMainWindow {
    /// Handles the SHIFT key press for the XYZZY cheat code.
    /// If the cheat code has been fully entered, this function toggles
    /// the cheat code state by XORing the counter with 20 (0b10100).
    pub fn handle_xyzzys_shift(&self) {
        if I_XYZZY.load(Ordering::Relaxed) >= CCH_XYZZY {
            I_XYZZY.fetch_xor(20, Ordering::Relaxed);
        }
    }

    /// Handles default key presses for the XYZZY cheat code.
    /// It checks if the pressed key matches the expected character in the
    /// XYZZY sequence and updates the counter accordingly.
    /// If the sequence is broken, the counter is reset.
    /// # Arguments
    /// * `w_param` - The WPARAM from the keydown message, containing the virtual key code
    pub fn handle_xyzzys_default_key(&self, key: VK) {
        let current = I_XYZZY.load(Ordering::Relaxed);
        if current < CCH_XYZZY {
            let expected = XYZZY_SEQUENCE[current as usize];
            if expected == key {
                I_XYZZY.store(current + 1, Ordering::Relaxed);
            } else {
                I_XYZZY.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Handles mouse movement for the XYZZY cheat code.
    /// If the cheat code is active and the Control key is held down,
    /// or if the cheat code has been fully entered,
    /// it reveals whether the cell under the cursor is a bomb or not by
    /// setting the pixel at (0,0) of the device context to black (bomb) or white (no bomb).
    ///
    /// TODO: Don't do anything if the game is not active, it currently reveals incorrect info
    /// until the first block is revealed.
    /// # Arguments
    /// * `key` - The WPARAM from the mouse move message, containing key states.
    /// * `point` - The LPARAM from the mouse move message, containing cursor position.
    /// # Returns
    /// An `Ok(())` if successful, or an error if handling the mouse move failed.
    pub fn handle_xyzzys_mouse(&self, key: MK, point: POINT) -> AnyResult<()> {
        // Check if the XYZZY cheat code is active
        let state = I_XYZZY.load(Ordering::Relaxed);
        if state == 0 {
            return Ok(());
        }

        // Check if the Control key is held down.
        let control_down = key == MK::CONTROL;
        if (state == CCH_XYZZY && control_down) || state > CCH_XYZZY {
            let x_pos = self.x_box_from_xpos(point.x);
            let y_pos = self.y_box_from_ypos(point.y);
            self.state.write().cursor_pos = POINT { x: x_pos, y: y_pos };
            // Check if the cursor is within the board's range
            let in_range = x_pos > 0
                && y_pos > 0
                && x_pos <= self.state.read().board_width
                && y_pos <= self.state.read().board_height;
            if in_range {
                let hdc = HWND::DESKTOP.GetDC()?;
                let is_bomb = {
                    // Check if the block at the calculated index is a bomb
                    self.state.read().board_cells[x_pos as usize][y_pos as usize].bomb
                };

                // Determine the color based on bomb status:
                // * Black: bomb present
                // * White: no bomb
                let color = if is_bomb {
                    COLORREF::from_rgb(0, 0, 0)
                } else {
                    COLORREF::from_rgb(0xFF, 0xFF, 0xFF)
                };

                // Set the pixel at (0,0) to indicate bomb status.
                HPEN::CreatePen(PS::SOLID, 0, color).and_then(|mut pen| {
                    let mut old_pen = hdc.SelectObject(&pen.leak())?;
                    hdc.MoveToEx(0, 0, None)?;
                    // LineTo excludes the endpoint, so drawing to (1,0) sets pixel (0,0)
                    hdc.LineTo(1, 0)?;
                    hdc.SelectObject(&old_pen.leak())?;
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
}
