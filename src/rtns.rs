//! Handlers for the core game logic and state management.
//! This includes board representation, game status tracking, and related utilities.

use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, AtomicI16, AtomicI32, AtomicU8, AtomicU16, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use winsafe::co::WM;
use winsafe::msg::WndMsg;
use winsafe::{HINSTANCE, HWND, prelude::*};

use crate::globals::{BLK_BTN_INPUT, ERR_TIMER, GAME_STATUS, StatusFlag};
use crate::grafix::{
    ButtonSprite, display_block, display_bomb_count, display_button, display_grid, display_time,
};
use crate::pref::{CCH_NAME_MAX, GameType, MenuMode, Pref, SoundState};
use crate::sound::{PlayTune, Tune, stop_all_sounds};
use crate::util::{ReportErr, Rnd};
use crate::winmine::{NEW_RECORD_DLG, WinMineMainWindow};

/// Encoded board values used to track each tile state.
///
/// These values are used to get the visual representation of each cell, in reverse order.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum BlockCell {
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
pub fn preferences_mutex() -> &'static Mutex<Pref> {
    PREFERENCES.get_or_init(|| {
        Mutex::new(Pref {
            wGameType: GameType::Begin,
            Mines: 0,
            Height: 0,
            Width: 0,
            xWindow: 0,
            yWindow: 0,
            fSound: SoundState::Off,
            fMark: false,
            fTick: false,
            fMenu: MenuMode::AlwaysOn,
            fColor: false,
            rgTime: [0; 3],
            szBegin: String::with_capacity(CCH_NAME_MAX),
            szInter: String::with_capacity(CCH_NAME_MAX),
            szExpert: String::with_capacity(CCH_NAME_MAX),
        })
    })
}

// TODO: Migrate these globals into a GameState struct

/// Current board width in cells (excluding border)
pub static BOARD_WIDTH: AtomicI32 = AtomicI32::new(0);

/// Current board height in cells (excluding border)
pub static BOARD_HEIGHT: AtomicI32 = AtomicI32::new(0);

/// Current button face sprite
pub static BTN_FACE_STATE: AtomicU8 = AtomicU8::new(ButtonSprite::Happy as u8);

/// Current number of bombs left to mark
///
/// Note: The bomb count can go negative if the user marks more squares than there are bombs.
pub static BOMBS_LEFT: AtomicI16 = AtomicI16::new(0);

/// Current elapsed time in seconds.
///
/// The timer should never exceed 999 seconds, so u16 is sufficient.
pub static SECS_ELAPSED: AtomicU16 = AtomicU16::new(0);

/// Number of visited boxes (revealed non-bomb cells).
///
/// Note: Maximum value is 2<sup>16</sup>, or a 256 x 256 board with no bombs.
pub static C_BOX_VISIT: AtomicU16 = AtomicU16::new(0);

/// Current cursor X position in board coordinates
pub static CURSOR_X_POS: AtomicI32 = AtomicI32::new(-1);

/// Current cursor Y position in board coordinates
pub static CURSOR_Y_POS: AtomicI32 = AtomicI32::new(-1);

/// Packed board cell values stored row-major including border
///
/// TODO: Replace with a better data structure, perhaps using a struct?
/// TODO: Use an enum for the cell values instead of i8
static RG_BLK: OnceLock<Mutex<[i8; C_BLK_MAX]>> = OnceLock::new();

