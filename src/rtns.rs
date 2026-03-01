//! Handlers for the core game logic and state management.
//! This includes board representation, game status tracking, and related utilities.

use core::cmp::min;
use core::mem::replace;
use core::ops::Deref as _;

use bitflags::bitflags;
use strum_macros::VariantArray;
use winsafe::co::{MK, WM};
use winsafe::guard::ReleaseDCGuard;
use winsafe::msg::WndMsg;
use winsafe::{AnyResult, HWND, POINT, PtInRect, RECT, prelude::*};

use crate::grafix::{ButtonSprite, GrafixState};
use crate::pref::{CCH_NAME_MAX, GameType, Pref};
use crate::sound::Sound;
use crate::util::Rng;
use crate::winmine::{NEW_RECORD_DLG, WinMineMainWindow};

/// Encoded board values used to track each tile state.
///
/// These values are used to get the visual representation of each cell, in reverse order.
#[derive(Copy, Clone, Eq, PartialEq, VariantArray)]
pub(crate) enum BlockCell {
    /// A blank cell with no adjacent bombs.
    Blank = 0,
    /// A cell with 1 adjacent bomb.
    One = 1,
    /// A cell with 2 adjacent bombs.
    Two = 2,
    /// A cell with 3 adjacent bombs.
    Three = 3,
    /// A cell with 4 adjacent bombs.
    Four = 4,
    /// A cell with 5 adjacent bombs.
    Five = 5,
    /// A cell with 6 adjacent bombs.
    Six = 6,
    /// A cell with 7 adjacent bombs.
    Seven = 7,
    /// A cell with 8 adjacent bombs.
    Eight = 8,
    /// The depressed version of the guess (?) mark.
    GuessDown = 9,
    /// A cell containing a bomb that has been revealed.
    BombDown = 10,
    /// An incorrectly marked cell.
    Wrong = 11,
    /// A detonated bomb cell.
    Explode = 12,
    /// The raised version of the guess (?) mark.
    GuessUp = 13,
    /// A flagged cell.
    Flagged = 14,
    /// A blank cell in the raised state.
    BlankUp = 15,
}

impl From<u8> for BlockCell {
    /// Convert a `u8` value to a `BlockCell` enum.
    /// # Arguments
    /// - `value` - The `u8` value to convert.
    /// # Returns
    /// - The corresponding `BlockCell` enum variant.
    fn from(value: u8) -> Self {
        match value {
            0 => BlockCell::Blank,
            1 => BlockCell::One,
            2 => BlockCell::Two,
            3 => BlockCell::Three,
            4 => BlockCell::Four,
            5 => BlockCell::Five,
            6 => BlockCell::Six,
            7 => BlockCell::Seven,
            8 => BlockCell::Eight,
            9 => BlockCell::GuessDown,
            10 => BlockCell::BombDown,
            11 => BlockCell::Wrong,
            12 => BlockCell::Explode,
            13 => BlockCell::GuessUp,
            14 => BlockCell::Flagged,
            15 => BlockCell::BlankUp,
            _ => BlockCell::Blank,
        }
    }
}

/// Struct representing information about a single block on the board.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) struct BlockInfo {
    /// Indicates whether the block contains a bomb.
    pub bomb: bool,
    /// Indicates whether the block has been visited (revealed).
    pub visited: bool,
    /// The type of block (visual representation).
    pub block_type: BlockCell,
}

impl From<BlockCell> for BlockInfo {
    /// Convert a `BlockCell` enum to a `BlockInfo` struct.
    ///
    /// The `bomb` and `visited` fields are set to `false` by default.
    /// # Arguments
    /// - `cell` - The `BlockCell` enum to convert.
    /// # Returns
    /// - A `BlockInfo` struct with the `block_type` set to the given `BlockCell`, and `bomb` and `visited` set to `false`.
    fn from(cell: BlockCell) -> Self {
        Self {
            bomb: false,
            visited: false,
            block_type: cell,
        }
    }
}

/// Maximum number of horizontal board cells
pub(crate) const MAX_X_BLKS: usize = 30;
/// Maximum number of vertical board cells
pub(crate) const MAX_Y_BLKS: usize = 25;
/// Upper bound on the flood-fill work queue used for empty regions.
const I_STEP_MAX: usize = 100;

/// Timer identifier used for the per-second gameplay timer.
pub(crate) const ID_TIMER: usize = 1;

bitflags! {
    /// Packed flags indicating adjustments needed for the main window.
    #[derive(Clone)]
    pub struct AdjustFlag: u8 {
        /// Indicate that a window resize is needed.
        const Resize = 0b01;
        /// Indicate that a display refresh is needed.
        const Redraw = 0b10;
        /// Indicate that both a resize and redraw are needed.
        const ResizeAndRedraw = 0b11;
    }
}

bitflags! {
    /// Flags defining the current game status.
    #[derive(Clone)]
    pub struct StatusFlag: u8 {
        /// Game is currently being played.
        const Play = 0b0001;
        /// Game is currently paused.
        const Pause = 0b0010;
        /// Game is currently minimized.
        const Minimized = 0b0100;
        /// Game is over (win or loss).
        const GameOver = 0b1000;
    }
}

/// The current state of the in-game timer.
#[derive(Eq, PartialEq, Default)]
enum TimerState {
    /// The timer is not running.
    #[default]
    Stopped,
    /// The timer is running and should be updated every second.
    Running,
    /// The timer is paused and should not be updated until resumed.
    Paused,
}

