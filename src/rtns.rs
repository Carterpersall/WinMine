//! Handlers for the core game logic and state management.
//! This includes board representation, game status tracking, and related utilities.

use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, Ordering};

use bitflags::bitflags;
use winsafe::co::WM;
use winsafe::msg::WndMsg;
use winsafe::{AnyResult, HWND, prelude::*};

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

/// Represents the current state of the game.
pub struct GameState {
    /// Graphics state containing bitmaps and rendering logic.
    pub grafix: GrafixState,
    /// Current user preferences.
    pub prefs: Pref,
    /// Aggregated status flags defining the current game state.
    pub game_status: StatusFlag,
    /// Current board width in cells (excluding border)
    pub board_width: usize,
    /// Current board height in cells (excluding border)
    pub board_height: usize,
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
    /// Current cursor x position in board coordinates
    pub cursor_x: usize,
    /// Current cursor y position in board coordinates
    pub cursor_y: usize,
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
            bombs_left: 0,
            secs_elapsed: 0,
            boxes_visited: 0,
            cursor_x: 0,
            cursor_y: 0,
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
        self.grafix.load_bitmaps(hwnd, self.prefs.color)?;
        self.clear_field();
        Ok(())
    }

    /// Check if the given coordinates are within the valid range of the board.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the coordinates are within range, `false` otherwise.
    const fn in_range(&self, x: usize, y: usize) -> bool {
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
                // If the cell is not visited is not the exploded bomb cell
                if !self.board_cells[x][y].visited
                    && self.board_cells[x][y].block_type != BlockCell::Explode
                {
                    if self.board_cells[x][y].bomb {
                        if self.board_cells[x][y].block_type != BlockCell::BombUp {
                            // If a bomb cell was not marked, reveal it
                            self.board_cells[x][y].block_type = cell;
                        }
                    } else if self.board_cells[x][y].block_type == BlockCell::BombUp {
                        // If a non-bomb cell was marked as a bomb, show it as incorrect
                        self.board_cells[x][y].block_type = BlockCell::Wrong;
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
    fn count_marks(&self, x_center: usize, y_center: usize) -> u8 {
        let mut count = 0;
        for y in (y_center - 1)..=(y_center + 1) {
            for x in (x_center - 1)..=(x_center + 1) {
                if self.board_cells[x][y].block_type == BlockCell::BombUp {
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
        let game = self.prefs.game_type;
        if game != GameType::Other {
            let game_idx = game as usize;
            if game_idx < self.prefs.best_times.len()
                && self.secs_elapsed < self.prefs.best_times[game_idx]
            {
                {
                    self.prefs.best_times[game_idx] = self.secs_elapsed;
                }

                if hwnd.as_opt().is_some() {
                    unsafe {
                        let _ = hwnd.PostMessage(WndMsg::new(WM::APP, NEW_RECORD_DLG, 0));
                    }
                }
            }
        }
    }

    /// Enqueue a square for flood-fill processing if it is empty.
    ///
    /// TODO: Don't take a mutable reference as an argument
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
        queue: &mut [(usize, usize); I_STEP_MAX],
        tail: &mut usize,
        x: usize,
        y: usize,
    ) -> AnyResult<()> {
        let blk = self.board_cells[x][y];
        if blk.visited || blk.block_type == BlockCell::Border || blk.block_type == BlockCell::BombUp
        {
            // Already visited, out of range, or marked as a bomb; do nothing
            return Ok(());
        }

        self.boxes_visited += 1;

        // Count the number of adjacent bombs
        let mut bombs = 0;
        for y_n in (y - 1)..=(y + 1) {
            for x_n in (x - 1)..=(x + 1) {
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
        self.grafix
            .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;

        // If no adjacent bombs, enqueue for further flood-fill processing
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
    fn step_box(&mut self, hwnd: &HWND, x: usize, y: usize) -> AnyResult<()> {
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
            if self.prefs.sound_enabled {
                Sound::WinGame.play(&hwnd.hinstance());
            }
        } else if self.prefs.sound_enabled {
            Sound::LoseGame.play(&hwnd.hinstance());
        }
        self.game_status = StatusFlag::GameOver;

        if win {
            self.record_win_if_needed(hwnd);
        }

        Ok(())
    }

    /// Handle a user click on a single square.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// * `x` - The X coordinate of the clicked square.
    /// * `y` - The Y coordinate of the clicked square.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn step_square(&mut self, hwnd: &HWND, x: usize, y: usize) -> AnyResult<()> {
        if self.board_cells[x][y].bomb {
            let visits = self.boxes_visited;
            if visits == 0 {
                // Ensure that the first clicked square is never a bomb
                for y_t in 1..self.board_height {
                    for x_t in 1..self.board_width {
                        if !self.board_cells[x_t][y_t].bomb {
                            self.board_cells[x][y].bomb = false;
                            self.board_cells[x_t][y_t].bomb = true;
                            self.step_box(hwnd, x, y)?;
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
    fn step_block(&mut self, hwnd: &HWND, x_center: usize, y_center: usize) -> AnyResult<()> {
        if !self.board_cells[x_center][y_center].visited
            || self.board_cells[x_center][y_center].block_type as u8
                != self.count_marks(x_center, y_center)
        {
            self.track_mouse(hwnd, usize::MAX - 2, usize::MAX - 2)?;
            return Ok(());
        }

        // If the conditions of a chord operation are met, reveal adjacent squares
        let mut lose = false;
        for y in (y_center - 1)..=(y_center + 1) {
            for x in (x_center - 1)..=(x_center + 1) {
                // Skip flagged squares
                if self.board_cells[x][y].block_type == BlockCell::BombUp {
                    continue;
                }

                if self.board_cells[x][y].bomb {
                    // If a flag was incorrectly placed, and a bomb is revealed, the player loses
                    lose = true;
                    self.board_cells[x][y].block_type = BlockCell::Explode;
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
    pub fn make_guess(&mut self, hwnd: &HWND, x: usize, y: usize) -> AnyResult<()> {
        // Cycle through blank -> flag -> question mark states depending on preferences.

        // Return if the square is out of range or already visited.
        if !self.in_range(x, y) || self.board_cells[x][y].visited {
            return Ok(());
        }

        let allow_marks = self.prefs.mark_enabled;

        // If currently flagged
        let block = if self.board_cells[x][y].block_type == BlockCell::BombUp {
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
        } else if self.board_cells[x][y].block_type == BlockCell::GuessUp {
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

        // Update the block type and redraw the square
        self.board_cells[x][y].block_type = block;
        self.grafix
            .draw_block(&hwnd.GetDC()?, x, y, &self.board_cells)?;

        // If the user has flagged the last bomb, they have won
        if self.board_cells[x][y].block_type == BlockCell::BombUp && self.check_win() {
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
    const fn push_box_down(&mut self, x: usize, y: usize) {
        let mut blk = self.board_cells[x][y].block_type;
        blk = match blk {
            BlockCell::GuessUp => BlockCell::GuessDown,
            BlockCell::BlankUp => BlockCell::Blank,
            _ => blk,
        };
        self.board_cells[x][y].block_type = blk;
    }

    /// Restore a depressed box visually.
    ///
    /// Boxes are restored to their raised state when the left mouse button is released or the cursor is no longer over them.
    /// # Arguments
    /// * `x` - The X coordinate of the box.
    /// * `y` - The Y coordinate of the box.
    const fn pop_box_up(&mut self, x: usize, y: usize) {
        let mut blk = self.board_cells[x][y].block_type;
        blk = match blk {
            BlockCell::GuessDown => BlockCell::GuessUp,
            BlockCell::Blank => BlockCell::BlankUp,
            _ => blk,
        };
        self.board_cells[x][y].block_type = blk;
    }

    /// Check if a given coordinate is within range, not visited, and not guessed as a bomb.
    /// # Arguments
    /// * `x` - The X coordinate.
    /// * `y` - The Y coordinate.
    /// # Returns
    /// `true` if the coordinate is valid for flood-fill, `false` otherwise.
    fn in_range_step(&mut self, x: usize, y: usize) -> bool {
        self.in_range(x, y)
            && !self.board_cells[x][y].visited
            && self.board_cells[x][y].block_type != BlockCell::BombUp
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
            self.board_cells[x][0].block_type = BlockCell::Border;
            self.board_cells[x][y_max + 1].block_type = BlockCell::Border;
        }
        for y in 0..=(y_max + 1) {
            self.board_cells[0][y].block_type = BlockCell::Border;
            self.board_cells[x_max + 1][y].block_type = BlockCell::Border;
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
            if self.prefs.sound_enabled {
                Sound::Tick.play(&hwnd.hinstance());
            }
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
            let state = self.state.read();
            (state.prefs.width, state.prefs.height, state.prefs.mines)
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
                x = rnd(width as u32) as usize + 1;
                y = rnd(height as u32) as usize + 1;
                if !self.state.read().board_cells[x][y].bomb {
                    break;
                }
            }
            self.state.write().board_cells[x][y].bomb = true;
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
    pub fn track_mouse(&mut self, hwnd: &HWND, x_new: usize, y_new: usize) -> AnyResult<()> {
        // No change in position; nothing to do
        if x_new == self.cursor_x && y_new == self.cursor_y {
            return Ok(());
        }

        let y_max = self.board_height;
        let x_max = self.board_width;

        // Check if a chord operation is active
        if self.chord_active {
            // Determine if the old and new positions are within range
            let valid_new = self.in_range(x_new, y_new);
            let valid_old = self.in_range(self.cursor_x, self.cursor_y);

            // If the old position is valid, pop up boxes in the previous area
            if valid_old {
                // Determine the 3x3 area around the old cursor position
                let y_old_min = max(self.cursor_y - 1, 1);
                let y_old_max = min(self.cursor_y + 1, y_max);
                let x_old_min = max(self.cursor_x - 1, 1);
                let x_old_max = min(self.cursor_x + 1, x_max);
                // Iterate over the old 3x3 area from left to right, top to bottom
                for y in y_old_min..=y_old_max {
                    for x in x_old_min..=x_old_max {
                        // Only pop up boxes that are not visited
                        if !self.board_cells[x][y].visited {
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
                // Determine the 3x3 area around the new cursor position
                let y_cur_min = max(y_new - 1, 1);
                let y_cur_max = min(y_new + 1, y_max);
                let x_cur_min = max(x_new - 1, 1);
                let x_cur_max = min(x_new + 1, x_max);
                // Iterate over the new 3x3 area from left to right, top to bottom
                for y in y_cur_min..=y_cur_max {
                    for x in x_cur_min..=x_cur_max {
                        // Only push down boxes that are not visited
                        if !self.board_cells[x][y].visited {
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
            if self.in_range(self.cursor_x, self.cursor_y)
                && !self.board_cells[self.cursor_x][self.cursor_y].visited
            {
                // Restore the old box to its raised state
                self.pop_box_up(self.cursor_x, self.cursor_y);
                self.grafix.draw_block(
                    &hwnd.GetDC()?,
                    self.cursor_x,
                    self.cursor_y,
                    &self.board_cells,
                )?;
            }
            // Check if the new cursor position is in range and not yet visited
            if self.in_range(x_new, y_new) && self.in_range_step(x_new, y_new) {
                // Depress the new box visually
                self.push_box_down(x_new, y_new);
                self.grafix
                    .draw_block(&hwnd.GetDC()?, x_new, y_new, &self.board_cells)?;
            }
        }
        // Store the new cursor position
        self.cursor_x = x_new;
        self.cursor_y = y_new;
        Ok(())
    }

    /// Handle a left-button release: start the timer, then either chord or step.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    pub fn do_button_1_up(&mut self, hwnd: &HWND) -> AnyResult<()> {
        // Check if the cursor is within the valid range of the board
        if self.in_range(self.cursor_x, self.cursor_y) {
            // If the number of visits and elapsed seconds are both zero, the game has not started yet
            if self.boxes_visited == 0 && self.secs_elapsed == 0 {
                // Play the tick sound, display the initial time, and start the timer
                // TODO: Could we just call `do_timer` here instead?
                if self.prefs.sound_enabled {
                    Sound::Tick.play(&hwnd.hinstance());
                }
                self.secs_elapsed = 1;
                self.grafix.draw_timer(&hwnd.GetDC()?, self.secs_elapsed)?;
                self.timer_running = true;
                if let Some(hwnd) = hwnd.as_opt() {
                    hwnd.SetTimer(ID_TIMER, 1000, None)?;
                }
            }

            // If the game is not in play mode, reset the cursor position to a location off the board
            if !self.game_status.contains(StatusFlag::Play) {
                self.cursor_x = usize::MAX - 2;
                self.cursor_y = usize::MAX - 2;
            }

            // Determine whether to chord (select adjacent squares) or step (reveal a single square)
            if self.chord_active {
                self.step_block(hwnd, self.cursor_x, self.cursor_y)?;
            } else if self.in_range_step(self.cursor_x, self.cursor_y) {
                self.step_square(hwnd, self.cursor_x, self.cursor_y)?;
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