/// Accessor for the packed board cell array.
/// # Returns
/// A mutex guard for the packed board cell array.
///
/// TODO: Use an enum for the cell values instead of i8
pub fn board_mutex() -> MutexGuard<'static, [i8; C_BLK_MAX]> {
    match RG_BLK
        .get_or_init(|| Mutex::new([BlockCell::BlankUp as i8; C_BLK_MAX]))
        .lock()
    {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Initial number of bombs at the start of the game
///
/// TODO: Migrate these globals into a GameState struct
static CBOMB_START: AtomicI16 = AtomicI16::new(0);

/// Total number of visited boxes needed to win
static CBOX_VISIT_MAC: AtomicU16 = AtomicU16::new(0);

/// Indicates whether the game timer is running
static F_TIMER: AtomicBool = AtomicBool::new(false);

/// Previous timer running state used to detect changes
static F_OLD_TIMER_STATUS: AtomicBool = AtomicBool::new(false);

/// Calculate the board index for the given coordinates.
///
/// TODO: Return Result instead of Option
/// TODO: X and Y should be usize or u32, negative indices make no sense
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

/// Retrieve the value of a block at the specified coordinates.
///
/// TODO: Return an enum instead of an u8
/// TODO: Does this function need to exist?
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// The value of the block, or 0 if out of range.
fn block_value(x: i32, y: i32) -> u8 {
    let guard = board_mutex();
    board_index(x, y)
        .and_then(|idx| guard.get(idx).copied())
        .unwrap_or(0) as u8
}

/// Set the value of a block at the specified coordinates.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// * `value` - The value to set.
fn set_block_value(x: i32, y: i32, value: u8) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = board_mutex();
        let prev = guard[idx] as u8;

        // Preserve existing flag bits, but allow callers to explicitly set the Visit bit.
        // (Bomb placement is handled separately via set_bomb/clear_bomb.)
        let flags = (prev & BlockMask::Flags as u8) | (value & BlockMask::Visit as u8);
        let data = value & BlockMask::Data as u8;
        guard[idx] = (flags | data) as i8;
    }
}

/// Set a block as a border at the specified coordinates.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
fn set_border(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = board_mutex();
        guard[idx] = BlockCell::Border as i8;
    }
}

/// Set a block as containing a bomb at the specified coordinates.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
fn set_bomb(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = board_mutex();
        let prev = guard[idx];
        guard[idx] = prev | BlockMask::Bomb as i8;
    }
}

/// Clear the bomb flag from a block at the specified coordinates.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
fn clear_bomb(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = board_mutex();
        let prev = guard[idx] as u8;
        guard[idx] = (prev & BlockMask::NotBomb as u8) as i8;
    }
}

/// Check if a block at the specified coordinates contains a bomb.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the block contains a bomb, `false` otherwise.
fn is_bomb(x: i32, y: i32) -> bool {
    (block_value(x, y) & BlockMask::Bomb as u8) != 0
}

/// Check if a block at the specified coordinates has been visited.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the block has been visited, `false` otherwise.
fn is_visit(x: i32, y: i32) -> bool {
    (block_value(x, y) & BlockMask::Visit as u8) != 0
}

/// Check if a block at the specified coordinates is guessed to contain a bomb.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the block is guessed to contain a bomb, `false` otherwise.
fn guessed_bomb(x: i32, y: i32) -> bool {
    block_value(x, y) & BlockMask::Data as u8 == BlockCell::BombUp as u8
}

/// Check if a block at the specified coordinates is guessed to be marked.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the block is guessed to be marked, `false` otherwise.
fn guessed_mark(x: i32, y: i32) -> bool {
    block_value(x, y) & BlockMask::Data as u8 == BlockCell::GuessUp as u8
}