/// Struct representing the current state of the in-game timer, including elapsed time and timer state.
#[derive(Default)]
pub(crate) struct Timer {
    /// The current state of the timer (running, paused, or stopped).
    state: TimerState,
    /// Current elapsed time in seconds.
    ///
    /// The timer should never exceed 999 seconds, so u16 is sufficient.
    pub elapsed: u16,
}

impl Timer {
    /// Starts the timer by setting its state to `Running`.
    const fn start(&mut self) {
        self.state = TimerState::Running;
    }

    /// Pauses the timer if it is currently running.
    fn pause(&mut self) {
        if self.state == TimerState::Running {
            self.state = TimerState::Paused;
        }
    }

    /// Resumes the timer if it is currently paused.
    fn resume(&mut self) {
        if self.state == TimerState::Paused {
            self.state = TimerState::Running;
        }
    }

    /// Stops the timer.
    const fn stop(&mut self) {
        self.state = TimerState::Stopped;
    }

    /// Increments the timer by one second if it is currently running and has not reached the maximum of 999 seconds.
    /// # Returns
    /// - `true` - If the timer was incremented.
    /// - `false` - If the timer was not incremented (either because it is not running or because it has reached the maximum).
    fn tick(&mut self) -> bool {
        if self.state == TimerState::Running && self.elapsed < 999 {
            self.elapsed += 1;
            true
        } else {
            false
        }
    }

    /// Stops the timer and resets the elapsed time to zero.
    const fn reset(&mut self) {
        self.stop();
        self.elapsed = 0;
    }
}

/// Represents the current state of the game.
pub(crate) struct GameState {
    /// Graphics state containing bitmaps and rendering logic.
    pub grafix: GrafixState,
    /// Current user preferences.
    pub prefs: Pref,
    /// Aggregated status flags defining the current game state.
    pub game_status: StatusFlag,
    /// Zero-indexed board width in cells
    pub board_width: usize,
    /// Zero-indexed board height in cells
    pub board_height: usize,
    /// Current button face sprite
    pub btn_face_state: ButtonSprite,
    /// Indicates whether the face button is currently being pressed.
    pub btn_face_pressed: bool,
    /// Current number of bombs left to mark
    ///
    /// Note: The bomb count can go negative if the user marks more squares than there are bombs.
    pub bombs_left: i16,
    /// Number of visited boxes (revealed non-bomb cells).
    ///
    /// Note: Maximum value is 2<sup>16</sup>, or a 256 x 256 board with no bombs.
    boxes_visited: u16,
    /// Current cursor x position in board coordinates
    pub cursor_x: usize,
    /// Current cursor y position in board coordinates
    pub cursor_y: usize,
    /// Signals that the next click should be ignored
    ///
    /// This is used after window activation to prevent accidental clicks.
    pub ignore_next_click: bool,
    /// Indicates whether a chord operation is currently active.
    ///
    /// A chord operation allows the player to reveal adjacent squares if the number of marked squares
    /// around a revealed number matches that number.
    ///
    /// Holding a chord operation depresses a 3x3 area around the cursor.
    ///
    /// A chord operation will begin if:
    /// - Both left and right buttons are held down, and the middle button is not held down
    /// - Only the middle button is held down
    /// - Shift is held _then_ left button is held down
    chord_active: bool,
    /// Indicates whether a drag operation is currently active
    drag_active: bool,
    /// 2D Array representing the state of each cell on the board
    pub board_cells: [[BlockInfo; MAX_Y_BLKS]; MAX_X_BLKS],
    /// Initial number of bombs at the start of the game
    total_bombs: i16,
    /// Total number of visited boxes needed to win
    boxes_to_win: u16,
    /// Current state of the in-game timer, which tracks elapsed time and whether the timer is running, paused, or stopped.
    pub timer: Timer,
    /// Random number generator used for bomb placement.
    rng: Rng,
}

impl GameState {
    /// Creates a new default `GameState`
    pub(crate) fn new() -> Self {
        Self {
            grafix: GrafixState::default(),
            prefs: Pref {
                beginner_name: String::with_capacity(CCH_NAME_MAX),
                inter_name: String::with_capacity(CCH_NAME_MAX),
                expert_name: String::with_capacity(CCH_NAME_MAX),
                ..Default::default()
            },
            game_status: StatusFlag::Minimized | StatusFlag::GameOver,
            board_width: 0,
            board_height: 0,
            btn_face_state: ButtonSprite::Happy,
            btn_face_pressed: false,
            bombs_left: 0,
            boxes_visited: 0,
            cursor_x: 0,
            cursor_y: 0,
            ignore_next_click: false,
            chord_active: false,
            drag_active: false,
            board_cells: [[BlockInfo {
                bomb: false,
                visited: false,
                block_type: BlockCell::BlankUp,
            }; MAX_Y_BLKS]; MAX_X_BLKS],
            total_bombs: 0,
            boxes_to_win: 0,
            timer: Timer::default(),
            rng: Rng::seed_rng(),
        }
    }
}

impl GameState {
    /// Check if the given coordinates are within the valid range of the board.
    /// # Arguments
    /// - `x` - The X coordinate.
    /// - `y` - The Y coordinate.
    /// # Returns
    /// - `true` - If the coordinates are within the valid range of the board.
    /// - `false` - If the coordinates are out of range.
    pub(crate) const fn in_range(&self, x: usize, y: usize) -> bool {
        x <= self.board_width && y <= self.board_height
    }

