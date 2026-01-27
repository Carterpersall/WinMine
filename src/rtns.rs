//! Handlers for the core game logic and state management.
//! This includes board representation, game status tracking, and related utilities.

use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use winsafe::co::WM;
use winsafe::msg::WndMsg;
use winsafe::{AnyResult, HWND, POINT, prelude::*};

use crate::globals::{CHORD_ACTIVE, GAME_STATUS, StatusFlag};
use crate::grafix::{
    ButtonSprite, display_button, draw_block, draw_bomb_count, draw_grid, draw_timer, load_bitmaps,
};
use crate::pref::{CCH_NAME_MAX, GameType, MenuMode, Pref, SoundState};
use crate::sound::Tune;
use crate::util::rnd;
use crate::winmine::{NEW_RECORD_DLG, WinMineMainWindow};

/// Encoded board values used to track each tile state.
///
/// These values are used to get the visual representation of each cell, in reverse order.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum BlockCell {
    /// A blank cell with no adjacent bombs.
    Blank = 0,
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
    BombUp = 14,
    /// A blank cell in the raised state.
    BlankUp = 15,
    /// A border cell surrounding the playable area.
    ///
    /// TODO: I don't see this in the blocks.bmp file; is it actually used?
    Border = 16,
}

/// Bit masks applied to the packed board cell value.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum BlockMask {
    /// Marks a cell that contains a bomb.
    Bomb = 0x80,
    /// Marks a cell that has been visited.
    Visit = 0x40,
    /// Covers all flag bits in a board cell.
    Flags = 0xE0,
    /// Covers the data bits (adjacent bomb count) in a cell.
    Data = 0x1F,
    /// Clears the bomb bit from a cell value.
    NotBomb = 0x7F,
}

/// Maximum number of board cells (27 columns by 32 rows including border).
pub const C_BLK_MAX: usize = 27 * 32;
/// Upper bound on the flood-fill work queue used for empty regions.
const I_STEP_MAX: usize = 100;

/// Timer identifier used for the per-second gameplay timer.
pub const ID_TIMER: usize = 1;

/// Window-adjustment flags.
///
/// TODO: Are these flags needed?
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum AdjustFlag {
    Resize = 0x02,
    Display = 0x04,
}

/// Shift applied when converting x/y to the packed board index.
pub const BOARD_INDEX_SHIFT: usize = 5;

/// Current preferences stored in a global Mutex.
static PREFERENCES: OnceLock<Mutex<Pref>> = OnceLock::new();

