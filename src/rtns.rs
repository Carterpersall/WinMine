use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::atomic::AtomicU8;
use std::sync::{Mutex, OnceLock};

use winsafe::prelude::*;

use crate::globals::{StatusFlag, fBlock, fStatus, global_state};
use crate::grafix::{
    ButtonSprite, DisplayBlk, DisplayBombCount, DisplayButton, DisplayGrid, DisplayTime,
};
use crate::pref::{CCH_NAME_MAX, GameType, MenuMode, Pref, SoundState};
use crate::sound::{EndTunes, PlayTune, Tune};
use crate::util::{ReportErr, Rnd};
use crate::winmine::{AdjustWindow, DoDisplayBest, DoEnterName};

/// Encoded board values used to track each tile state.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum BlockCell {
    Blank = 0,
    GuessDown = 9,
    BombDown = 10,
    Wrong = 11,
    Explode = 12,
    GuessUp = 13,
    BombUp = 14,
    BlankUp = 15,
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
/// Identifier for reporting timer-related errors through ReportErr.
const ID_ERR_TIMER: u16 = 4;

/// Window-adjustment flags mirrored from the Win16 sources.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum AdjustFlag {
    Resize = 0x02,
    Display = 0x04,
}

/// Shift applied when converting x/y to the packed board index.
pub const BOARD_INDEX_SHIFT: isize = 5;

static PREFERENCES: OnceLock<Mutex<Pref>> = OnceLock::new();

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
            szBegin: [0; CCH_NAME_MAX],
            szInter: [0; CCH_NAME_MAX],
            szExpert: [0; CCH_NAME_MAX],
        })
    })
}

pub static xBoxMac: AtomicI32 = AtomicI32::new(0);

pub static yBoxMac: AtomicI32 = AtomicI32::new(0);

pub static iButtonCur: AtomicU8 = AtomicU8::new(ButtonSprite::Happy as u8);

pub static cBombLeft: AtomicI32 = AtomicI32::new(0);

pub static cSec: AtomicI32 = AtomicI32::new(0);

pub static C_BOX_VISIT: AtomicI32 = AtomicI32::new(0);

pub static xCur: AtomicI32 = AtomicI32::new(-1);

pub static yCur: AtomicI32 = AtomicI32::new(-1);

const RG_BLK_INIT: [i8; C_BLK_MAX] = [BlockCell::BlankUp as i8; C_BLK_MAX];

static RG_BLK: OnceLock<Mutex<[i8; C_BLK_MAX]>> = OnceLock::new();

pub fn board_mutex() -> &'static Mutex<[i8; C_BLK_MAX]> {
    RG_BLK.get_or_init(|| Mutex::new(RG_BLK_INIT))
}

static CBOMB_START: AtomicI32 = AtomicI32::new(0);
static CBOX_VISIT_MAC: AtomicI32 = AtomicI32::new(0);
static F_TIMER: AtomicBool = AtomicBool::new(false);
static F_OLD_TIMER_STATUS: AtomicBool = AtomicBool::new(false);

fn status_play() -> bool {
    (fStatus.load(Ordering::Relaxed) & (StatusFlag::Play as i32)) != 0
}

fn status_pause() -> bool {
    (fStatus.load(Ordering::Relaxed) & (StatusFlag::Pause as i32)) != 0
}

fn set_status_play() {
    fStatus.store(StatusFlag::Play as i32, Ordering::Relaxed)
}

fn set_status_demo() {
    fStatus.store(StatusFlag::Demo as i32, Ordering::Relaxed)
}

fn set_status_pause() {
    fStatus.fetch_or(StatusFlag::Pause as i32, Ordering::Relaxed);
}

fn clr_status_pause() {
    fStatus.fetch_and(!(StatusFlag::Pause as i32), Ordering::Relaxed);
}

fn board_index(x: i32, y: i32) -> Option<usize> {
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    if offset < 0 {
        return None;
    }
    let idx = offset as usize;
    if idx < C_BLK_MAX { Some(idx) } else { None }
}

fn block_value(x: i32, y: i32) -> u8 {
    let guard = match board_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    board_index(x, y)
        .and_then(|idx| guard.get(idx).copied())
        .unwrap_or(0) as u8
}

fn set_block_value(x: i32, y: i32, value: u8) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let prev = guard[idx] as u8;

        // Preserve existing flag bits, but allow callers to explicitly set the Visit bit.
        // (Bomb placement is handled separately via set_bomb/clear_bomb.)
        let flags = (prev & BlockMask::Flags as u8) | (value & BlockMask::Visit as u8);
        let data = value & BlockMask::Data as u8;
        guard[idx] = (flags | data) as i8;
    }
}

fn set_border(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard[idx] = BlockCell::Border as i8;
    }
}