/// Check if the given coordinates are within the valid range of the board.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the coordinates are within range, `false` otherwise.
fn f_in_range(x: i32, y: i32) -> bool {
    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

/// Set a raw block value at the specified coordinates, preserving only data and visit bits.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// * `block` - The raw block value to set.
fn set_raw_block(x: i32, y: i32, block: u8) {
    // Keep only the data bits plus the Visit bit (when present).
    let masked = block & (BlockMask::Data as u8 | BlockMask::Visit as u8);
    set_block_value(x, y, masked);
}

/// Get the data bits of a block at the specified coordinates.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// The data bits of the block.
fn block_data(x: i32, y: i32) -> u8 {
    block_value(x, y) & BlockMask::Data as u8
}

/// Check if the player has won the game.
/// # Returns
/// `true` if the player has won, `false` otherwise.
fn check_win() -> bool {
    C_BOX_VISIT.load(Ordering::Relaxed) == CBOX_VISIT_MAC.load(Ordering::Relaxed)
}

/// Play a logical tune if sound effects are enabled in preferences.
/// # Arguments
/// * `tune` - The tune to play.
fn play_tune(hinst: &HINSTANCE, tune: Tune) {
    let sound_on = {
        let prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.fSound == SoundState::On
    };

    if sound_on {
        PlayTune(hinst, tune);
    }
}

/// Reveal all bombs on the board and mark incorrect guesses.
///
/// This is called when the game ends to show the final board state.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `cell` - The block cell type to use for revealed bombs.
fn show_bombs(hwnd: &HWND, cell: BlockCell) {
    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);

    for y in 1..=y_max {
        for x in 1..=x_max {
            if !is_visit(x, y) {
                if is_bomb(x, y) {
                    if !guessed_bomb(x, y) {
                        set_raw_block(x, y, cell as u8);
                    }
                } else if guessed_bomb(x, y) {
                    set_raw_block(x, y, BlockCell::Wrong as u8);
                }
            }
        }
    }
    display_grid(hwnd);
}

