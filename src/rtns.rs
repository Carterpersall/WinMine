//! Handlers for the core game logic and state management.
//! This includes board representation, game status tracking, and related utilities.

use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use bitflags::bitflags;
use winsafe::co::WM;
use winsafe::msg::WndMsg;
use winsafe::{AnyResult, HWND, POINT, prelude::*};

use crate::grafix::{ButtonSprite, GrafixState};
use crate::pref::{CCH_NAME_MAX, GameType, Pref};
use crate::sound::Sound;
use crate::util::rnd;
use crate::winmine::{NEW_RECORD_DLG, WinMineMainWindow};

/// Encoded board values used to track each tile state.
///
/// These values are used to get the visual representation of each cell, in reverse order.
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum BlockCell {
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
    BombUp = 14,
    /// A blank cell in the raised state.
    BlankUp = 15,
    /// A border cell surrounding the playable area.
    ///
    /// TODO: I don't see this in the blocks.bmp file; is it actually used?
    Border = 16,
}

impl From<u8> for BlockCell {
    /// Convert a `u8` value to a `BlockCell` enum.
    /// # Arguments
    /// * `value` - The `u8` value to convert.
    /// # Returns
    /// The corresponding `BlockCell` enum variant.
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
            14 => BlockCell::BombUp,
            15 => BlockCell::BlankUp,
            16 => BlockCell::Border,
            _ => BlockCell::Blank,
        }
    }
}

/// Struct representing information about a single block on the board.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct BlockInfo {
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
    /// * `cell` - The `BlockCell` enum to convert.
    /// # Returns
    /// The corresponding `BlockInfo` struct.
    fn from(cell: BlockCell) -> Self {
        Self {
            bomb: false,
            visited: false,
            block_type: cell,
        }
    }
}

/// Maximum number of board cells
pub const MAX_X_BLKS: usize = 27;
pub const MAX_Y_BLKS: usize = 32;
/// Upper bound on the flood-fill work queue used for empty regions.
const I_STEP_MAX: usize = 100;

/// Timer identifier used for the per-second gameplay timer.
pub const ID_TIMER: usize = 1;

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
        ///
        /// TODO: Is this flag needed? The Pause flag may be sufficient.
        /// It is only read when the window is being moved, which we may not want to do anyways.
        const Minimized = 0b0100;
        /// Game is over (win or loss).
        const GameOver = 0b1000;
    }
}

/// Current preferences stored in a global Mutex.
static PREFERENCES: OnceLock<Mutex<Pref>> = OnceLock::new();

/// Retrieve the global preferences mutex.
/// # Returns
/// A reference to the global preferences mutex.
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
                sound_enabled: false,
                mark_enabled: false,
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
pub struct GameState {
    /// Graphics state containing bitmaps and rendering logic.
    pub grafix: GrafixState,
    /// Aggregated status flags defining the current game state.
    pub game_status: StatusFlag,
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
    /// Indicates whether a chord operation is currently active.
    ///
    /// A chord operation depresses a 3x3 area of cells around the cursor.
    ///
    /// A chord operation will begin if:
    /// - Both left and right buttons are held down, and the middle button is not held down
    /// - Only the middle button is held down
    /// - Shift is held _then_ left button is held down
    pub chord_active: bool,
    /// 2D Array representing the state of each cell on the board
    pub board_cells: [[BlockInfo; MAX_X_BLKS]; MAX_Y_BLKS],
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
            grafix: GrafixState::default(),
            game_status: StatusFlag::Minimized | StatusFlag::GameOver,
            board_width: 0,
            board_height: 0,
            btn_face_state: ButtonSprite::Happy,
            bombs_left: 0,
            secs_elapsed: 0,
            boxes_visited: 0,
            cursor_pos: POINT::default(),
            chord_active: false,
            board_cells: [[BlockInfo {
                bomb: false,
                visited: false,
                block_type: BlockCell::BlankUp,
            }; MAX_X_BLKS]; MAX_Y_BLKS],
            total_bombs: 0,
            boxes_to_win: 0,
            timer_running: false,
        }
    }
}

