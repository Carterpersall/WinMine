use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use winsafe::prelude::*;

use crate::globals::{fBlock, fStatus, hwndMain};
use crate::grafix::{DisplayBlk, DisplayBombCount, DisplayButton, DisplayGrid, DisplayTime};
use crate::pref::{Pref, CCH_NAME_MAX};
use crate::sound::{EndTunes, PlayTune};
use crate::util::{ReportErr, Rnd};
use crate::winmine::{AdjustWindow, DoDisplayBest, DoEnterName};

const I_BLK_BLANK: i32 = 0;
const I_BLK_GUESS_DOWN: i32 = 9;
const I_BLK_BOMB_DOWN: i32 = 10;
const I_BLK_WRONG: i32 = 11;
const I_BLK_EXPLODE: i32 = 12;
const I_BLK_GUESS_UP: i32 = 13;
const I_BLK_BOMB_UP: i32 = 14;
const I_BLK_BLANK_UP: i32 = 15;
const I_BLK_MAX_SENTINEL: i32 = 16;

const I_BUTTON_HAPPY: i32 = 0;
const I_BUTTON_WIN: i32 = 3;
const I_BUTTON_LOSE: i32 = 2;

const MASK_BOMB: u8 = 0x80;
const MASK_VISIT: u8 = 0x40;
const MASK_FLAGS: u8 = 0xE0;
const MASK_DATA: u8 = 0x1F;
const MASK_NOT_BOMB: u8 = !MASK_BOMB;

const C_BLK_MAX: usize = 27 * 32;
const I_STEP_MAX: usize = 100;

const TUNE_TICK: i32 = 1;
const TUNE_WINGAME: i32 = 2;
const TUNE_LOSEGAME: i32 = 3;

const W_GAME_OTHER: i32 = 3;

const ID_TIMER: usize = 1;
const ID_ERR_TIMER: u16 = 4;

const F_PLAY: i32 = 0x01;
const F_PAUSE: i32 = 0x02;
const F_DEMO: i32 = 0x10;

const F_RESIZE: i32 = 0x02;
const F_DISPLAY: i32 = 0x04;

pub static mut Preferences: Pref = Pref {
    wGameType: 0,
    Mines: 0,
    Height: 0,
    Width: 0,
    xWindow: 0,
    yWindow: 0,
    fSound: 0,
    fMark: false,
    fTick: false,
    fMenu: 0,
    fColor: false,
    rgTime: [0; 3],
    szBegin: [0; CCH_NAME_MAX],
    szInter: [0; CCH_NAME_MAX],
    szExpert: [0; CCH_NAME_MAX],
};

pub static xBoxMac: AtomicI32 = AtomicI32::new(0);

pub static yBoxMac: AtomicI32 = AtomicI32::new(0);

pub static iButtonCur: AtomicI32 = AtomicI32::new(I_BUTTON_HAPPY);

pub static cBombLeft: AtomicI32 = AtomicI32::new(0);

pub static cSec: AtomicI32 = AtomicI32::new(0);

pub static C_BOX_VISIT: AtomicI32 = AtomicI32::new(0);

pub static xCur: AtomicI32 = AtomicI32::new(-1);

pub static yCur: AtomicI32 = AtomicI32::new(-1);

pub static mut rgBlk: [i8; C_BLK_MAX] = [I_BLK_BLANK_UP as i8; C_BLK_MAX];

static CBOMB_START: AtomicI32 = AtomicI32::new(0);
static CBOX_VISIT_MAC: AtomicI32 = AtomicI32::new(0);
static F_TIMER: AtomicBool = AtomicBool::new(false);
static F_OLD_TIMER_STATUS: AtomicBool = AtomicBool::new(false);

fn status_play() -> bool {
    (fStatus.load(Ordering::Relaxed) & F_PLAY) != 0
}

fn status_pause() -> bool {
    (fStatus.load(Ordering::Relaxed) & F_PAUSE) != 0
}

fn set_status_play() {
    fStatus.store(F_PLAY, Ordering::Relaxed)
}

fn set_status_demo() {
    fStatus.store(F_DEMO, Ordering::Relaxed)
}