fn set_bomb(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let prev = guard[idx] as u8;
        guard[idx] = (prev | BlockMask::Bomb as u8) as i8;
    }
}

fn clear_bomb(x: i32, y: i32) {
    if let Some(idx) = board_index(x, y) {
        let mut guard = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let prev = guard[idx] as u8;
        guard[idx] = (prev & BlockMask::NotBomb as u8) as i8;
    }
}

fn is_bomb(x: i32, y: i32) -> bool {
    (block_value(x, y) & BlockMask::Bomb as u8) != 0
}

fn is_visit(x: i32, y: i32) -> bool {
    (block_value(x, y) & BlockMask::Visit as u8) != 0
}

fn guessed_bomb(x: i32, y: i32) -> bool {
    block_value(x, y) & BlockMask::Data as u8 == BlockCell::BombUp as u8
}

fn guessed_mark(x: i32, y: i32) -> bool {
    block_value(x, y) & BlockMask::Data as u8 == BlockCell::GuessUp as u8
}

fn f_in_range(x: i32, y: i32) -> bool {
    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

fn set_raw_block(x: i32, y: i32, block: i32) {
    // Keep only the data bits plus the Visit bit (when present).
    let masked = (block & (BlockMask::Data as i32 | BlockMask::Visit as i32)) as u8;
    set_block_value(x, y, masked);
}

fn block_data(x: i32, y: i32) -> i32 {
    (block_value(x, y) & BlockMask::Data as u8) as i32
}

fn check_win() -> bool {
    C_BOX_VISIT.load(Ordering::Relaxed) == CBOX_VISIT_MAC.load(Ordering::Relaxed)
}

fn display_block(x: i32, y: i32) {
    DisplayBlk(x, y);
}

fn display_grid() {
    DisplayGrid();
}

fn display_button(state: ButtonSprite) {
    DisplayButton(state);
}

fn display_time() {
    DisplayTime();
}

fn display_bomb_count() {
    DisplayBombCount();
}

/// Play a logical tune if sound effects are enabled in preferences.
fn play_tune(tune: Tune) {
    let sound_on = {
        let prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.fSound == SoundState::On
    };

    if sound_on {
        PlayTune(tune);
    }
}

fn stop_all_audio() {
    EndTunes();
}

fn show_bombs(cell: BlockCell) {
    // Display hidden bombs and mark incorrect guesses.
    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);

    for y in 1..=y_max {
        for x in 1..=x_max {
            if !is_visit(x, y) {
                if is_bomb(x, y) {
                    if !guessed_bomb(x, y) {
                        set_raw_block(x, y, cell as i32);
                    }
                } else if guessed_bomb(x, y) {
                    set_raw_block(x, y, BlockCell::Wrong as i32);
                }
            }
        }
    }
    display_grid();
}

fn count_marks(x_center: i32, y_center: i32) -> i32 {
    // Count the number of adjacent flagged squares.
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

fn update_button_for_result(win: bool) {
    // Mirror the original happy/win/lose button logic when the game ends.
    let state = if win {
        ButtonSprite::Win
    } else {
        ButtonSprite::Lose
    };
    iButtonCur.store(state as u8, Ordering::Relaxed);
    display_button(state);
}

fn record_win_if_needed() {
    let elapsed = cSec.load(Ordering::Relaxed);
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
            DoEnterName();
            DoDisplayBest();
        }
    }
}

fn change_blk(x: i32, y: i32, block: i32) {
    // Update a single cell and repaint it immediately.
    set_raw_block(x, y, block);
    display_block(x, y);
}

fn step_xy(queue: &mut [(i32, i32); I_STEP_MAX], tail: &mut usize, x: i32, y: i32) {
    // Visit a square; enqueue it when empty so we flood-fill neighbors later.
    if let Some(idx) = board_index(x, y) {
        let mut board = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
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
        display_block(x, y);

        if bombs == 0 && *tail < I_STEP_MAX {
            queue[*tail] = (x, y);
            *tail += 1;
        }
    }
}

fn step_box(x: i32, y: i32) {
    // Flood-fill contiguous empty squares using the same 3x3 sweep as the C version.
    let mut queue = [(0, 0); I_STEP_MAX];
    let mut head = 0usize;
    let mut tail = 0usize;

    step_xy(&mut queue, &mut tail, x, y);

    while head < tail {
        let (sx, sy) = queue[head];
        head += 1;

        let mut ty = sy - 1;
        step_xy(&mut queue, &mut tail, sx - 1, ty);
        step_xy(&mut queue, &mut tail, sx, ty);
        step_xy(&mut queue, &mut tail, sx + 1, ty);

        ty += 1;
        step_xy(&mut queue, &mut tail, sx - 1, ty);
        step_xy(&mut queue, &mut tail, sx + 1, ty);

        ty += 1;
        step_xy(&mut queue, &mut tail, sx - 1, ty);
        step_xy(&mut queue, &mut tail, sx, ty);
        step_xy(&mut queue, &mut tail, sx + 1, ty);
    }
}