    /// Convert a set of coordinates in pixels to a box index on the board.
    /// # Arguments
    /// - `pos`: The POINT structure containing the x and y coordinates in pixels.
    /// # Returns
    /// - The corresponding box index.
    /// # Panics
    /// - In debug mode, this function will panic if the cell width is zero or negative, which would indicate an invalid game state.
    /// - In release mode, the function assumes that the cell width is valid and does not perform these checks for performance reasons.
    ///   If the cell width is zero in release mode, this will result in a division by zero and a panic.
    pub(crate) const fn box_from_point(&self, pos: POINT) -> (usize, usize) {
        let cell = self.grafix.dims.block.cx;
        #[cfg(debug_assertions)]
        {
            if cell == 0 {
                panic!("Cell width is zero, this indicates an invalid game state.");
            } else if cell < 0 {
                panic!("Cell width is negative, invalid game state.");
            }
        }
        (
            ((pos.x - self.grafix.dims.left_space) / cell) as usize,
            ((pos.y - self.grafix.dims.grid_offset) / cell) as usize,
        )
    }

    /// Check if the player has won the game.
    /// # Returns
    /// - `true` - If the player has won.
    /// - `false` - If the player has not won.
    const fn check_win(&self) -> bool {
        self.boxes_visited == self.boxes_to_win
    }

    /// Count the number of adjacent marked squares around the specified coordinates.
    /// # Arguments
    /// - `x_center` - The X coordinate of the center square.
    /// - `y_center` - The Y coordinate of the center square.
    /// # Returns
    /// - The number of adjacent marked squares (maximum 8).
    fn count_marks(&self, x_center: usize, y_center: usize) -> u8 {
        let mut count = 0;
        for y in y_center.saturating_sub(1)..=min(y_center + 1, self.board_height) {
            for x in x_center.saturating_sub(1)..=min(x_center + 1, self.board_width) {
                if self.board_cells[x][y].block_type == BlockCell::Flagged {
                    count += 1;
                }
            }
        }
        count
    }

    /// Handles clicks on the smiley face button.
    /// # Arguments
    /// - `hdc`: Handle to the device context to draw on.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(true)` - If the click was on the button and handled.
    /// - `Ok(false)` - If the click was not on the button.
    /// - `Err` - If an error occurred while handling the click.
    fn btn_click_handler(&mut self, hdc: &ReleaseDCGuard, point: POINT) -> AnyResult<bool> {
        let rc = {
            RECT {
                left: (self.grafix.wnd_pos.x - self.grafix.dims.button.cx) / 2,
                right: (self.grafix.wnd_pos.x + self.grafix.dims.button.cx) / 2,
                top: self.grafix.dims.top_led,
                bottom: self.grafix.dims.top_led + self.grafix.dims.button.cy,
            }
        };
        if !PtInRect(rc, point) {
            return Ok(false);
        }

        self.btn_face_pressed = true;
        self.grafix.draw_button(hdc, ButtonSprite::Down)?;

        Ok(true)
    }

    /// Handles smiley-face interaction while the left button is pressed.
    /// # Arguments
    /// - `hdc`: Handle to the device context to draw on.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(())` - If the mouse move was handled.
    /// - `Err` - If an error occurred while handling the mouse move.
    fn handle_face_button_mouse_move(&self, hdc: &ReleaseDCGuard, point: POINT) -> AnyResult<()> {
        let rc = {
            RECT {
                left: (self.grafix.wnd_pos.x - self.grafix.dims.button.cx) / 2,
                right: (self.grafix.wnd_pos.x + self.grafix.dims.button.cx) / 2,
                top: self.grafix.dims.top_led,
                bottom: self.grafix.dims.top_led + self.grafix.dims.button.cy,
            }
        };
        if PtInRect(rc, point) {
            // If the cursor is over the button, draw the "pressed" state
            self.grafix.draw_button(hdc, ButtonSprite::Down)?;
        } else {
            // If the cursor is not over the button, draw the "not pressed" state
            self.grafix.draw_button(hdc, self.btn_face_state)?;
        }

        Ok(())
    }

    /// Begins a primary button drag operation.
    /// # Arguments
    /// - `hdc` - Handle to the device context, used to draw the button in the "caution" state to indicate the drag has started.
    /// # Returns
    /// - `Ok(())` - If the drag operation was successfully initiated and the button was drawn.
    /// - `Err` - If an error occurred while getting the device context.
    fn begin_primary_button_drag(&mut self, hdc: &ReleaseDCGuard) -> AnyResult<()> {
        self.drag_active = true;
        self.cursor_x = usize::MAX - 1;
        self.cursor_y = usize::MAX - 1;
        self.grafix.draw_button(hdc, ButtonSprite::Caution)?;
        Ok(())
    }