/// Count the number of adjacent marked squares around the specified coordinates.
/// # Arguments
/// * `x_center` - The X coordinate of the center square.
/// * `y_center` - The Y coordinate of the center square.
/// # Returns
/// The count of adjacent marked squares (maximum 8).
fn count_marks(x_center: i32, y_center: i32) -> u8 {
    let mut count = 0;
    for y in (y_center - 1)..=(y_center + 1) {
        for x in (x_center - 1)..=(x_center + 1) {
            if guessed_bomb(x, y) {
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
fn update_button_for_result(hwnd: &HWND, win: bool) {
    let state = if win {
        ButtonSprite::Win
    } else {
        ButtonSprite::Lose
    };
    BTN_FACE_STATE.store(state as u8, Ordering::Relaxed);
    display_button(hwnd, state);
}

/// Record a new win time if it is a personal best.
/// # Arguments
/// * `hwnd` - Handle to the main window.
fn record_win_if_needed(hwnd: &HWND) {
    let elapsed = SECS_ELAPSED.load(Ordering::Relaxed);
    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let game = prefs.wGameType;
    if game != GameType::Other {
        let game_idx = game as usize;
        if game_idx < prefs.rgTime.len() && elapsed < prefs.rgTime[game_idx] {
            prefs.rgTime[game_idx] = elapsed;
            drop(prefs);

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
fn change_blk(hwnd: &HWND, x: i32, y: i32, block: u8) {
    set_raw_block(x, y, block);
    display_block(hwnd, x, y);
}

/// Enqueue a square for flood-fill processing if it is empty.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `queue` - The flood-fill work queue.
/// * `tail` - The current tail index of the queue.
/// * `x` - The X coordinate of the square.
/// * `y` - The Y coordinate of the square.
fn step_xy(hwnd: &HWND, queue: &mut [(i32, i32); I_STEP_MAX], tail: &mut usize, x: i32, y: i32) {
    // Visit a square; enqueue it when empty so we flood-fill neighbors later.
    if let Some(idx) = board_index(x, y) {
        let mut board = board_mutex();
        let mut blk = board[idx] as u8;
        if (blk & BlockMask::Visit as u8) != 0 {
            return;
        }

        let data = blk & BlockMask::Data as u8;
        if data == BlockCell::Border as u8 || data == BlockCell::BombUp as u8 {
            return;
        }

        C_BOX_VISIT.fetch_add(1, Ordering::Relaxed);
        let mut bombs = 0;
        for y_n in (y - 1)..=(y + 1) {
            for x_n in (x - 1)..=(x + 1) {
                if let Some(nidx) = board_index(x_n, y_n) {
                    let cell = board[nidx] as u8;
                    if (cell & BlockMask::Bomb as u8) != 0 {
                        bombs += 1;
                    }
                }
            }
        }
        blk = BlockMask::Visit as u8 | ((bombs as u8) & BlockMask::Data as u8);
        board[idx] = blk as i8;
        drop(board);
        display_block(hwnd, x, y);

        if bombs == 0 && *tail < I_STEP_MAX {
            queue[*tail] = (x, y);
            *tail += 1;
        }
    }
}

/// Flood-fill contiguous empty squares starting from (x, y).
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `x` - X coordinate of the starting square
/// * `y` - Y coordinate of the starting square
fn step_box(hwnd: &HWND, x: i32, y: i32) {
    let mut queue = [(0, 0); I_STEP_MAX];
    let mut head = 0usize;
    let mut tail = 0usize;

    step_xy(hwnd, &mut queue, &mut tail, x, y);

    while head < tail {
        let (sx, sy) = queue[head];
        head += 1;

        let mut ty = sy - 1;
        step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty);
        step_xy(hwnd, &mut queue, &mut tail, sx, ty);
        step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty);

        ty += 1;
        step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty);
        step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty);

        ty += 1;
        step_xy(hwnd, &mut queue, &mut tail, sx - 1, ty);
        step_xy(hwnd, &mut queue, &mut tail, sx, ty);
        step_xy(hwnd, &mut queue, &mut tail, sx + 1, ty);
    }
}

/// Handle the end of the game - stopping the timer, revealing bombs, updating the face, and recording wins.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `win` - `true` if the player has won, `false` otherwise
fn game_over(hwnd: &HWND, win: bool) {
    F_TIMER.store(false, Ordering::Relaxed);
    update_button_for_result(hwnd, win);
    show_bombs(
        hwnd,
        if win {
            BlockCell::BombUp
        } else {
            BlockCell::BombDown
        },
    );
    if win {
        BOMBS_LEFT.store(0, Ordering::Relaxed);
    }
    play_tune(
        &hwnd.hinstance(),
        if win { Tune::WinGame } else { Tune::LoseGame },
    );
    GAME_STATUS.store(StatusFlag::Demo as i32, Ordering::Relaxed);

    if win {
        record_win_if_needed(hwnd);
    }
}

/// Handle a user click on a single square (first-click safety included).
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `x` - The X coordinate of the clicked square.
/// * `y` - The Y coordinate of the clicked square.
fn step_square(hwnd: &HWND, x: i32, y: i32) {
    if is_bomb(x, y) {
        let visits = C_BOX_VISIT.load(Ordering::Relaxed);
        if visits == 0 {
            let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
            let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);
            for y_t in 1..y_max {
                for x_t in 1..x_max {
                    if !is_bomb(x_t, y_t) {
                        clear_bomb(x, y);
                        set_bomb(x_t, y_t);
                        step_box(hwnd, x, y);
                        return;
                    }
                }
            }
        } else {
            change_blk(
                hwnd,
                x,
                y,
                BlockMask::Visit as u8 | BlockCell::Explode as u8,
            );
            game_over(hwnd, false);
        }
    } else {
        step_box(hwnd, x, y);
        if check_win() {
            game_over(hwnd, true);
        }
    }
}

/// Handle a chord action on a revealed number square.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `x_center` - The X coordinate of the center square.
/// * `y_center` - The Y coordinate of the center square.
fn step_block(hwnd: &HWND, x_center: i32, y_center: i32) {
    if !is_visit(x_center, y_center)
        || block_data(x_center, y_center) != count_marks(x_center, y_center)
    {
        TrackMouse(hwnd, -2, -2);
        return;
    }

    let mut lose = false;
    for y in (y_center - 1)..=(y_center + 1) {
        for x in (x_center - 1)..=(x_center + 1) {
            if guessed_bomb(x, y) {
                continue;
            }

            if is_bomb(x, y) {
                lose = true;
                change_blk(
                    hwnd,
                    x,
                    y,
                    BlockMask::Visit as u8 | BlockCell::Explode as u8,
                );
            } else {
                step_box(hwnd, x, y);
            }
        }
    }

    if lose {
        game_over(hwnd, false);
    } else if check_win() {
        game_over(hwnd, true);
    }
}

/// Handle a user guess (flag or question mark) on a square.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `x` - The X coordinate of the square.
/// * `y` - The Y coordinate of the square.
pub fn make_guess(hwnd: &HWND, x: i32, y: i32) {
    // Cycle through blank -> flag -> question mark states depending on preferences.

    // Return if the square is out of range or already visited.
    if !f_in_range(x, y) || is_visit(x, y) {
        return;
    }

    let allow_marks = {
        let prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.fMark
    };

    let block = if guessed_bomb(x, y) {
        update_bomb_count_internal(hwnd, 1);
        if allow_marks {
            BlockCell::GuessUp as u8
        } else {
            BlockCell::BlankUp as u8
        }
    } else if guessed_mark(x, y) {
        BlockCell::BlankUp as u8
    } else {
        update_bomb_count_internal(hwnd, -1);
        BlockCell::BombUp as u8
    };

    change_blk(hwnd, x, y, block);

    if guessed_bomb(x, y) && check_win() {
        game_over(hwnd, true);
    }
}

/// Depress a box visually.
///
/// Boxes are pushed down while the left mouse button is pressed over them.
///
/// TODO: Can `push_box_down` and `pop_box_up` be merged?
/// # Arguments
/// * `x` - The X coordinate of the box.
/// * `y` - The Y coordinate of the box.
fn push_box_down(x: i32, y: i32) {
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == BlockCell::GuessUp as u8 => BlockCell::GuessDown as u8,
        b if b == BlockCell::BlankUp as u8 => BlockCell::Blank as u8,
        _ => blk,
    };
    set_raw_block(x, y, blk);
}

/// Restore a depressed box visually.
///
/// Boxes are restored to their raised state when the left mouse button is released or the cursor is no longer over them.
/// # Arguments
/// * `x` - The X coordinate of the box.
/// * `y` - The Y coordinate of the box.
fn pop_box_up(x: i32, y: i32) {
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == BlockCell::GuessDown as u8 => BlockCell::GuessUp as u8,
        b if b == BlockCell::Blank as u8 => BlockCell::BlankUp as u8,
        _ => blk,
    };
    set_raw_block(x, y, blk);
}