fn set_status_pause() {
    fStatus.fetch_or(F_PAUSE, Ordering::Relaxed);
}

fn clr_status_pause() {
    fStatus.fetch_and(!F_PAUSE, Ordering::Relaxed);
}

fn board_index(x: i32, y: i32) -> usize {
    let offset = ((y as isize) << 5) + x as isize;
    offset.max(0) as usize
}

fn block_value(x: i32, y: i32) -> u8 {
    unsafe { rgBlk[board_index(x, y)] as u8 }
}

fn set_block_value(x: i32, y: i32, value: u8) {
    unsafe {
        let idx = board_index(x, y);
        let prev = rgBlk[idx] as u8;
        rgBlk[idx] = ((prev & MASK_FLAGS) | (value & MASK_DATA)) as i8;
    }
}

fn set_border(x: i32, y: i32) {
    unsafe {
        rgBlk[board_index(x, y)] = I_BLK_MAX_SENTINEL as i8;
    }
}

fn set_bomb(x: i32, y: i32) {
    unsafe {
        let idx = board_index(x, y);
        let prev = rgBlk[idx] as u8;
        rgBlk[idx] = (prev | MASK_BOMB) as i8;
    }
}

fn clear_bomb(x: i32, y: i32) {
    unsafe {
        let idx = board_index(x, y);
        let prev = rgBlk[idx] as u8;
        rgBlk[idx] = (prev & MASK_NOT_BOMB) as i8;
    }
}

fn is_bomb(x: i32, y: i32) -> bool {
    (block_value(x, y) & MASK_BOMB) != 0
}

fn is_visit(x: i32, y: i32) -> bool {
    (block_value(x, y) & MASK_VISIT) != 0
}

fn guessed_bomb(x: i32, y: i32) -> bool {
    block_value(x, y) & MASK_DATA == I_BLK_BOMB_UP as u8
}

fn guessed_mark(x: i32, y: i32) -> bool {
    block_value(x, y) & MASK_DATA == I_BLK_GUESS_UP as u8
}