    /// Finishes a primary button drag operation.
    /// # Arguments
    /// - `hwnd` - Handle to the main window, used to get the device context and track the mouse if the game is not active.
    /// # Returns
    /// - `Ok(())` - If the drag operation was successfully finished and the button was drawn.
    /// - `Err` - If an error occurred while getting the device context or drawing the button.
    pub(crate) fn finish_primary_button_drag(&mut self, hwnd: &HWND) -> AnyResult<()> {
        self.drag_active = false;
        if self.game_status.contains(StatusFlag::Play) {
            // Check if the cursor is within the valid range of the board
            if self.in_range(self.cursor_x, self.cursor_y) {
                // If the number of visits and elapsed seconds are both zero, the game has not started yet
                if self.boxes_visited == 0 && self.timer.elapsed == 0 {
                    // Play the tick sound, display the initial time, and start the timer
                    self.timer.start();
                    self.do_timer(hwnd)?;
                    hwnd.SetTimer(ID_TIMER, 1000, None)?;
                }

                // If the game is not in play mode, reset the cursor position to a location off the board
                if !self.game_status.contains(StatusFlag::Play) {
                    self.cursor_x = usize::MAX - 2;
                    self.cursor_y = usize::MAX - 2;
                }

                // Determine whether to chord (select adjacent squares) or step (reveal a single square)
                if self.chord_active {
                    self.step_block(hwnd, self.cursor_x, self.cursor_y)?;
                } else if self.in_range(self.cursor_x, self.cursor_y)
                    && !self.board_cells[self.cursor_x][self.cursor_y].visited
                    && self.board_cells[self.cursor_x][self.cursor_y].block_type
                        != BlockCell::Flagged
                {
                    // Handle a click on a single square
                    self.step_square(hwnd, self.cursor_x, self.cursor_y)?;
                }
            }

            self.grafix
                .draw_button(hwnd.GetDC()?.deref(), self.btn_face_state)?;
        } else {
            // If the game is not active, track the mouse on a location off the board to reset any drag states
            self.track_mouse(&hwnd.GetDC()?, usize::MAX - 2, usize::MAX - 2)?;
        }
        // If a chord operation was active, end it now
        self.chord_active = false;
        Ok(())
    }

    /// Handles mouse move events.
    ///
    /// TODO: This function handles more than just mouse movement, rename it accordingly.
    /// # Arguments
    /// - `hwnd`: Handle to the main window, used to get the device context and track the mouse if the game is not active.
    /// - `key`: The mouse buttons currently pressed.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(())` - If the mouse move was handled successfully.
    /// - `Err` - If an error occurred while handling the mouse move or if getting the device context failed.
    pub(crate) fn handle_mouse_move(
        &mut self,
        hwnd: &HWND,
        key: MK,
        point: POINT,
    ) -> AnyResult<()> {
        if self.btn_face_pressed {
            // If the face button is being clicked, handle mouse movement for that interaction
            self.handle_face_button_mouse_move(&hwnd.GetDC()?, point)?;
        } else if self.drag_active {
            // If the user is dragging, track the mouse position
            if self.game_status.contains(StatusFlag::Play) {
                let (x_new, y_new) = self.box_from_point(point);
                self.track_mouse(&hwnd.GetDC()?, x_new, y_new)?;
            } else {
                self.finish_primary_button_drag(hwnd)?;
            }
        } else if self.timer.elapsed > 0 {
            // If the user is not dragging but the game is active, track the mouse position for the XYZZY cheat code
            self.handle_xyzzys_mouse(key, point)?;
        }
        Ok(())
    }

    /// Handles right mouse button down events.
    /// # Arguments
    /// - `hwnd`: Handle to the main window, used to get the device context and make guesses if the game is active.
    /// - `btn`: The mouse button that was pressed.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(())` - If the right button down was handled successfully.
    /// - `Err` - If an error occurred.
    pub(crate) fn handle_rbutton_down(
        &mut self,
        hwnd: &HWND,
        btn: MK,
        point: POINT,
    ) -> AnyResult<()> {
        // Ignore right-clicks if the next click is set to be ignored or if the game is not active
        if !replace(&mut self.ignore_next_click, false)
            && self.game_status.contains(StatusFlag::Play)
        {
            if btn & (MK::LBUTTON | MK::RBUTTON | MK::MBUTTON) == MK::LBUTTON | MK::RBUTTON {
                // If the left and right buttons are both down, and the middle button is not down, start a chord operation
                self.chord_active = true;
                self.track_mouse(&hwnd.GetDC()?, usize::MAX - 3, usize::MAX - 3)?;
                self.begin_primary_button_drag(&hwnd.GetDC()?)?;
                self.handle_mouse_move(hwnd, btn, point)?;
            } else {
                // Regular right-click: Cycle through blank -> flag -> question mark states depending on preferences

                // Get the box coordinates from the mouse position
                let (x, y) = self.box_from_point(point);

                // Return if the square is out of range or already visited.
                if !self.in_range(x, y) || self.board_cells[x][y].visited {
                    return Ok(());
                }

                // If currently flagged
                let hdc = hwnd.GetDC()?;
                let block = if self.board_cells[x][y].block_type == BlockCell::Flagged {
                    // Increment the bomb count
                    self.bombs_left += 1;
                    self.grafix.draw_bomb_count(&hdc, self.bombs_left)?;

                    // If marks are allowed, change to question mark; otherwise, change to blank
                    if self.prefs.mark_enabled {
                        BlockCell::GuessUp
                    } else {
                        BlockCell::BlankUp
                    }
                } else if self.board_cells[x][y].block_type == BlockCell::GuessUp {
                    // If currently marked with a question mark, change to blank
                    // No need to update the bomb count since the guess mark doesn't affect it
                    BlockCell::BlankUp
                } else {
                    // Currently blank; change to flagged and decrement bomb count
                    self.bombs_left -= 1;
                    self.grafix.draw_bomb_count(&hdc, self.bombs_left)?;
                    BlockCell::Flagged
                };

                // Update the block type and redraw the square
                self.board_cells[x][y].block_type = block;
                self.grafix.draw_block(&hdc, x, y, &self.board_cells)?;

                // If the user has flagged the last bomb, they have won
                if self.board_cells[x][y].block_type == BlockCell::Flagged && self.check_win() {
                    self.game_over(hwnd, true)?;
                }
            }
        }
        Ok(())
    }