/// Change the bomb count by the specified delta and update the display.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `delta` - The change in bomb count (positive or negative).
fn update_bomb_count_internal(hwnd: &HWND, delta: i16) {
    BOMBS_LEFT.fetch_add(delta, Ordering::Relaxed);
    display_bomb_count(hwnd);
}

/// Check if a given coordinate is within range, not visited, and not guessed as a bomb.
/// # Arguments
/// * `x` - The X coordinate.
/// * `y` - The Y coordinate.
/// # Returns
/// `true` if the coordinate is valid for flood-fill, `false` otherwise.
fn in_range_step(x: i32, y: i32) -> bool {
    f_in_range(x, y) && !is_visit(x, y) && !guessed_bomb(x, y)
}

/// Reset the game field to its initial blank state and rebuild the border.
pub fn ClearField() {
    {
        let mut board = board_mutex();
        board.iter_mut().for_each(|b| *b = BlockCell::BlankUp as i8);
    }

    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);

    for x in 0..=(x_max + 1) {
        set_border(x, 0);
        set_border(x, y_max + 1);
    }
    for y in 0..=(y_max + 1) {
        set_border(0, y);
        set_border(x_max + 1, y);
    }
}

/// Handle the per-second game timer tick.
/// # Arguments
/// * `hwnd` - Handle to the main window.
pub fn DoTimer(hwnd: &HWND) {
    let secs = SECS_ELAPSED.load(Ordering::Relaxed);
    if F_TIMER.load(Ordering::Relaxed) && secs < 999 {
        SECS_ELAPSED.store(secs + 1, Ordering::Relaxed);
        display_time(hwnd);
        play_tune(&hwnd.hinstance(), Tune::Tick);
    }
}