/// Retrieve the global preferences mutex.
/// # Returns
/// A reference to the global preferences mutex.
///
/// TODO: Handle locking as well.
pub fn preferences_mutex() -> std::sync::MutexGuard<'static, Pref> {
    match PREFERENCES
        .get_or_init(|| {
            Mutex::new(Pref {
                game_type: GameType::Begin,
                mines: 0,
                height: 0,
                width: 0,
                wnd_x_pos: 0,
                wnd_y_pos: 0,
                sound_state: SoundState::Off,
                mark_enabled: false,
                timer: false,
                menu_mode: MenuMode::AlwaysOn,
                color: false,
                best_times: [0; 3],
                beginner_name: String::with_capacity(CCH_NAME_MAX),
                inter_name: String::with_capacity(CCH_NAME_MAX),
                expert_name: String::with_capacity(CCH_NAME_MAX),
            })
        })
        .lock()
    {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Represents the current state of the game.
#[derive(Clone)]
pub struct GameState {
    /// Current board width in cells (excluding border)
    pub board_width: i32,
    /// Current board height in cells (excluding border)
    pub board_height: i32,
    /// Current button face sprite
    pub btn_face_state: ButtonSprite,
    /// Current number of bombs left to mark
    ///
    /// Note: The bomb count can go negative if the user marks more squares than there are bombs.
    pub bombs_left: i16,
    /// Current elapsed time in seconds.
    ///
    /// The timer should never exceed 999 seconds, so u16 is sufficient.
    pub secs_elapsed: u16,
    /// Number of visited boxes (revealed non-bomb cells).
    ///
    /// Note: Maximum value is 2<sup>16</sup>, or a 256 x 256 board with no bombs.
    pub boxes_visited: u16,
    /// Current cursor position in board coordinates
    pub cursor_pos: POINT,
    /// Packed board cell values stored row-major including border
    ///
    /// TODO: Replace with a better data structure, perhaps using a struct?
    /// TODO: Use an enum for the cell values instead of i8
    pub board_cells: [i8; C_BLK_MAX],
    /// Initial number of bombs at the start of the game
    pub total_bombs: i16,
    /// Total number of visited boxes needed to win
    pub boxes_to_win: u16,
    /// Indicates whether the game timer is running
    pub timer_running: bool,
}

impl GameState {
    /// Creates a new default GameState
    pub fn new() -> Self {
        Self {
            board_width: 0,
            board_height: 0,
            btn_face_state: ButtonSprite::Happy,
            bombs_left: 0,
            secs_elapsed: 0,
            boxes_visited: 0,
            cursor_pos: POINT::default(),
            board_cells: [BlockCell::BlankUp as i8; C_BLK_MAX],
            total_bombs: 0,
            boxes_to_win: 0,
            timer_running: false,
        }
    }
}

/// Previous timer running state used to detect changes
static F_OLD_TIMER_STATUS: AtomicBool = AtomicBool::new(false);

/// Calculate the board index for the given coordinates.
///
/// TODO: Return Result instead of Option
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// An option containing the board index if valid, or None if out of range.
pub const fn board_index(x: i32, y: i32) -> Option<usize> {
    // Calculate the offset in the packed board array.
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    if offset < 0 {
        return None;
    }
    let idx = offset as usize;
    if idx < C_BLK_MAX { Some(idx) } else { None }
}

impl GameState {
    /// Initialize local graphics resources and reset the minefield before the game starts.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// `Ok(())` if successful, or an error if loading resources failed.
    pub fn init_game(&mut self, hwnd: &HWND) -> AnyResult<()> {
        load_bitmaps(hwnd)?;
        self.clear_field();
        Ok(())
    }

    /// Retrieve the value of a block at the specified coordinates.
    ///
    /// TODO: Return a `BlockCell` instead of an u8
    /// TODO: Does this function need to exist?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// The value of the block, or 0 if out of range.
    fn block_value(&self, x: i32, y: i32) -> u8 {
        board_index(x, y)
            .and_then(|idx| self.board_cells.get(idx).copied())
            .unwrap_or(BlockCell::Blank as i8) as u8
    }

    /// Set the value of a block at the specified coordinates.
    ///
    /// TODO: Make value a `BlockCell` instead of u8
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// * `value` - The value to set.
    const fn set_block_value(&mut self, x: i32, y: i32, value: u8) {
        if let Some(idx) = board_index(x, y) {
            let prev = self.board_cells[idx] as u8;

            // Preserve existing flag bits, but allow callers to explicitly set the Visit bit.
            // (Bomb placement is handled separately via set_bomb/clear_bomb.)
            let flags = (prev & BlockMask::Flags as u8) | (value & BlockMask::Visit as u8);
            let data = value & BlockMask::Data as u8;
            self.board_cells[idx] = (flags | data) as i8;
        }
    }

    /// Set a block as a border at the specified coordinates.
    ///
    /// TODO: Is this function needed?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn set_border(&mut self, x: i32, y: i32) {
        if let Some(idx) = board_index(x, y) {
            self.board_cells[idx] = BlockCell::Border as i8;
        }
    }

    /// Set a block as containing a bomb at the specified coordinates.
    ///
    /// TODO: Could these functions be moved under a BlockMask impl?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn set_bomb(&mut self, x: i32, y: i32) {
        if let Some(idx) = board_index(x, y) {
            let prev = self.board_cells[idx];
            self.board_cells[idx] = prev | BlockMask::Bomb as i8;
        }
    }

    /// Clear the bomb flag from a block at the specified coordinates.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn clear_bomb(&mut self, x: i32, y: i32) {
        if let Some(idx) = board_index(x, y) {
            let prev = self.board_cells[idx] as u8;
            self.board_cells[idx] = (prev & BlockMask::NotBomb as u8) as i8;
        }
    }

    /// Check if a block at the specified coordinates contains a bomb.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the block contains a bomb, `false` otherwise.
    fn is_bomb(&self, x: i32, y: i32) -> bool {
        (self.block_value(x, y) & BlockMask::Bomb as u8) != 0
    }

    /// Check if a block at the specified coordinates has been visited.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the block has been visited, `false` otherwise.
    fn is_visit(&self, x: i32, y: i32) -> bool {
        (self.block_value(x, y) & BlockMask::Visit as u8) != 0
    }

    /// Check if a block at the specified coordinates is currently flagged as a bomb.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the block is guessed to contain a bomb, `false` otherwise.
    fn block_flagged(&self, x: i32, y: i32) -> bool {
        self.block_value(x, y) & BlockMask::Data as u8 == BlockCell::BombUp as u8
    }

    /// Check if a block at the specified coordinates is currently guessed (marked with a ?).
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the block is guessed to be marked, `false` otherwise.
    fn block_guessed(&self, x: i32, y: i32) -> bool {
        self.block_value(x, y) & BlockMask::Data as u8 == BlockCell::GuessUp as u8
    }

    /// Check if the given coordinates are within the valid range of the board.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the coordinates are within range, `false` otherwise.
    const fn in_range(&self, x: i32, y: i32) -> bool {
        x > 0 && y > 0 && x <= self.board_width && y <= self.board_height
    }

    /// Set a raw block value at the specified coordinates, preserving only data and visit bits.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// * `block` - The raw block value to set.
    const fn set_raw_block(&mut self, x: i32, y: i32, block: u8) {
        // Keep only the data bits plus the Visit bit (when present).
        let masked = block & (BlockMask::Data as u8 | BlockMask::Visit as u8);
        self.set_block_value(x, y, masked);
    }

    /// Get the data bits of a block at the specified coordinates.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// The data bits of the block.
    fn block_data(&self, x: i32, y: i32) -> u8 {
        self.block_value(x, y) & BlockMask::Data as u8
    }

    /// Check if the player has won the game.
    ///
    /// TODO: Remove this function.
    /// # Returns
    /// `true` if the player has won, `false` otherwise.
    const fn check_win(&self) -> bool {
        self.boxes_visited == self.boxes_to_win
    }

    /// Reveal all bombs on the board and mark incorrect guesses.
    ///
    /// This is called when the game ends to show the final board state.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `cell` - The block cell type to use for revealed bombs.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn show_bombs(&mut self, hwnd: &HWND, cell: BlockCell) -> AnyResult<()> {
        for y in 1..=self.board_height {
            for x in 1..=self.board_width {
                if !self.is_visit(x, y) {
                    if self.is_bomb(x, y) {
                        if !self.block_flagged(x, y) {
                            self.set_raw_block(x, y, cell as u8);
                        }
                    } else if self.block_flagged(x, y) {
                        self.set_raw_block(x, y, BlockCell::Wrong as u8);
                    }
                }
            }
        }
        draw_grid(
            &hwnd.GetDC()?,
            self.board_width,
            self.board_height,
            &self.board_cells,
        )?;
        Ok(())
    }

    /// Count the number of adjacent marked squares around the specified coordinates.
    /// # Arguments
    /// * `x_center` - The X coordinate of the center square.
    /// * `y_center` - The Y coordinate of the center square.
    /// # Returns
    /// The count of adjacent marked squares (maximum 8).
    fn count_marks(&self, x_center: i32, y_center: i32) -> u8 {
        let mut count = 0;
        for y in (y_center - 1)..=(y_center + 1) {
            for x in (x_center - 1)..=(x_center + 1) {
                if self.block_flagged(x, y) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Update the button face based on the game result.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `win` - `true` if the player has won, `false` otherwise.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn update_button_for_result(&mut self, hwnd: &HWND, win: bool) -> AnyResult<()> {
        let state = if win {
            ButtonSprite::Win
        } else {
            ButtonSprite::Lose
        };
        self.btn_face_state = state;
        display_button(hwnd, state)?;
        Ok(())
    }

    /// Record a new win time if it is a personal best.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    fn record_win_if_needed(&mut self, hwnd: &HWND) {
        let mut prefs = preferences_mutex();
        let game = prefs.game_type;
        if game != GameType::Other {
            let game_idx = game as usize;
            if game_idx < prefs.best_times.len() && self.secs_elapsed < prefs.best_times[game_idx] {
                {
                    prefs.best_times[game_idx] = self.secs_elapsed;
                }

                if hwnd.as_opt().is_some() {
                    unsafe {
                        let _ = hwnd.PostMessage(WndMsg::new(WM::APP, NEW_RECORD_DLG, 0));
                    }
                }
            }
        }
    }

    /// Change a single block's value and repaint it immediately.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// * `block` - The new block value.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn change_blk(&mut self, hwnd: &HWND, x: i32, y: i32, block: u8) -> AnyResult<()> {
        self.set_raw_block(x, y, block);
        draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
        Ok(())
    }

    /// Enqueue a square for flood-fill processing if it is empty.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `queue` - The flood-fill work queue.
    /// * `tail` - The current tail index of the queue.
    /// * `x` - The X coordinate of the square.
    /// * `y` - The Y coordinate of the square.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn step_xy(
        &mut self,
        hwnd: &HWND,
        queue: &mut [(i32, i32); I_STEP_MAX],
        tail: &mut usize,
        x: i32,
        y: i32,
    ) -> AnyResult<()> {
        // Visit a square; enqueue it when empty so we flood-fill neighbors later.
        if let Some(idx) = board_index(x, y) {
            let mut blk = self.board_cells[idx] as u8;
            if (blk & BlockMask::Visit as u8) != 0 {
                return Ok(());
            }

            let data = blk & BlockMask::Data as u8;
            if data == BlockCell::Border as u8 || data == BlockCell::BombUp as u8 {
                return Ok(());
            }

            self.boxes_visited += 1;
            let mut bombs = 0;
            for y_n in (y - 1)..=(y + 1) {
                for x_n in (x - 1)..=(x + 1) {
                    if let Some(nidx) = board_index(x_n, y_n) {
                        let cell = self.board_cells[nidx] as u8;
                        if (cell & BlockMask::Bomb as u8) != 0 {
                            bombs += 1;
                        }
                    }
                }
            }
            blk = BlockMask::Visit as u8 | ((bombs as u8) & BlockMask::Data as u8);
            self.board_cells[idx] = blk as i8;
            draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;

            if bombs == 0 && *tail < I_STEP_MAX {
                queue[*tail] = (x, y);
                *tail += 1;
            }
        }
        Ok(())
    }

    /// Flood-fill contiguous empty squares starting from (x, y).
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x` - X coordinate of the starting square
    /// * `y` - Y coordinate of the starting square
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn step_box(&mut self, hwnd: &HWND, x: i32, y: i32) -> AnyResult<()> {
        let mut queue = [(0, 0); I_STEP_MAX];
        let mut head = 0usize;
        let mut tail = 0usize;

        self.step_xy(hwnd, &mut queue, &mut tail, x, y)?;

        while head < tail {
            let (sx, sy) = queue[head];
            head += 1;

            let mut ty = sy - 1;
            self.step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty)?;
            self.step_xy(hwnd, &mut queue, &mut tail, sx, ty)?;
            self.step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty)?;
            ty += 1;
            self.step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty)?;
            self.step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty)?;

            ty += 1;
            self.step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty)?;
            self.step_xy(hwnd, &mut queue, &mut tail, sx, ty)?;
            self.step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty)?;
        }
        Ok(())
    }

    /// Handle the end of the game - stopping the timer, revealing bombs, updating the face, and recording wins.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `win` - `true` if the player has won, `false` otherwise
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn game_over(&mut self, hwnd: &HWND, win: bool) -> AnyResult<()> {
        self.timer_running = false;
        self.update_button_for_result(hwnd, win)?;
        self.show_bombs(
            hwnd,
            if win {
                BlockCell::BombUp
            } else {
                BlockCell::BombDown
            },
        )?;
        if win {
            self.bombs_left = 0;
            Tune::WinGame.play(&hwnd.hinstance());
        } else {
            Tune::LoseGame.play(&hwnd.hinstance());
        }
        GAME_STATUS.store(StatusFlag::Demo as i32, Ordering::Relaxed);

        if win {
            self.record_win_if_needed(hwnd);
        }

        Ok(())
    }

    /// Handle a user click on a single square (first-click safety included).
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x` - The X coordinate of the clicked square.
    /// * `y` - The Y coordinate of the clicked square.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn step_square(&mut self, hwnd: &HWND, x: i32, y: i32) -> AnyResult<()> {
        if self.is_bomb(x, y) {
            let visits = self.boxes_visited;
            if visits == 0 {
                for y_t in 1..self.board_height {
                    for x_t in 1..self.board_width {
                        if !self.is_bomb(x_t, y_t) {
                            self.clear_bomb(x, y);
                            self.set_bomb(x_t, y_t);
                            self.step_box(hwnd, x, y)?;
                            return Ok(());
                        }
                    }
                }
            } else {
                self.change_blk(
                    hwnd,
                    x,
                    y,
                    BlockMask::Visit as u8 | BlockCell::Explode as u8,
                )?;
                self.game_over(hwnd, false)?;
            }
        } else {
            self.step_box(hwnd, x, y)?;
            if self.check_win() {
                self.game_over(hwnd, true)?;
            }
        }

        Ok(())
    }

    /// Handle a chord action on a revealed number square.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x_center` - The X coordinate of the center square.
    /// * `y_center` - The Y coordinate of the center square.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn step_block(&mut self, hwnd: &HWND, x_center: i32, y_center: i32) -> AnyResult<()> {
        if !self.is_visit(x_center, y_center)
            || self.block_data(x_center, y_center) != self.count_marks(x_center, y_center)
        {
            self.track_mouse(hwnd, -2, -2)?;
            return Ok(());
        }

        let mut lose = false;
        for y in (y_center - 1)..=(y_center + 1) {
            for x in (x_center - 1)..=(x_center + 1) {
                if self.block_flagged(x, y) {
                    continue;
                }

                if self.is_bomb(x, y) {
                    lose = true;
                    self.change_blk(
                        hwnd,
                        x,
                        y,
                        BlockMask::Visit as u8 | BlockCell::Explode as u8,
                    )?;
                } else {
                    self.step_box(hwnd, x, y)?;
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

    /// Handle a user guess (flag or question mark) on a square.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x` - The X coordinate of the square.
    /// * `y` - The Y coordinate of the square.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    pub fn make_guess(&mut self, hwnd: &HWND, x: i32, y: i32) -> AnyResult<()> {
        // Cycle through blank -> flag -> question mark states depending on preferences.

        // Return if the square is out of range or already visited.
        if !self.in_range(x, y) || self.is_visit(x, y) {
            return Ok(());
        }

        let allow_marks = { preferences_mutex().mark_enabled };

        // If currently flagged
        let block = if self.block_flagged(x, y) {
            // Increment the bomb count
            self.bombs_left += 1;
            draw_bomb_count(&hwnd.GetDC()?, self.bombs_left)?;

            // If marks are allowed, change to question mark; otherwise, change to blank
            if allow_marks {
                BlockCell::GuessUp as u8
            } else {
                BlockCell::BlankUp as u8
            }
        } else if self.block_guessed(x, y) {
            // If currently marked with a question mark, change to blank
            // No need to update the bomb count since the guess mark doesn't affect it
            BlockCell::BlankUp as u8
        } else {
            // Currently blank; change to flagged and decrement bomb count
            self.bombs_left -= 1;
            draw_bomb_count(&hwnd.GetDC()?, self.bombs_left)?;
            BlockCell::BombUp as u8
        };

        // Update the block visually
        self.change_blk(hwnd, x, y, block)?;

        // If the user has flagged the last bomb, they have won
        if self.block_flagged(x, y) && self.check_win() {
            self.game_over(hwnd, true)?;
        }

        Ok(())
    }

    /// Depress a box visually.
    ///
    /// Boxes are pushed down while the left mouse button is pressed over them.
    ///
    /// TODO: Can `push_box_down` and `pop_box_up` be merged?
    /// # Arguments
    /// * `x` - The X coordinate of the box.
    /// * `y` - The Y coordinate of the box.
    fn push_box_down(&mut self, x: i32, y: i32) {
        let mut blk = self.block_data(x, y);
        blk = match blk {
            b if b == BlockCell::GuessUp as u8 => BlockCell::GuessDown as u8,
            b if b == BlockCell::BlankUp as u8 => BlockCell::Blank as u8,
            _ => blk,
        };
        self.set_raw_block(x, y, blk);
    }

    /// Restore a depressed box visually.
    ///
    /// Boxes are restored to their raised state when the left mouse button is released or the cursor is no longer over them.
    /// # Arguments
    /// * `x` - The X coordinate of the box.
    /// * `y` - The Y coordinate of the box.
    fn pop_box_up(&mut self, x: i32, y: i32) {
        let mut blk = self.block_data(x, y);
        blk = match blk {
            b if b == BlockCell::GuessDown as u8 => BlockCell::GuessUp as u8,
            b if b == BlockCell::Blank as u8 => BlockCell::BlankUp as u8,
            _ => blk,
        };
        self.set_raw_block(x, y, blk);
    }

    /// Check if a given coordinate is within range, not visited, and not guessed as a bomb.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the coordinate is valid for flood-fill, `false` otherwise.
    fn in_range_step(&mut self, x: i32, y: i32) -> bool {
        self.in_range(x, y) && !self.is_visit(x, y) && !self.block_flagged(x, y)
    }

    /// Reset the game field to its initial blank state and rebuild the border.
    pub fn clear_field(&mut self) {
        self.board_cells
            .iter_mut()
            .for_each(|b| *b = BlockCell::BlankUp as i8);

        let x_max = self.board_width;
        let y_max = self.board_height;

        for x in 0..=(x_max + 1) {
            self.set_border(x, 0);
            self.set_border(x, y_max + 1);
        }
        for y in 0..=(y_max + 1) {
            self.set_border(0, y);
            self.set_border(x_max + 1, y);
        }
    }

    /// Handle the per-second game timer tick.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// An `Ok(())` if successful, or an error if updating the display failed
    pub fn do_timer(&mut self, hwnd: &HWND) -> AnyResult<()> {
        if self.timer_running && self.secs_elapsed < 999 {
            self.secs_elapsed += 1;
            draw_timer(&hwnd.GetDC()?, self.secs_elapsed)?;
            Tune::Tick.play(&hwnd.hinstance());
        }
        Ok(())
    }
}

impl WinMineMainWindow {
    /// Start a new game by resetting globals, randomizing bombs, and resizing the window if the board changed.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// An `Ok(())` if successful, or an error if resizing or updating the display failed.
    pub fn start_game(&self) -> AnyResult<()> {
        self.state.write().timer_running = false;

        let x_prev = self.state.read().board_width;
        let y_prev = self.state.read().board_height;

        let (pref_width, pref_height, total_bombs) = {
            let prefs = preferences_mutex();
            (prefs.width, prefs.height, prefs.mines)
        };

        let f_adjust = if pref_width != x_prev || pref_height != y_prev {
            AdjustFlag::Resize as i32 | AdjustFlag::Display as i32
        } else {
            AdjustFlag::Display as i32
        };

        self.state.write().board_width = pref_width;
        self.state.write().board_height = pref_height;

        self.state.write().clear_field();
        self.state.write().btn_face_state = ButtonSprite::Happy;
        self.state.write().timer_running = false;

        self.state.write().total_bombs = total_bombs;

        let width = self.state.read().board_width;
        let height = self.state.read().board_height;

        let mut bombs = total_bombs;
        while bombs > 0 {
            let mut x;
            let mut y;
            loop {
                x = rnd(width as u32) + 1;
                y = rnd(height as u32) + 1;
                if !self.state.read().is_bomb(x as i32, y as i32) {
                    break;
                }
            }
            self.state.write().set_bomb(x as i32, y as i32);
            bombs -= 1;
        }

        self.state.write().secs_elapsed = 0;
        self.state.write().bombs_left = total_bombs;
        self.state.write().boxes_visited = 0;
        self.state.write().boxes_to_win = (width * height) as u16 - total_bombs as u16;
        GAME_STATUS.store(StatusFlag::Play as i32, Ordering::Relaxed);

        draw_bomb_count(&self.wnd.hwnd().GetDC()?, self.state.read().bombs_left)?;

        self.adjust_window(f_adjust)?;

        Ok(())
    }
}

impl GameState {
    /// Track mouse movement over the board and provide visual feedback.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x_new` - The new X coordinate of the mouse.
    /// * `y_new` - The new Y coordinate of the mouse.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    pub fn track_mouse(&mut self, hwnd: &HWND, x_new: i32, y_new: i32) -> AnyResult<()> {
        let pt_new = POINT { x: x_new, y: y_new };
        // No change in position; nothing to do
        if pt_new == self.cursor_pos {
            return Ok(());
        }

        let y_max = self.board_height;
        let x_max = self.board_width;

        // Check if a chord operation is active
        if CHORD_ACTIVE.load(Ordering::Relaxed) {
            // Determine if the old and new positions are within range
            let valid_new = self.in_range(pt_new.x, pt_new.y);
            let valid_old = self.in_range(self.cursor_pos.x, self.cursor_pos.y);

            // Determine the affected area (3x3 grid around old and new positions)
            let y_old_min = max(self.cursor_pos.y - 1, 1);
            let y_old_max = min(self.cursor_pos.y + 1, y_max);
            let y_cur_min = max(pt_new.y - 1, 1);
            let y_cur_max = min(pt_new.y + 1, y_max);
            let x_old_min = max(self.cursor_pos.x - 1, 1);
            let x_old_max = min(self.cursor_pos.x + 1, x_max);
            let x_cur_min = max(pt_new.x - 1, 1);
            let x_cur_max = min(pt_new.x + 1, x_max);

            // If the old position is valid, pop up boxes in the previous area
            if valid_old {
                // Iterate over the old 3x3 area from left to right, top to bottom
                for y in y_old_min..=y_old_max {
                    for x in x_old_min..=x_old_max {
                        // Only pop up boxes that are not visited
                        if !self.is_visit(x, y) {
                            // Restore the box to its raised state
                            self.pop_box_up(x, y);
                            draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
                        }
                    }
                }
            }

            // If the new position is valid, push down boxes in the new area
            if valid_new {
                // Iterate over the new 3x3 area from left to right, top to bottom
                for y in y_cur_min..=y_cur_max {
                    for x in x_cur_min..=x_cur_max {
                        // Only push down boxes that are not visited
                        if !self.is_visit(x, y) {
                            // Depress the box visually
                            self.push_box_down(x, y);
                            draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
                        }
                    }
                }
            }
        } else {
            // Otherwise, handle single-box push/pop
            // Check if the old cursor position is in range and not yet visited
            if self.in_range(self.cursor_pos.x, self.cursor_pos.y)
                && !self.is_visit(self.cursor_pos.x, self.cursor_pos.y)
            {
                // Restore the old box to its raised state
                self.pop_box_up(self.cursor_pos.x, self.cursor_pos.y);
                draw_block(
                    &hwnd.GetDC()?,
                    self.cursor_pos.x,
                    self.cursor_pos.y,
                    &self.board_cells,
                )?;
            }
            // Check if the new cursor position is in range and not yet visited
            if self.in_range(pt_new.x, pt_new.y) && self.in_range_step(pt_new.x, pt_new.y) {
                // Depress the new box visually
                self.push_box_down(pt_new.x, pt_new.y);
                draw_block(&hwnd.GetDC()?, pt_new.x, pt_new.y, &self.board_cells)?;
            }
        }
        // Store the new cursor position
        self.cursor_pos = pt_new;
        Ok(())
    }

    /// Handle a left-button release: start the timer, then either chord or step.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    pub fn do_button_1_up(&mut self, hwnd: &HWND) -> AnyResult<()> {
        // Get the current cursor position
        let x_pos = self.cursor_pos.x;
        let y_pos = self.cursor_pos.y;

        // Check if the cursor is within the valid range of the board
        if self.in_range(x_pos, y_pos) {
            // If the number of visits and elapsed seconds are both zero, the game has not started yet
            if self.boxes_visited == 0 && self.secs_elapsed == 0 {
                // Play the tick sound, display the initial time, and start the timer
                Tune::Tick.play(&hwnd.hinstance());
                self.secs_elapsed = 1;
                draw_timer(&hwnd.GetDC()?, self.secs_elapsed)?;
                self.timer_running = true;
                if let Some(hwnd) = hwnd.as_opt() {
                    hwnd.SetTimer(ID_TIMER, 1000, None)?;
                }
            }

            // If the game is not in play mode, reset the cursor position to a location off the board
            if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) == 0 {
                self.cursor_pos = POINT { x: -2, y: -2 };
            }

            // Determine whether to chord (select adjacent squares) or step (reveal a single square)
            if CHORD_ACTIVE.load(Ordering::Relaxed) {
                self.step_block(hwnd, x_pos, y_pos)?;
            } else if self.in_range_step(x_pos, y_pos) {
                self.step_square(hwnd, x_pos, y_pos)?;
            }
        }

        display_button(hwnd, self.btn_face_state)?;

        Ok(())
    }

    /// Pause the game by silencing audio, storing the timer state, and setting the pause flag.
    pub fn pause_game(&mut self) {
        SoundState::stop_all();

        if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Pause as i32)) == 0 {
            F_OLD_TIMER_STATUS.store(self.timer_running, Ordering::Relaxed);
        }
        if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) != 0 {
            self.timer_running = false;
        }

        GAME_STATUS.fetch_or(StatusFlag::Pause as i32, Ordering::Relaxed);
    }

    /// Resume the game by restoring the timer state and clearing the pause flag.
    pub fn resume_game(&mut self) {
        if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) != 0 {
            self.timer_running = F_OLD_TIMER_STATUS.load(Ordering::Relaxed);
        }
        GAME_STATUS.fetch_and(!(StatusFlag::Pause as i32), Ordering::Relaxed);
    }
}