/// Previous timer running state used to detect changes
static F_OLD_TIMER_STATUS: AtomicBool = AtomicBool::new(false);

impl GameState {
    /// Initialize local graphics resources and reset the minefield before the game starts.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// `Ok(())` if successful, or an error if loading resources failed.
    pub fn init_game(&mut self, hwnd: &HWND) -> AnyResult<()> {
        self.grafix.load_bitmaps(hwnd)?;
        self.clear_field();
        Ok(())
    }

    /// Retrieve the value of a block at the specified coordinates.
    ///
    /// TODO: Does this function need to exist?
    /// TODO: Should this return an `Option`/`Result` instead of a default value?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// The `BlockInfo` value at the specified coordinates, or a blank block if out of range.
    const fn block_value(&self, x: i32, y: i32) -> BlockInfo {
        self.board_cells[x as usize][y as usize]
    }

    /// Set the value of a block at the specified coordinates.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// * `value` - The `BlockInfo` value to set.
    ///
    /// TODO: Should this function be split into separate functions for each field?
    ///       Or should it be merged with the bomb setters? Or should it be removed entirely?
    const fn set_block_value(&mut self, x: i32, y: i32, value: BlockInfo) {
        // The bomb flag is preserved since it is handled separately.
        self.board_cells[x as usize][y as usize] = BlockInfo {
            bomb: self.board_cells[x as usize][y as usize].bomb,
            visited: value.visited,
            block_type: value.block_type,
        }
    }