impl WinMineMainWindow {
    /// Start a new game by resetting globals, randomizing bombs, and resizing the window if the board changed.
    /// # Arguments
    /// * `hwnd` - Handle to the main window.
    pub fn StartGame(&self) {
        F_TIMER.store(false, Ordering::Relaxed);

        let x_prev = BOARD_WIDTH.load(Ordering::Relaxed);
        let y_prev = BOARD_HEIGHT.load(Ordering::Relaxed);

        let (pref_width, pref_height, pref_mines) = {
            let prefs = match preferences_mutex().lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            (prefs.Width, prefs.Height, prefs.Mines)
        };

        let f_adjust = if pref_width != x_prev || pref_height != y_prev {
            AdjustFlag::Resize as i32 | AdjustFlag::Display as i32
        } else {
            AdjustFlag::Display as i32
        };

        BOARD_WIDTH.store(pref_width, Ordering::Relaxed);
        BOARD_HEIGHT.store(pref_height, Ordering::Relaxed);

        ClearField();
        BTN_FACE_STATE.store(ButtonSprite::Happy as u8, Ordering::Relaxed);

        CBOMB_START.store(pref_mines, Ordering::Relaxed);

        let total_bombs = CBOMB_START.load(Ordering::Relaxed);
        let width = BOARD_WIDTH.load(Ordering::Relaxed);
        let height = BOARD_HEIGHT.load(Ordering::Relaxed);

        let mut bombs = total_bombs;
        while bombs > 0 {
            let mut x;
            let mut y;
            loop {
                x = Rnd(width as u32) + 1;
                y = Rnd(height as u32) + 1;
                if !is_bomb(x as i32, y as i32) {
                    break;
                }
            }
            set_bomb(x as i32, y as i32);
            bombs -= 1;
        }

        SECS_ELAPSED.store(0, Ordering::Relaxed);
        BOMBS_LEFT.store(total_bombs, Ordering::Relaxed);
        C_BOX_VISIT.store(0, Ordering::Relaxed);
        CBOX_VISIT_MAC.store(
            (width * height) as u16 - total_bombs as u16,
            Ordering::Relaxed,
        );
        GAME_STATUS.store(StatusFlag::Play as i32, Ordering::Relaxed);

        display_bomb_count(self.wnd.hwnd());

        self.AdjustWindow(f_adjust);
    }
}