    /// Handles left mouse button down events.
    /// # Arguments
    /// - `hwnd`: Handle to the main window.
    /// - `vkey`: The virtual key code of the mouse button event.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    // - `Ok(())` - If the left button down was handled successfully.
    /// - `Err` - If an error occurred.
    pub(crate) fn handle_lbutton_down(
        &mut self,
        hwnd: &HWND,
        vkey: MK,
        point: POINT,
    ) -> AnyResult<()> {
        // If the next click should be ignored of if the click was on the button and was handled, do nothing else
        if !replace(&mut self.ignore_next_click, false)
            && !self.btn_click_handler(&hwnd.GetDC()?, point)?
        {
            if vkey.has(MK::RBUTTON) || vkey.has(MK::SHIFT) {
                // If the right button or the shift key is also down, start a chord operation
                self.chord_active = true;
            }
            if self.game_status.contains(StatusFlag::Play) {
                self.begin_primary_button_drag(&hwnd.GetDC()?)?;
                self.handle_mouse_move(hwnd, vkey, point)?;
            }
        }
        Ok(())
    }

    /// Handles middle mouse button down events.
    /// # Arguments
    /// - `hwnd`: Handle to the main window.
    /// - `vkey`: The virtual key code of the mouse button event.
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(())` - If the middle button down was handled successfully.
    /// - `Err` - If an error occurred.
    pub(crate) fn handle_mbutton_down(
        &mut self,
        hwnd: &HWND,
        vkey: MK,
        point: POINT,
    ) -> AnyResult<()> {
        // Ignore middle-clicks if the next click is to be ignored
        if !replace(&mut self.ignore_next_click, false) {
            if vkey.has(MK::MBUTTON) {
                // If the middle button is pressed, start a chord operation
                // However, if a chord is already active, end the chord instead
                self.chord_active = !self.chord_active;
            }

            // Is the game is active, start a drag operation
            if self.game_status.contains(StatusFlag::Play) {
                self.begin_primary_button_drag(&hwnd.GetDC()?)?;
                self.handle_mouse_move(hwnd, vkey, point)?;
            }
        }
        Ok(())
    }

    /// Enqueue a square for flood-fill processing if it is empty.
    ///
    /// TODO: Could this function be merged with `step_box` to avoid passing the queue around?
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `queue` - The flood-fill work queue.
    /// - `tail` - The current tail index of the queue.
    /// - `x` - The X coordinate of the square.
    /// - `y` - The Y coordinate of the square.
    /// # Returns
    /// - `Ok(())` - If the square was successfully processed.
    /// - `Err` - If an error occurred while drawing a square.
    /// # Panics (Debug Only)
    /// - If the square is a bomb, which should never happen since only empty squares should be enqueued for flood-fill processing.
    ///   If this panic occurs, it indicates a bug in the flood-fill logic that is allowing bombs to be processed.
    fn step_xy(
        &mut self,
        hdc: &ReleaseDCGuard,
        queue: &mut [(usize, usize); I_STEP_MAX],
        tail: &mut usize,
        x: usize,
        y: usize,
    ) -> AnyResult<()> {
        let blk = self.board_cells[x][y];
        if blk.visited || blk.block_type == BlockCell::Flagged {
            // Already visited, out of range, or marked as a bomb; do nothing
            return Ok(());
        }

        #[cfg(debug_assertions)]
        {
            // Flood-fill processed squares should never be bombs.
            // If this assertion fails, it indicates a bug in the flood-fill logic that is allowing bombs to be processed.
            if blk.bomb {
                panic!("Attempted to flood-fill a bomb at ({}, {})", x, y);
            }
        }

        self.boxes_visited += 1;

        // Count the number of adjacent bombs
        let mut bombs = 0;
        for y_n in y.saturating_sub(1)..=min(y + 1, self.board_height) {
            for x_n in x.saturating_sub(1)..=min(x + 1, self.board_width) {
                if self.board_cells[x_n][y_n].bomb {
                    bombs += 1;
                }
            }
        }

        // Update the revealed block to show the adjacent bomb count and draw it
        self.board_cells[x][y] = BlockInfo {
            bomb: false,
            visited: true,
            block_type: BlockCell::from(bombs),
        };
        self.grafix.draw_block(hdc, x, y, &self.board_cells)?;

        // If no adjacent bombs, enqueue for further flood-fill processing
        if bombs == 0 {
            queue[*tail] = (x, y);
            *tail += 1;
            if *tail == I_STEP_MAX {
                // Queue overflow, loop back to the start and overwrite old entries
                *tail = 0;
            }
        }
        Ok(())
    }

    /// Flood-fill contiguous empty squares starting from (x, y).
    ///
    /// TODO: Should this function be renamed?
    /// # Arguments
    /// - `hdc` - The device context to draw on.
    /// - `x` - X coordinate of the starting square
    /// - `y` - Y coordinate of the starting square
    /// # Returns
    /// - `Ok(())` - If the flood-fill was successful.
    /// - `Err` - If an error occurred while drawing the board.
    fn step_box(&mut self, hdc: &ReleaseDCGuard, x: usize, y: usize) -> AnyResult<()> {
        // Use a queue to perform a breadth-first flood-fill of empty squares.
        // The queue has a fixed maximum size, and if it overflows we loop back to the start and overwrite old entries.
        let mut queue = [(0, 0); I_STEP_MAX];
        // `head` tracks the current index being processed
        let mut head = 0usize;
        // `tail` tracks the next open index for adding new squares to process
        let mut tail = 0usize;

        // Enqueue the initial square; if it is empty, this will kick off the flood-fill process
        self.step_xy(hdc, &mut queue, &mut tail, x, y)?;

        // Process squares in the queue until there are no more to process
        while head != tail {
            // For each square in queue, check the 8 surrounding squares, and enqueue any that have no adjacent bombs
            let (sx, sy) = queue[head];

            // Iterate over the 3x3 area around the current square, ensuring we don't go out of bounds or process the center square again
            for ty in sy.saturating_sub(1)..=min(sy + 1, self.board_height) {
                for tx in sx.saturating_sub(1)..=min(sx + 1, self.board_width) {
                    if (tx, ty) == (sx, sy) {
                        // Skip the center square
                        continue;
                    }
                    self.step_xy(hdc, &mut queue, &mut tail, tx, ty)?;
                }
            }

            head += 1;
            if head == I_STEP_MAX {
                // Queue overflow, loop back to the start
                head = 0;
            }
        }
        Ok(())
    }

    /// Handle the end of the game - stopping the timer, revealing bombs, updating the face, and recording wins.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// - `win` - `true` if the player has won, `false` otherwise
    /// # Returns
    /// - `Ok(())` - If the game over state was successfully handled.
    /// - `Err` - If an error occurred while drawing the board.
    fn game_over(&mut self, hwnd: &HWND, win: bool) -> AnyResult<()> {
        self.timer.stop();
        let hdc = hwnd.GetDC()?;

        // Update the button face to show win or loss
        let state = if win {
            ButtonSprite::Win
        } else {
            ButtonSprite::Lose
        };
        self.btn_face_state = state;
        self.grafix.draw_button(&hdc, state)?;

        // Show all of the bombs and mark incorrect guesses
        for y in 0..=self.board_height {
            for x in 0..=self.board_width {
                // If the cell is not visited is not the exploded bomb cell
                if !self.board_cells[x][y].visited
                    && self.board_cells[x][y].block_type != BlockCell::Explode
                {
                    if self.board_cells[x][y].bomb {
                        if self.board_cells[x][y].block_type != BlockCell::Flagged {
                            // If a bomb cell was not marked, reveal it
                            let cell = if win {
                                BlockCell::Flagged
                            } else {
                                BlockCell::BombDown
                            };
                            self.board_cells[x][y].block_type = cell;
                        }
                    } else if self.board_cells[x][y].block_type == BlockCell::Flagged {
                        // If a non-bomb cell was marked as a bomb, show it as incorrect
                        self.board_cells[x][y].block_type = BlockCell::Wrong;
                    }
                }
            }
        }
        self.grafix
            .draw_grid(&hdc, self.board_width, self.board_height, &self.board_cells)?;

        // Play the appropriate sound effect based on win or loss, if sound is enabled
        if self.prefs.sound_enabled {
            if win {
                Sound::WinGame.play(&hwnd.hinstance());
            } else {
                Sound::LoseGame.play(&hwnd.hinstance());
            }
        }
        self.game_status = StatusFlag::GameOver;

        // If the player won, set the bomb count to 0 and record the win if it's a personal best
        if win {
            self.bombs_left = 0;

            // If this win is a new personal best, update the best time and show the new record dialog
            if match self.prefs.game_type {
                GameType::Begin => self.timer.elapsed < self.prefs.beginner_time,
                GameType::Inter => self.timer.elapsed < self.prefs.inter_time,
                GameType::Expert => self.timer.elapsed < self.prefs.expert_time,
                GameType::Other => false,
            } {
                match self.prefs.game_type {
                    GameType::Begin => self.prefs.beginner_time = self.timer.elapsed,
                    GameType::Inter => self.prefs.inter_time = self.timer.elapsed,
                    GameType::Expert => self.prefs.expert_time = self.timer.elapsed,
                    GameType::Other => unreachable!(),
                }

                // TODO: Don't use PostMessage to do what could just be a function call
                unsafe {
                    let _ = hwnd.PostMessage(WndMsg::new(WM::APP, NEW_RECORD_DLG, 0));
                }
            }
        }

        Ok(())
    }

    /// Handle a user click on a single square.
    ///
    /// TODO: This function and `step_block` have a lot of overlap and could potentially be merged
    ///       into a single function that handles both regular clicks and chord operations, since
    ///       a chord is just a click with extra conditions.
    ///       Also, they are both only used in `do_button_1_up`, so they could potentially be merged
    ///       into that function as well.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// - `x` - The X coordinate of the clicked square.
    /// - `y` - The Y coordinate of the clicked square.
    /// # Returns
    /// - `Ok(())` - If the square was successfully processed.
    /// - `Err` - If an error occurred while drawing the square.
    fn step_square(&mut self, hwnd: &HWND, x: usize, y: usize) -> AnyResult<()> {
        let hdc = hwnd.GetDC()?;
        if self.board_cells[x][y].bomb {
            let visits = self.boxes_visited;
            if visits == 0 {
                // Ensure that the first clicked square is never a bomb
                for y_t in 0..=self.board_height {
                    for x_t in 0..=self.board_width {
                        if !self.board_cells[x_t][y_t].bomb {
                            self.board_cells[x][y].bomb = false;
                            self.board_cells[x_t][y_t].bomb = true;
                            self.step_box(&hdc, x, y)?;
                            return Ok(());
                        }
                    }
                }
            } else {
                // If a bomb was clicked, reveal it and end the game
                self.board_cells[x][y].block_type = BlockCell::Explode;
                self.game_over(hwnd, false)?;
            }
        } else {
            // If a non-bomb square was clicked, reveal it and check for a win
            self.step_box(&hdc, x, y)?;
            if self.check_win() {
                self.game_over(hwnd, true)?;
            }
        }

        Ok(())
    }

    /// Handle a chord action on a revealed number square.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// - `x_center` - The X coordinate of the center square.
    /// - `y_center` - The Y coordinate of the center square.
    /// # Returns
    /// - `Ok(())` - If the chord operation was successful.
    /// - `Err` - If an error occurred while drawing the board.
    fn step_block(&mut self, hwnd: &HWND, x_center: usize, y_center: usize) -> AnyResult<()> {
        let hdc = hwnd.GetDC()?;

        if !self.board_cells[x_center][y_center].visited
            || self.board_cells[x_center][y_center].block_type as u8
                != self.count_marks(x_center, y_center)
        {
            self.track_mouse(&hdc, usize::MAX - 2, usize::MAX - 2)?;
            return Ok(());
        }

        // If the conditions of a chord operation are met, reveal adjacent squares
        let mut lose = false;
        for y in y_center.saturating_sub(1)..=min(y_center + 1, self.board_height) {
            for x in x_center.saturating_sub(1)..=min(x_center + 1, self.board_width) {
                // Skip flagged squares
                if self.board_cells[x][y].block_type == BlockCell::Flagged {
                    continue;
                }

                if self.board_cells[x][y].bomb {
                    // If a flag was incorrectly placed, and a bomb is revealed, the player loses
                    lose = true;
                    self.board_cells[x][y].block_type = BlockCell::Explode;
                } else {
                    self.step_box(&hdc, x, y)?;
                }
            }
        }

        if lose {
            self.game_over(hwnd, false)?;
        } else if self.check_win() {
            self.game_over(hwnd, true)?;
        }

        Ok(())
    }

    /// Invert the visual state of a box to show it as pressed or released.
    /// - Boxes are pushed down while the left mouse button is pressed over them.
    /// - Boxes are restored to their raised state when the left mouse button is released or the cursor is no longer over them.
    /// # Arguments
    /// - `x` - The X coordinate of the box.
    /// - `y` - The Y coordinate of the box.
    const fn invert_box(&mut self, x: usize, y: usize) {
        let blk = self.board_cells[x][y].block_type;
        self.board_cells[x][y].block_type = match blk {
            // Push the box down by changing the block type to the corresponding "down" version
            BlockCell::GuessUp => BlockCell::GuessDown,
            BlockCell::BlankUp => BlockCell::Blank,
            // Restore the block back to its raised state
            BlockCell::GuessDown => BlockCell::GuessUp,
            BlockCell::Blank => BlockCell::BlankUp,
            _ => blk,
        };
    }

    /// Reset the game field to its initial blank state and rebuild the border.
    pub(crate) fn clear_field(&mut self) {
        self.board_cells
            .iter_mut()
            .flatten()
            .for_each(|b| *b = BlockInfo::from(BlockCell::BlankUp));
    }

    /// Handle the per-second game timer tick.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// # Returns
    /// - `Ok(())` - If the timer was successfully updated.
    /// - `Err` - If an error occurred while updating the display.
    pub(crate) fn do_timer(&mut self, hwnd: &HWND) -> AnyResult<()> {
        if self.timer.tick() {
            self.grafix
                .draw_timer(hwnd.GetDC()?.deref(), self.timer.elapsed)?;
            if self.prefs.sound_enabled {
                Sound::Tick.play(&hwnd.hinstance());
            }
        }
        Ok(())
    }
}

impl WinMineMainWindow {
    /// Start a new game by resetting globals, randomizing bombs, and resizing the window if the board changed.
    ///
    /// TODO: Move this into `GameState`.
    ///       Moving this into `GameState` is currently blocked by the function `adjust_window`.
    /// # Arguments
    /// - `hwnd` - Handle to the main window.
    /// # Returns
    /// - `Ok(())` - If the game was successfully started.
    /// - `Err` - If an error occurred while resizing or updating the display.
    pub(crate) fn start_game(&self) -> AnyResult<()> {
        let mut state = self.state.write();
        let x_prev = state.board_width + 1;
        let y_prev = state.board_height + 1;

        let f_adjust = if state.prefs.width != x_prev || state.prefs.height != y_prev {
            AdjustFlag::ResizeAndRedraw
        } else {
            AdjustFlag::Redraw
        };

        // Update the board dimensions based on the current preferences.
        // 1 is subtracted from each dimension to make it zero-indexed.
        state.board_width = state.prefs.width - 1;
        state.board_height = state.prefs.height - 1;

        // Reset the board to a blank state
        state.clear_field();
        state.btn_face_state = ButtonSprite::Happy;
        state.timer.reset();

        // Randomly place bombs on the board until the total number of bombs matches the number specified in preferences
        state.total_bombs = state.prefs.mines;
        let mut bombs = state.prefs.mines;
        while bombs > 0 {
            let mut x;
            let mut y;
            loop {
                // Select a random position on the board
                let width = state.prefs.width as u32;
                let height = state.prefs.height as u32;
                x = state.rng.rnd(width) as usize;
                y = state.rng.rnd(height) as usize;
                // If there is not already a bomb at that position, place a bomb there
                if !state.board_cells[x][y].bomb {
                    break;
                }
            }
            state.board_cells[x][y].bomb = true;
            bombs -= 1;
        }

        state.bombs_left = state.prefs.mines;
        state.boxes_visited = 0;
        state.boxes_to_win =
            (state.prefs.width * state.prefs.height) as u16 - state.prefs.mines as u16;
        state.game_status = StatusFlag::Play;

        state
            .grafix
            .draw_bomb_count(self.wnd.hwnd().GetDC()?.deref(), state.prefs.mines)?;

        // Drop the write lock before calling `adjust_window` since it also needs to acquire a write lock to update the board state for redrawing.
        drop(state);

        self.adjust_window(f_adjust)?;

        Ok(())
    }
}

impl GameState {
    /// Track mouse movement over the board and provide visual feedback.
    /// # Arguments
    /// - `hdc` - Handle to the device context to draw on.
    /// - `x_new` - The new X coordinate of the mouse.
    /// - `y_new` - The new Y coordinate of the mouse.
    /// # Returns
    /// - `Ok(())` - If the mouse tracking was successfully handled and the board was updated.
    /// - `Err` - If an error occurred while drawing the board or if getting the device context failed.
    fn track_mouse(&mut self, hdc: &ReleaseDCGuard, x_new: usize, y_new: usize) -> AnyResult<()> {
        // No change in position; nothing to do
        if x_new == self.cursor_x && y_new == self.cursor_y {
            return Ok(());
        }

        let y_max = self.board_height;
        let x_max = self.board_width;

        // Check if a chord operation is active and the game is not in a non-play state (e.g. paused or game over)
        if self.chord_active && self.game_status.contains(StatusFlag::Play) {
            // Determine if the old and new positions are within range
            let valid_new = self.in_range(x_new, y_new);
            let valid_old = self.in_range(self.cursor_x, self.cursor_y);

            // If the old position is valid, pop up boxes in the previous area
            if valid_old {
                // Determine the 3x3 area around the old cursor position
                let y_old_min = self.cursor_y.saturating_sub(1);
                let y_old_max = min(self.cursor_y + 1, y_max);
                let x_old_min = self.cursor_x.saturating_sub(1);
                let x_old_max = min(self.cursor_x + 1, x_max);
                // Iterate over the old 3x3 area from left to right, top to bottom
                for y in y_old_min..=y_old_max {
                    for x in x_old_min..=x_old_max {
                        // Only pop up boxes that are not visited
                        if !self.board_cells[x][y].visited {
                            // Restore the box to its raised state
                            self.invert_box(x, y);
                            self.grafix.draw_block(hdc, x, y, &self.board_cells)?;
                        }
                    }
                }
            }

            // If the new position is valid, push down boxes in the new area
            if valid_new {
                // Determine the 3x3 area around the new cursor position
                let y_cur_min = y_new.saturating_sub(1);
                let y_cur_max = min(y_new + 1, y_max);
                let x_cur_min = x_new.saturating_sub(1);
                let x_cur_max = min(x_new + 1, x_max);
                // Iterate over the new 3x3 area from left to right, top to bottom
                for y in y_cur_min..=y_cur_max {
                    for x in x_cur_min..=x_cur_max {
                        // Only push down boxes that are not visited
                        if !self.board_cells[x][y].visited {
                            // Depress the box visually
                            self.invert_box(x, y);
                            self.grafix.draw_block(hdc, x, y, &self.board_cells)?;
                        }
                    }
                }
            }
        } else {
            // Otherwise, handle single-box push/pop
            // Check if the old cursor position is in range and not yet visited
            if self.in_range(self.cursor_x, self.cursor_y)
                && !self.board_cells[self.cursor_x][self.cursor_y].visited
            {
                // Restore the old box to its raised state
                self.invert_box(self.cursor_x, self.cursor_y);
                self.grafix
                    .draw_block(hdc, self.cursor_x, self.cursor_y, &self.board_cells)?;
            }
            // Check if the new cursor position is in range, not yet visited, and not flagged as a bomb
            if self.in_range(x_new, y_new)
                && !self.board_cells[x_new][y_new].visited
                && self.board_cells[x_new][y_new].block_type != BlockCell::Flagged
            {
                // Depress the new box visually
                self.invert_box(x_new, y_new);
                self.grafix
                    .draw_block(hdc, x_new, y_new, &self.board_cells)?;
            }
        }
        // Store the new cursor position
        self.cursor_x = x_new;
        self.cursor_y = y_new;
        Ok(())
    }

    /// Pause the game by silencing audio, storing the timer state, and setting the pause flag.
    pub(crate) fn pause_game(&mut self) {
        Sound::reset();

        if !self.game_status.contains(StatusFlag::Pause)
            && self.game_status.contains(StatusFlag::Play)
        {
            self.timer.pause();
        }

        self.game_status.insert(StatusFlag::Pause);
    }

    /// Resume the game by restoring the timer state and clearing the pause flag from the game status.
    pub(crate) fn resume_game(&mut self) {
        if self.game_status.contains(StatusFlag::Play) {
            self.timer.resume();
        }
        self.game_status.remove(StatusFlag::Pause);
    }
}