    /// Set a block as a border at the specified coordinates.
    ///
    /// TODO: Is this function needed?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn set_border(&mut self, x: i32, y: i32) {
        self.board_cells[x as usize][y as usize].block_type = BlockCell::Border;
    }

    /// Set a block as containing a bomb at the specified coordinates.
    ///
    /// TODO: Could these functions be moved under a BlockInfo impl?
    ///       Or should this function just be removed entirely?
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn set_bomb(&mut self, x: i32, y: i32) {
        self.board_cells[x as usize][y as usize].bomb = true;
    }

    /// Clear the bomb flag from a block at the specified coordinates.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    const fn clear_bomb(&mut self, x: i32, y: i32) {
        self.board_cells[x as usize][y as usize].bomb = false;
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

    /// Check if the player has won the game.
    ///
    /// TODO: Should this function be removed?
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
    /// * `cell` - The `BlockCell` type to use for revealed bombs:
    ///     - `BlockCell::BombDown` for a loss
    ///     - `BlockCell::BombUp` for a win
    ///
    /// TODO: Should the caller just pass in whether the game was won or lost instead of a `BlockCell`?
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn show_bombs(&mut self, hwnd: &HWND, cell: BlockCell) -> AnyResult<()> {
        for y in 1..=self.board_height {
            for x in 1..=self.board_width {
                if !self.block_value(x, y).visited {
                    if self.block_value(x, y).bomb {
                        if self.block_value(x, y).block_type != BlockCell::BombUp {
                            self.set_block_value(x, y, BlockInfo::from(cell));
                        }
                    } else if self.block_value(x, y).block_type == BlockCell::BombUp {
                        // TODO: Could `set_block_value` take a `BlockCell` directly?
                        self.set_block_value(x, y, BlockInfo::from(BlockCell::Wrong));
                    }
                }
            }
        }
        self.grafix.draw_grid(
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
                if self.block_value(x, y).block_type == BlockCell::BombUp {
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
        self.grafix.display_button(hwnd, state)?;
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
    /// * `block` - The new `BlockInfo` value to set.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn change_blk(&mut self, hwnd: &HWND, x: i32, y: i32, block: BlockInfo) -> AnyResult<()> {
        self.set_block_value(x, y, block);
        self.grafix
            .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
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
        let blk = self.board_cells[x as usize][y as usize];
        if blk.visited {
            return Ok(());
        }

        if blk.block_type == BlockCell::Border || blk.block_type == BlockCell::BombUp {
            return Ok(());
        }

        self.boxes_visited += 1;
        let mut bombs = 0;
        for y_n in (y - 1)..=(y + 1) {
            for x_n in (x - 1)..=(x + 1) {
                if self.board_cells[x_n as usize][y_n as usize].bomb {
                    bombs += 1;
                }
            }
        }
        self.board_cells[x as usize][y as usize] = BlockInfo {
            bomb: false,
            visited: true,
            block_type: BlockCell::from(bombs),
        };
        self.grafix
            .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;

        if bombs == 0 && *tail < I_STEP_MAX {
            queue[*tail] = (x, y);
            *tail += 1;
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
            Sound::WinGame.play(&hwnd.hinstance());
        } else {
            Sound::LoseGame.play(&hwnd.hinstance());
        }
        self.game_status = StatusFlag::GameOver;

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
        if self.block_value(x, y).bomb {
            let visits = self.boxes_visited;
            if visits == 0 {
                for y_t in 1..self.board_height {
                    for x_t in 1..self.board_width {
                        if !self.block_value(x_t, y_t).bomb {
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
                    BlockInfo {
                        bomb: true,
                        visited: true,
                        block_type: BlockCell::Explode,
                    },
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
        if !self.block_value(x_center, y_center).visited
            || self.block_value(x_center, y_center).block_type as u8
                != self.count_marks(x_center, y_center)
        {
            self.track_mouse(hwnd, -2, -2)?;
            return Ok(());
        }

        let mut lose = false;
        for y in (y_center - 1)..=(y_center + 1) {
            for x in (x_center - 1)..=(x_center + 1) {
                if self.block_value(x, y).block_type == BlockCell::BombUp {
                    continue;
                }

                if self.block_value(x, y).bomb {
                    lose = true;
                    self.change_blk(
                        hwnd,
                        x,
                        y,
                        BlockInfo {
                            bomb: true,
                            visited: true,
                            block_type: BlockCell::Explode,
                        },
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
        if !self.in_range(x, y) || self.block_value(x, y).visited {
            return Ok(());
        }

        let allow_marks = { preferences_mutex().mark_enabled };

        // If currently flagged
        let block = if self.block_value(x, y).block_type == BlockCell::BombUp {
            // Increment the bomb count
            self.bombs_left += 1;
            self.grafix
                .draw_bomb_count(&hwnd.GetDC()?, self.bombs_left)?;

            // If marks are allowed, change to question mark; otherwise, change to blank
            if allow_marks {
                BlockCell::GuessUp
            } else {
                BlockCell::BlankUp
            }
        } else if self.block_value(x, y).block_type == BlockCell::GuessUp {
            // If currently marked with a question mark, change to blank
            // No need to update the bomb count since the guess mark doesn't affect it
            BlockCell::BlankUp
        } else {
            // Currently blank; change to flagged and decrement bomb count
            self.bombs_left -= 1;
            self.grafix
                .draw_bomb_count(&hwnd.GetDC()?, self.bombs_left)?;
            BlockCell::BombUp
        };

        // Update the block visually
        self.change_blk(hwnd, x, y, BlockInfo::from(block))?;

        // If the user has flagged the last bomb, they have won
        if self.block_value(x, y).block_type == BlockCell::BombUp && self.check_win() {
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
        let mut blk = self.block_value(x, y).block_type;
        blk = match blk {
            BlockCell::GuessUp => BlockCell::GuessDown,
            BlockCell::BlankUp => BlockCell::Blank,
            _ => blk,
        };
        self.set_block_value(x, y, BlockInfo::from(blk));
    }

    /// Restore a depressed box visually.
    ///
    /// Boxes are restored to their raised state when the left mouse button is released or the cursor is no longer over them.
    /// # Arguments
    /// * `x` - The X coordinate of the box.
    /// * `y` - The Y coordinate of the box.
    fn pop_box_up(&mut self, x: i32, y: i32) {
        let mut blk = self.block_value(x, y).block_type;
        blk = match blk {
            BlockCell::GuessDown => BlockCell::GuessUp,
            BlockCell::Blank => BlockCell::BlankUp,
            _ => blk,
        };
        self.set_block_value(x, y, BlockInfo::from(blk));
    }

    /// Check if a given coordinate is within range, not visited, and not guessed as a bomb.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the coordinate is valid for flood-fill, `false` otherwise.
    fn in_range_step(&mut self, x: i32, y: i32) -> bool {
        self.in_range(x, y)
            && !self.block_value(x, y).visited
            && self.block_value(x, y).block_type != BlockCell::BombUp
    }

    /// Reset the game field to its initial blank state and rebuild the border.
    pub fn clear_field(&mut self) {
        self.board_cells
            .iter_mut()
            .flatten()
            .for_each(|b| *b = BlockInfo::from(BlockCell::BlankUp));

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
            self.grafix.draw_timer(&hwnd.GetDC()?, self.secs_elapsed)?;
            Sound::Tick.play(&hwnd.hinstance());
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
        let x_prev = self.state.read().board_width;
        let y_prev = self.state.read().board_height;

        let (pref_width, pref_height, total_bombs) = {
            let prefs = preferences_mutex();
            (prefs.width, prefs.height, prefs.mines)
        };

        let f_adjust = if pref_width != x_prev || pref_height != y_prev {
            AdjustFlag::ResizeAndRedraw
        } else {
            AdjustFlag::Redraw
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
                if !self.state.read().block_value(x as i32, y as i32).bomb {
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
        self.state.write().game_status = StatusFlag::Play;

        self.state
            .write()
            .grafix
            .draw_bomb_count(&self.wnd.hwnd().GetDC()?, total_bombs)?;

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
        if self.chord_active {
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
                        if !self.block_value(x, y).visited {
                            // Restore the box to its raised state
                            self.pop_box_up(x, y);
                            self.grafix
                                .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
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
                        if !self.block_value(x, y).visited {
                            // Depress the box visually
                            self.push_box_down(x, y);
                            self.grafix
                                .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;
                        }
                    }
                }
            }
        } else {
            // Otherwise, handle single-box push/pop
            // Check if the old cursor position is in range and not yet visited
            if self.in_range(self.cursor_pos.x, self.cursor_pos.y)
                && !self
                    .block_value(self.cursor_pos.x, self.cursor_pos.y)
                    .visited
            {
                // Restore the old box to its raised state
                self.pop_box_up(self.cursor_pos.x, self.cursor_pos.y);
                self.grafix.draw_block(
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
                self.grafix
                    .draw_block(&hwnd.GetDC()?, pt_new.x, pt_new.y, &self.board_cells)?;
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
                Sound::Tick.play(&hwnd.hinstance());
                self.secs_elapsed = 1;
                self.grafix.draw_timer(&hwnd.GetDC()?, self.secs_elapsed)?;
                self.timer_running = true;
                if let Some(hwnd) = hwnd.as_opt() {
                    hwnd.SetTimer(ID_TIMER, 1000, None)?;
                }
            }

            // If the game is not in play mode, reset the cursor position to a location off the board
            if !self.game_status.contains(StatusFlag::Play) {
                self.cursor_pos = POINT { x: -2, y: -2 };
            }

            // Determine whether to chord (select adjacent squares) or step (reveal a single square)
            if self.chord_active {
                self.step_block(hwnd, x_pos, y_pos)?;
            } else if self.in_range_step(x_pos, y_pos) {
                self.step_square(hwnd, x_pos, y_pos)?;
            }
        }

        self.grafix.display_button(hwnd, self.btn_face_state)?;

        Ok(())
    }

    /// Pause the game by silencing audio, storing the timer state, and setting the pause flag.
    pub fn pause_game(&mut self) {
        Sound::stop_all();

        if !self.game_status.contains(StatusFlag::Pause) {
            F_OLD_TIMER_STATUS.store(self.timer_running, Ordering::Relaxed);
        }
        if self.game_status.contains(StatusFlag::Play) {
            self.timer_running = false;
        }

        self.game_status.insert(StatusFlag::Pause);
    }

    /// Resume the game by restoring the timer state and clearing the pause flag.
    pub fn resume_game(&mut self) {
        if self.game_status.contains(StatusFlag::Play) {
            self.timer_running = F_OLD_TIMER_STATUS.load(Ordering::Relaxed);
        }
        self.game_status.remove(StatusFlag::Pause);
    }
}
