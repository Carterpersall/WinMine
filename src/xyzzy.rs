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

use winsafe::co::{MK, PS, VK};
use winsafe::{AnyResult, COLORREF, HPEN, HWND, POINT};

use crate::rtns::GameState;

/// Length of the XYZZY cheat code sequence.
const XYZZY_LENGTH: usize = XYZZY_SEQUENCE.len();
/// The expected sequence of virtual key codes for the XYZZY cheat code.
const XYZZY_SEQUENCE: [VK; 5] = [VK::CHAR_X, VK::CHAR_Y, VK::CHAR_Z, VK::CHAR_Z, VK::CHAR_Y];

impl GameState {
    /// Handles the SHIFT key press for the XYZZY cheat code.
    /// If the cheat code has been fully entered, this function toggles
    /// the cheat code state by XORing the counter with 20 (0b10100).
    pub(crate) fn toggle_xyzzy(&mut self) {
        if self.xyzzy_progress >= XYZZY_LENGTH {
            self.xyzzy_progress ^= 20;
        }
    }

    /// Handles key presses for the XYZZY cheat code.
    /// It checks if the pressed key matches the expected character in the
    /// XYZZY sequence and updates the counter accordingly.
    /// If the sequence is broken, the counter is reset.
    /// # Arguments
    /// - `key` - The virtual key code from the key press event.
    pub(crate) fn handle_xyzzys_input(&mut self, key: VK) {
        if self.xyzzy_progress < XYZZY_LENGTH {
            let expected = XYZZY_SEQUENCE[self.xyzzy_progress];
            if expected == key {
                self.xyzzy_progress += 1;
            } else {
                self.xyzzy_progress = 0;
            }
        }
    }

    /// Handles mouse movement for the XYZZY cheat code.
    /// If the cheat code is active and the Control key is held down,
    /// or if the cheat code has been fully entered,
    /// it reveals whether the cell under the cursor is a bomb or not by
    /// setting the pixel at (0,0) of the device context to black (bomb) or white (no bomb).
    ///
    /// until the first block is revealed.
    /// # Arguments
    /// - `key` - The WPARAM from the mouse move message, containing key states.
    /// - `point` - The LPARAM from the mouse move message, containing cursor position.
    /// # Returns
    /// - `Ok(())` - If the mouse move was handled successfully
    /// - `Err` - If there was an error during handling
    pub(crate) fn handle_xyzzys_mouse(&mut self, key: MK, point: POINT) -> AnyResult<()> {
        // Check if the Control key is held down.
        let control_down = key.has(MK::CONTROL);

        // Check if the XYZZY cheat code is active
        let state = self.xyzzy_progress;
        if (state == XYZZY_LENGTH && control_down) || state > XYZZY_LENGTH {
            let (x_pos, y_pos) = self.box_from_point(point);
            self.cursor_x = x_pos;
            self.cursor_y = y_pos;
            // Check if the cursor is within the board's range
            if self.in_range(x_pos, y_pos) {
                let hdc = HWND::DESKTOP.GetDC()?;
                let is_bomb = {
                    // Check if the block at the calculated index is a bomb
                    self.board_cells[x_pos][y_pos].bomb
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
                HPEN::CreatePen(PS::SOLID, 0, color).and_then(|pen| {
                    let _pen_guard = hdc.SelectObject(&*pen)?;
                    hdc.MoveToEx(0, 0, None)?;
                    // LineTo excludes the endpoint, so drawing to (1,0) sets pixel (0,0)
                    hdc.LineTo(1, 0)?;
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
}