fn game_over(win: bool) {
    // Stop the timer, reveal bombs, update the face, and record wins if needed.
    F_TIMER.store(false, Ordering::Relaxed);
    update_button_for_result(win);
    show_bombs(if win {
        BlockCell::BombUp
    } else {
        BlockCell::BombDown
    });
    if win {
        let bombs_left = cBombLeft.load(Ordering::Relaxed);
        if bombs_left != 0 {
            update_bomb_count_internal(-bombs_left);
        }
    }
    play_tune(if win { Tune::WinGame } else { Tune::LoseGame });
    set_status_demo();

    if win {
        record_win_if_needed();
    }
}

fn step_square(x: i32, y: i32) {
    // Handle a user click on a single square (first-click safety included).
    if is_bomb(x, y) {
        let visits = C_BOX_VISIT.load(Ordering::Relaxed);
        if visits == 0 {
            let x_max = xBoxMac.load(Ordering::Relaxed);
            let y_max = yBoxMac.load(Ordering::Relaxed);
            for y_t in 1..y_max {
                for x_t in 1..x_max {
                    if !is_bomb(x_t, y_t) {
                        clear_bomb(x, y);
                        set_bomb(x_t, y_t);
                        step_box(x, y);
                        return;
                    }
                }
            }
        } else {
            change_blk(
                x,
                y,
                (BlockMask::Visit as u8 | BlockCell::Explode as u8) as i32,
            );
            game_over(false);
        }
    } else {
        step_box(x, y);
        if check_win() {
            game_over(true);
        }
    }
}

fn step_block(x_center: i32, y_center: i32) {
    // Chord around a revealed number once the flag count matches its value.
    if !is_visit(x_center, y_center)
        || block_data(x_center, y_center) != count_marks(x_center, y_center)
    {
        TrackMouse(-2, -2);
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
                    x,
                    y,
                    (BlockMask::Visit as u8 | BlockCell::Explode as u8) as i32,
                );
            } else {
                step_box(x, y);
            }
        }
    }

    if lose {
        game_over(false);
    } else if check_win() {
        game_over(true);
    }
}

fn make_guess_internal(x: i32, y: i32) {
    // Cycle through blank -> flag -> question mark states depending on preferences.
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
        update_bomb_count_internal(1);
        if allow_marks {
            BlockCell::GuessUp as i32
        } else {
            BlockCell::BlankUp as i32
        }
    } else if guessed_mark(x, y) {
        BlockCell::BlankUp as i32
    } else {
        update_bomb_count_internal(-1);
        BlockCell::BombUp as i32
    };

    change_blk(x, y, block);

    if guessed_bomb(x, y) && check_win() {
        game_over(true);
    }
}

fn push_box_down(x: i32, y: i32) {
    // Depress covered neighbors while tracking mouse drags.
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == BlockCell::GuessUp as i32 => BlockCell::GuessDown as i32,
        b if b == BlockCell::BlankUp as i32 => BlockCell::Blank as i32,
        _ => blk,
    };
    set_raw_block(x, y, blk);
}

fn pop_box_up(x: i32, y: i32) {
    // Restore a previously pushed square back to its raised variant.
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == BlockCell::GuessDown as i32 => BlockCell::GuessUp as i32,
        b if b == BlockCell::Blank as i32 => BlockCell::BlankUp as i32,
        _ => blk,
    };
    set_raw_block(x, y, blk);
}

fn update_bomb_count_internal(delta: i32) {
    // Adjust the visible bomb counter and repaint the LEDs.
    cBombLeft.fetch_add(delta, Ordering::Relaxed);
    display_bomb_count();
}

fn in_range_step(x: i32, y: i32) -> bool {
    f_in_range(x, y) && !is_visit(x, y) && !guessed_bomb(x, y)
}

pub fn ClearField() {
    // Reset every cell to blank-up and rebuild the sentinel border.
    {
        let mut board = match board_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        board.iter_mut().for_each(|b| *b = BlockCell::BlankUp as i8);
    }

    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);

    for x in 0..=(x_max + 1) {
        set_border(x, 0);
        set_border(x, y_max + 1);
    }
    for y in 0..=(y_max + 1) {
        set_border(0, y);
        set_border(x_max + 1, y);
    }
}

pub fn DoTimer() {
    let secs = cSec.load(Ordering::Relaxed);
    if F_TIMER.load(Ordering::Relaxed) && secs < 999 {
        cSec.store(secs + 1, Ordering::Relaxed);
        display_time();
        play_tune(Tune::Tick);
    }
}