/// Track mouse movement over the board and provide visual feedback.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `x_new` - The new X coordinate of the mouse.
/// * `y_new` - The new Y coordinate of the mouse.
pub fn TrackMouse(hwnd: &HWND, x_new: i32, y_new: i32) {
    let x_old = CURSOR_X_POS.load(Ordering::Relaxed);
    let y_old = CURSOR_Y_POS.load(Ordering::Relaxed);

    if x_new == x_old && y_new == y_old {
        return;
    }

    CURSOR_X_POS.store(x_new, Ordering::Relaxed);
    CURSOR_Y_POS.store(y_new, Ordering::Relaxed);

    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);
    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);

    if BLK_BTN_INPUT.load(Ordering::Relaxed) {
        let valid_new = f_in_range(x_new, y_new);
        let valid_old = f_in_range(x_old, y_old);

        let y_old_min = max(y_old - 1, 1);
        let y_old_max = min(y_old + 1, y_max);
        let y_cur_min = max(y_new - 1, 1);
        let y_cur_max = min(y_new + 1, y_max);
        let x_old_min = max(x_old - 1, 1);
        let x_old_max = min(x_old + 1, x_max);
        let x_cur_min = max(x_new - 1, 1);
        let x_cur_max = min(x_new + 1, x_max);

        if valid_old {
            for y in y_old_min..=y_old_max {
                for x in x_old_min..=x_old_max {
                    if !is_visit(x, y) {
                        pop_box_up(x, y);
                    }
                }
            }
        }

        if valid_new {
            for y in y_cur_min..=y_cur_max {
                for x in x_cur_min..=x_cur_max {
                    if !is_visit(x, y) {
                        push_box_down(x, y);
                    }
                }
            }
        }

        if valid_old {
            for y in y_old_min..=y_old_max {
                for x in x_old_min..=x_old_max {
                    display_block(hwnd, x, y);
                }
            }
        }

        if valid_new {
            for y in y_cur_min..=y_cur_max {
                for x in x_cur_min..=x_cur_max {
                    display_block(hwnd, x, y);
                }
            }
        }
    } else {
        if f_in_range(x_old, y_old) && !is_visit(x_old, y_old) {
            pop_box_up(x_old, y_old);
            display_block(hwnd, x_old, y_old);
        }
        if f_in_range(x_new, y_new) && in_range_step(x_new, y_new) {
            push_box_down(x_new, y_new);
            display_block(hwnd, x_new, y_new);
        }
    }
}

/// Handle a left-button release: start the timer, then either chord or step.
/// # Arguments
/// * `hwnd` - Handle to the main window.
pub fn DoButton1Up(hwnd: &HWND) {
    let x_pos = CURSOR_X_POS.load(Ordering::Relaxed);
    let y_pos = CURSOR_Y_POS.load(Ordering::Relaxed);

    if f_in_range(x_pos, y_pos) {
        let visits = C_BOX_VISIT.load(Ordering::Relaxed);
        let secs = SECS_ELAPSED.load(Ordering::Relaxed);
        if visits == 0 && secs == 0 {
            play_tune(&hwnd.hinstance(), Tune::Tick);
            SECS_ELAPSED.store(1, Ordering::Relaxed);
            display_time(hwnd);
            F_TIMER.store(true, Ordering::Relaxed);
            if let Some(hwnd) = hwnd.as_opt()
                && hwnd.SetTimer(ID_TIMER, 1000, None).is_err()
            {
                ReportErr(ERR_TIMER);
            }
        }

        if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) == 0 {
            CURSOR_X_POS.store(-2, Ordering::Relaxed);
            CURSOR_Y_POS.store(-2, Ordering::Relaxed);
        }

        if BLK_BTN_INPUT.load(Ordering::Relaxed) {
            step_block(hwnd, x_pos, y_pos);
        } else if in_range_step(x_pos, y_pos) {
            step_square(hwnd, x_pos, y_pos);
        }
    }

    let button = match BTN_FACE_STATE.load(Ordering::Relaxed) {
        0 => ButtonSprite::Happy,
        1 => ButtonSprite::Caution,
        2 => ButtonSprite::Lose,
        3 => ButtonSprite::Win,
        _ => ButtonSprite::Down,
    };
    display_button(hwnd, button);
}

/// Pause the game by silencing audio, storing the timer state, and setting the pause flag.
pub fn PauseGame() {
    stop_all_sounds();

    if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Pause as i32)) == 0 {
        F_OLD_TIMER_STATUS.store(F_TIMER.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) != 0 {
        F_TIMER.store(false, Ordering::Relaxed);
    }

    GAME_STATUS.fetch_or(StatusFlag::Pause as i32, Ordering::Relaxed);
}

/// Resume the game by restoring the timer state and clearing the pause flag.
pub fn ResumeGame() {
    if (GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) != 0 {
        F_TIMER.store(
            F_OLD_TIMER_STATUS.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }
    GAME_STATUS.fetch_and(!(StatusFlag::Pause as i32), Ordering::Relaxed);
}