fn f_in_range(x: i32, y: i32) -> bool {
    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

fn clamp_board_value(value: i32) -> u8 {
    (value & MASK_DATA as i32) as u8
}

fn set_raw_block(x: i32, y: i32, block: i32) {
    set_block_value(x, y, clamp_board_value(block));
}

fn block_data(x: i32, y: i32) -> i32 {
    (block_value(x, y) & MASK_DATA) as i32
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

fn display_button(state: i32) {
    DisplayButton(state);
}

fn display_time() {
    DisplayTime();
}

fn display_bomb_count() {
    DisplayBombCount();
}

fn play_tune(which: i32) {
    PlayTune(which);
}

fn stop_all_audio() {
    EndTunes();
}

fn show_bombs(i_blk: i32) {
    // Display hidden bombs and mark incorrect guesses.
    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);

    for y in 1..=y_max {
        for x in 1..=x_max {
            if !is_visit(x, y) {
                if is_bomb(x, y) {
                    if !guessed_bomb(x, y) {
                        set_raw_block(x, y, i_blk);
                    }
                } else if guessed_bomb(x, y) {
                    set_raw_block(x, y, I_BLK_WRONG);
                }
            }
        }
    }
    display_grid();
}

fn count_bombs(x_center: i32, y_center: i32) -> i32 {
    // Count the bombs surrounding the target square.
    let mut c_bombs = 0;
    for y in (y_center - 1)..=(y_center + 1) {
        for x in (x_center - 1)..=(x_center + 1) {
            if is_bomb(x, y) {
                c_bombs += 1;
            }
        }
    }
    c_bombs
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
    let state = if win { I_BUTTON_WIN } else { I_BUTTON_LOSE };
    iButtonCur.store(state, Ordering::Relaxed);
    display_button(state);
}

fn record_win_if_needed() {
    unsafe {
        let elapsed = cSec.load(Ordering::Relaxed);
        if Preferences.wGameType != W_GAME_OTHER as u16
            && elapsed < Preferences.rgTime[Preferences.wGameType as usize]
        {
            Preferences.rgTime[Preferences.wGameType as usize] = elapsed;
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
    unsafe {
        let idx = board_index(x, y);
        let mut blk = rgBlk[idx] as u8;
        if (blk & MASK_VISIT) != 0 {
            return;
        }

        let data = blk & MASK_DATA;
        if data == I_BLK_MAX_SENTINEL as u8 || data == I_BLK_BOMB_UP as u8 {
            return;
        }

        C_BOX_VISIT.fetch_add(1, Ordering::Relaxed);
        let bombs = count_bombs(x, y);
        blk = MASK_VISIT | (bombs as u8 & MASK_DATA);
        rgBlk[idx] = blk as i8;
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
    show_bombs(if win { I_BLK_BOMB_UP } else { I_BLK_BOMB_DOWN });
    if win {
        let bombs_left = cBombLeft.load(Ordering::Relaxed);
        if bombs_left != 0 {
            update_bomb_count_internal(-bombs_left);
        }
    }
    play_tune(if win { TUNE_WINGAME } else { TUNE_LOSEGAME });
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
            change_blk(x, y, (MASK_VISIT | I_BLK_EXPLODE as u8) as i32);
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
                change_blk(x, y, (MASK_VISIT | I_BLK_EXPLODE as u8) as i32);
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
    unsafe {
        if !f_in_range(x, y) || is_visit(x, y) {
            return;
        }

        let block = if guessed_bomb(x, y) {
            update_bomb_count_internal(1);
            if Preferences.fMark {
                I_BLK_GUESS_UP
            } else {
                I_BLK_BLANK_UP
            }
        } else if guessed_mark(x, y) {
            I_BLK_BLANK_UP
        } else {
            update_bomb_count_internal(-1);
            I_BLK_BOMB_UP
        };

        change_blk(x, y, block);

        if guessed_bomb(x, y) && check_win() {
            game_over(true);
        }
    }
}

fn push_box_down(x: i32, y: i32) {
    // Depress covered neighbors while tracking mouse drags.
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == I_BLK_GUESS_UP => I_BLK_GUESS_DOWN,
        b if b == I_BLK_BLANK_UP => I_BLK_BLANK,
        _ => blk,
    };
    set_raw_block(x, y, blk);
}

fn pop_box_up(x: i32, y: i32) {
    // Restore a previously pushed square back to its raised variant.
    let mut blk = block_data(x, y);
    blk = match blk {
        b if b == I_BLK_GUESS_DOWN => I_BLK_GUESS_UP,
        b if b == I_BLK_BLANK => I_BLK_BLANK_UP,
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
    unsafe {
        #[allow(clippy::needless_range_loop)]
        for idx in 0..C_BLK_MAX {
            rgBlk[idx] = I_BLK_BLANK_UP as i8;
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
}

pub fn DoTimer() {
    let secs = cSec.load(Ordering::Relaxed);
    if F_TIMER.load(Ordering::Relaxed) && secs < 999 {
        cSec.store(secs + 1, Ordering::Relaxed);
        display_time();
        play_tune(TUNE_TICK);
    }
}

pub fn StartGame() {
    // Reset globals, randomize bombs, and resize the window if the board changed.
    F_TIMER.store(false, Ordering::Relaxed);

    let x_prev = xBoxMac.load(Ordering::Relaxed);
    let y_prev = yBoxMac.load(Ordering::Relaxed);

    let (pref_width, pref_height, pref_mines) = unsafe {
        (Preferences.Width, Preferences.Height, Preferences.Mines)
    };

    let f_adjust = if pref_width != x_prev || pref_height != y_prev {
        F_RESIZE | F_DISPLAY
    } else {
        F_DISPLAY
    };

    xBoxMac.store(pref_width, Ordering::Relaxed);
    yBoxMac.store(pref_height, Ordering::Relaxed);

    ClearField();
    iButtonCur.store(I_BUTTON_HAPPY, Ordering::Relaxed);

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
            play_tune(TUNE_TICK);
            cSec.store(1, Ordering::Relaxed);
            display_time();
            F_TIMER.store(true, Ordering::Relaxed);
            if let Some(hwnd) = unsafe { hwndMain.as_opt() }
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

    let button = iButtonCur.load(Ordering::Relaxed);
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