pub fn StartGame() {
    // Reset globals, randomize bombs, and resize the window if the board changed.
    F_TIMER.store(false, Ordering::Relaxed);

    let x_prev = xBoxMac.load(Ordering::Relaxed);
    let y_prev = yBoxMac.load(Ordering::Relaxed);

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

    xBoxMac.store(pref_width, Ordering::Relaxed);
    yBoxMac.store(pref_height, Ordering::Relaxed);

    ClearField();
    iButtonCur.store(ButtonSprite::Happy as u8, Ordering::Relaxed);

    CBOMB_START.store(pref_mines, Ordering::Relaxed);

    let total_bombs = CBOMB_START.load(Ordering::Relaxed);
    let width = xBoxMac.load(Ordering::Relaxed);
    let height = yBoxMac.load(Ordering::Relaxed);

    let mut bombs = total_bombs;
    while bombs > 0 {
        let mut x;
        let mut y;
        loop {
            x = Rnd(width) + 1;
            y = Rnd(height) + 1;
            if !is_bomb(x, y) {
                break;
            }
        }
        set_bomb(x, y);
        bombs -= 1;
    }

    cSec.store(0, Ordering::Relaxed);
    cBombLeft.store(total_bombs, Ordering::Relaxed);
    C_BOX_VISIT.store(0, Ordering::Relaxed);
    CBOX_VISIT_MAC.store((width * height) - total_bombs, Ordering::Relaxed);
    set_status_play();

    display_bomb_count();

    AdjustWindow(f_adjust);
}

pub fn TrackMouse(x_new: i32, y_new: i32) {
    // Provide the classic pressed-square feedback during mouse drags.
    let x_old = xCur.load(Ordering::Relaxed);
    let y_old = yCur.load(Ordering::Relaxed);

    if x_new == x_old && y_new == y_old {
        return;
    }

    xCur.store(x_new, Ordering::Relaxed);
    yCur.store(y_new, Ordering::Relaxed);

    let y_max = yBoxMac.load(Ordering::Relaxed);
    let x_max = xBoxMac.load(Ordering::Relaxed);

    if fBlock.load(Ordering::Relaxed) {
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
                    display_block(x, y);
                }
            }
        }

        if valid_new {
            for y in y_cur_min..=y_cur_max {
                for x in x_cur_min..=x_cur_max {
                    display_block(x, y);
                }
            }
        }
    } else {
        if f_in_range(x_old, y_old) && !is_visit(x_old, y_old) {
            pop_box_up(x_old, y_old);
            display_block(x_old, y_old);
        }
        if f_in_range(x_new, y_new) && in_range_step(x_new, y_new) {
            push_box_down(x_new, y_new);
            display_block(x_new, y_new);
        }
    }
}

pub fn MakeGuess(x: i32, y: i32) {
    // Toggle through flag/question mark states and update the bomb counter.
    make_guess_internal(x, y);
}

pub fn DoButton1Up() {
    // Handle a left-button release: start the timer, then either chord or step.
    let x_pos = xCur.load(Ordering::Relaxed);
    let y_pos = yCur.load(Ordering::Relaxed);

    if f_in_range(x_pos, y_pos) {
        let visits = C_BOX_VISIT.load(Ordering::Relaxed);
        let secs = cSec.load(Ordering::Relaxed);
        if visits == 0 && secs == 0 {
            play_tune(Tune::Tick);
            cSec.store(1, Ordering::Relaxed);
            display_time();
            F_TIMER.store(true, Ordering::Relaxed);
            let hwnd_guard = match global_state().hwnd_main.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(hwnd) = hwnd_guard.as_opt()
                && hwnd.SetTimer(ID_TIMER, 1000, None).is_err()
            {
                ReportErr(ID_ERR_TIMER);
            }
        }

        if !status_play() {
            xCur.store(-2, Ordering::Relaxed);
            yCur.store(-2, Ordering::Relaxed);
        }

        if fBlock.load(Ordering::Relaxed) {
            step_block(x_pos, y_pos);
        } else if in_range_step(x_pos, y_pos) {
            step_square(x_pos, y_pos);
        }
    }

    let button = match iButtonCur.load(Ordering::Relaxed) {
        0 => ButtonSprite::Happy,
        1 => ButtonSprite::Caution,
        2 => ButtonSprite::Lose,
        3 => ButtonSprite::Win,
        _ => ButtonSprite::Down,
    };
    display_button(button);
}

pub fn PauseGame() {
    // Pause by silencing audio, remembering timer state, and setting the flag.
    stop_all_audio();

    if !status_pause() {
        F_OLD_TIMER_STATUS.store(F_TIMER.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    if status_play() {
        F_TIMER.store(false, Ordering::Relaxed);
    }

    set_status_pause();
}

pub fn ResumeGame() {
    // Resume from pause by restoring the timer state and clearing the flag.
    if status_play() {
        F_TIMER.store(
            F_OLD_TIMER_STATUS.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }
    clr_status_pause();
}
