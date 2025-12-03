use core::cmp::{max, min};
use core::ffi::c_int;

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, TRUE, FALSE};
use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;

use crate::grafix::{DisplayBlk, DisplayBombCount, DisplayButton, DisplayGrid, DisplayTime};
use crate::pref::{PREF, CCH_NAME_MAX};
use crate::sound::{EndTunes, PlayTune};
use crate::util::{ReportErr, Rnd};

const I_BLK_BLANK: c_int = 0;
const I_BLK_GUESS_DOWN: c_int = 9;
const I_BLK_BOMB_DOWN: c_int = 10;
const I_BLK_WRONG: c_int = 11;
const I_BLK_EXPLODE: c_int = 12;
const I_BLK_GUESS_UP: c_int = 13;
const I_BLK_BOMB_UP: c_int = 14;
const I_BLK_BLANK_UP: c_int = 15;
const I_BLK_MAX_SENTINEL: c_int = 16;

const I_BUTTON_HAPPY: c_int = 0;
const I_BUTTON_WIN: c_int = 3;
const I_BUTTON_LOSE: c_int = 2;

const MASK_BOMB: u8 = 0x80;
const MASK_VISIT: u8 = 0x40;
const MASK_FLAGS: u8 = 0xE0;
const MASK_DATA: u8 = 0x1F;
const MASK_NOT_BOMB: u8 = !MASK_BOMB;

const C_BLK_MAX: usize = 27 * 32;
const I_STEP_MAX: usize = 100;

const TUNE_TICK: c_int = 1;
const TUNE_WINGAME: c_int = 2;
const TUNE_LOSEGAME: c_int = 3;

const W_GAME_OTHER: c_int = 3;

const ID_TIMER: usize = 1;
const ID_ERR_TIMER: u16 = 4;

const F_PLAY: c_int = 0x01;
const F_PAUSE: c_int = 0x02;
const F_DEMO: c_int = 0x10;

const F_RESIZE: c_int = 0x02;
const F_DISPLAY: c_int = 0x04;

#[no_mangle]
pub static mut Preferences: PREF = PREF {
	wGameType: 0,
	Mines: 0,
	Height: 0,
	Width: 0,
	xWindow: 0,
	yWindow: 0,
	fSound: 0,
	fMark: FALSE,
	fTick: FALSE,
	fMenu: 0,
	fColor: FALSE,
	rgTime: [0; 3],
	szBegin: [0; CCH_NAME_MAX],
	szInter: [0; CCH_NAME_MAX],
	szExpert: [0; CCH_NAME_MAX],
};

#[no_mangle]
pub static mut xBoxMac: c_int = 0;
#[no_mangle]
pub static mut yBoxMac: c_int = 0;

#[no_mangle]
pub static mut iButtonCur: c_int = I_BUTTON_HAPPY;
#[no_mangle]
pub static mut cBombLeft: c_int = 0;
#[no_mangle]
pub static mut cSec: c_int = 0;
#[no_mangle]
pub static mut cBoxVisit: c_int = 0;

#[no_mangle]
pub static mut xCur: c_int = -1;
#[no_mangle]
pub static mut yCur: c_int = -1;

#[no_mangle]
pub static mut rgBlk: [i8; C_BLK_MAX] = [I_BLK_BLANK_UP as i8; C_BLK_MAX];

static mut CBOMB_START: c_int = 0;
static mut CBOX_VISIT_MAC: c_int = 0;
static mut F_TIMER: BOOL = FALSE;
static mut F_OLD_TIMER_STATUS: BOOL = FALSE;

extern "C" {
	static mut hwndMain: HWND;
	static mut fBlock: BOOL;
	static mut fStatus: c_int;

	fn AdjustWindow(flags: c_int);
	fn DoEnterName();
	fn DoDisplayBest();
}

fn bool_from_bool32(value: BOOL) -> bool {
	value != 0
}

fn status_play() -> bool {
	unsafe { (fStatus & F_PLAY) != 0 }
}

fn status_pause() -> bool {
	unsafe { (fStatus & F_PAUSE) != 0 }
}

fn set_status_play() {
	unsafe { fStatus = F_PLAY; }
}

fn set_status_demo() {
	unsafe { fStatus = F_DEMO; }
}

fn set_status_pause() {
	unsafe { fStatus |= F_PAUSE; }
}

fn clr_status_pause() {
	unsafe { fStatus &= !F_PAUSE; }
}

fn board_index(x: c_int, y: c_int) -> usize {
	let offset = ((y as isize) << 5) + x as isize;
	offset.max(0) as usize
}

fn block_value(x: c_int, y: c_int) -> u8 {
	unsafe { rgBlk[board_index(x, y)] as u8 }
}

fn set_block_value(x: c_int, y: c_int, value: u8) {
	unsafe {
		let idx = board_index(x, y);
		let prev = rgBlk[idx] as u8;
		rgBlk[idx] = ((prev & MASK_FLAGS) | (value & MASK_DATA)) as i8;
	}
}

fn set_border(x: c_int, y: c_int) {
	unsafe { rgBlk[board_index(x, y)] = I_BLK_MAX_SENTINEL as i8; }
}

fn set_bomb(x: c_int, y: c_int) {
	unsafe {
		let idx = board_index(x, y);
		let prev = rgBlk[idx] as u8;
		rgBlk[idx] = (prev | MASK_BOMB) as i8;
	}
}

fn clear_bomb(x: c_int, y: c_int) {
	unsafe {
		let idx = board_index(x, y);
		let prev = rgBlk[idx] as u8;
		rgBlk[idx] = (prev & MASK_NOT_BOMB) as i8;
	}
}

fn is_bomb(x: c_int, y: c_int) -> bool {
	(block_value(x, y) & MASK_BOMB) != 0
}

fn is_visit(x: c_int, y: c_int) -> bool {
	(block_value(x, y) & MASK_VISIT) != 0
}

fn guessed_bomb(x: c_int, y: c_int) -> bool {
	block_value(x, y) & MASK_DATA == I_BLK_BOMB_UP as u8
}

fn guessed_mark(x: c_int, y: c_int) -> bool {
	block_value(x, y) & MASK_DATA == I_BLK_GUESS_UP as u8
}

fn f_in_range(x: c_int, y: c_int) -> bool {
	unsafe { x > 0 && y > 0 && x <= xBoxMac && y <= yBoxMac }
}

fn clamp_board_value(value: c_int) -> u8 {
	(value & MASK_DATA as c_int) as u8
}

fn set_raw_block(x: c_int, y: c_int, block: c_int) {
	set_block_value(x, y, clamp_board_value(block));
}

fn block_data(x: c_int, y: c_int) -> c_int {
	(block_value(x, y) & MASK_DATA) as c_int
}

fn check_win() -> bool {
	unsafe { cBoxVisit == CBOX_VISIT_MAC }
}

fn display_block(x: c_int, y: c_int) {
	unsafe { DisplayBlk(x, y) };
}

fn display_grid() {
	unsafe { DisplayGrid() };
}

fn display_button(state: c_int) {
	unsafe { DisplayButton(state) };
}

fn display_time() {
	unsafe { DisplayTime() };
}

fn display_bomb_count() {
	unsafe { DisplayBombCount() };
}

fn play_tune(which: c_int) {
	PlayTune(which);
}

fn stop_all_audio() {
	EndTunes();
}

fn show_bombs(i_blk: c_int) {
	// Display hidden bombs and mark incorrect guesses.
	unsafe {
		for y in 1..=yBoxMac {
			for x in 1..=xBoxMac {
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
	}
	display_grid();
}

fn count_bombs(x_center: c_int, y_center: c_int) -> c_int {
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

fn count_marks(x_center: c_int, y_center: c_int) -> c_int {
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
	unsafe {
		iButtonCur = if win { I_BUTTON_WIN } else { I_BUTTON_LOSE };
		display_button(iButtonCur);
	}
}

fn record_win_if_needed() {
	unsafe {
		if Preferences.wGameType != W_GAME_OTHER as u16 && cSec < Preferences.rgTime[Preferences.wGameType as usize] {
			Preferences.rgTime[Preferences.wGameType as usize] = cSec;
			DoEnterName();
			DoDisplayBest();
		}
	}
}

fn change_blk(x: c_int, y: c_int, block: c_int) {
	// Update a single cell and repaint it immediately.
	set_raw_block(x, y, block);
	display_block(x, y);
}

fn step_xy(queue: &mut [(c_int, c_int); I_STEP_MAX], tail: &mut usize, x: c_int, y: c_int) {
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

		cBoxVisit += 1;
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

fn step_box(x: c_int, y: c_int) {
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
	unsafe {
		F_TIMER = FALSE;
		update_button_for_result(win);
		show_bombs(if win { I_BLK_BOMB_UP } else { I_BLK_BOMB_DOWN });
		if win && cBombLeft != 0 {
			update_bomb_count_internal(-cBombLeft);
		}
	}
	play_tune(if win { TUNE_WINGAME } else { TUNE_LOSEGAME });
	set_status_demo();

	if win {
		record_win_if_needed();
	}
}

fn step_square(x: c_int, y: c_int) {
	// Handle a user click on a single square (first-click safety included).
	unsafe {
		if is_bomb(x, y) {
			if cBoxVisit == 0 {
				for y_t in 1..yBoxMac {
					for x_t in 1..xBoxMac {
						if !is_bomb(x_t, y_t) {
							clear_bomb(x, y);
							set_bomb(x_t, y_t);
							step_box(x, y);
							return;
						}
					}
				}
			} else {
				change_blk(x, y, (MASK_VISIT | I_BLK_EXPLODE as u8) as c_int);
				game_over(false);
			}
		} else {
			step_box(x, y);
			if check_win() {
				game_over(true);
			}
		}
	}
}

fn step_block(x_center: c_int, y_center: c_int) {
	// Chord around a revealed number once the flag count matches its value.
	if !is_visit(x_center, y_center)
		|| block_data(x_center, y_center) != count_marks(x_center, y_center)
	{
		unsafe {
			TrackMouse(-2, -2);
		}
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
				change_blk(x, y, (MASK_VISIT | I_BLK_EXPLODE as u8) as c_int);
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

fn make_guess_internal(x: c_int, y: c_int) {
	// Cycle through blank -> flag -> question mark states depending on preferences.
	unsafe {
		if !f_in_range(x, y) || is_visit(x, y) {
			return;
		}

		let block = if guessed_bomb(x, y) {
			update_bomb_count_internal(1);
			if Preferences.fMark != 0 {
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

fn push_box_down(x: c_int, y: c_int) {
	// Depress covered neighbors while tracking mouse drags.
	let mut blk = block_data(x, y);
	blk = match blk {
		b if b == I_BLK_GUESS_UP => I_BLK_GUESS_DOWN,
		b if b == I_BLK_BLANK_UP => I_BLK_BLANK,
		_ => blk,
	};
	set_raw_block(x, y, blk);
}

fn pop_box_up(x: c_int, y: c_int) {
	// Restore a previously pushed square back to its raised variant.
	let mut blk = block_data(x, y);
	blk = match blk {
		b if b == I_BLK_GUESS_DOWN => I_BLK_GUESS_UP,
		b if b == I_BLK_BLANK => I_BLK_BLANK_UP,
		_ => blk,
	};
	set_raw_block(x, y, blk);
}

fn update_bomb_count_internal(delta: c_int) {
	// Adjust the visible bomb counter and repaint the LEDs.
	unsafe {
		cBombLeft += delta;
	}
	display_bomb_count();
}

fn in_range_step(x: c_int, y: c_int) -> bool {
	f_in_range(x, y) && !is_visit(x, y) && !guessed_bomb(x, y)
}

#[no_mangle]
pub unsafe extern "C" fn ClearField() {
	// Reset every cell to blank-up and rebuild the sentinel border.
	#[allow(clippy::needless_range_loop)]
	for idx in 0..C_BLK_MAX {
		rgBlk[idx] = I_BLK_BLANK_UP as i8;
	}

	for x in 0..=(xBoxMac + 1) {
		set_border(x, 0);
		set_border(x, yBoxMac + 1);
	}
	for y in 0..=(yBoxMac + 1) {
		set_border(0, y);
		set_border(xBoxMac + 1, y);
	}
}

#[no_mangle]
pub unsafe extern "C" fn DoTimer() {
	if F_TIMER != FALSE && cSec < 999 {
		cSec += 1;
		display_time();
		play_tune(TUNE_TICK);
	}
}

#[no_mangle]
pub unsafe extern "C" fn StartGame() {
	// Reset globals, randomize bombs, and resize the window if the board changed.
	F_TIMER = FALSE;

	let f_adjust = if Preferences.Width != xBoxMac || Preferences.Height != yBoxMac {
		F_RESIZE | F_DISPLAY
	} else {
		F_DISPLAY
	};

	xBoxMac = Preferences.Width;
	yBoxMac = Preferences.Height;

	ClearField();
	iButtonCur = I_BUTTON_HAPPY;

	CBOMB_START = Preferences.Mines;

	let mut bombs = CBOMB_START;
	while bombs > 0 {
		let mut x;
		let mut y;
		loop {
			x = Rnd(xBoxMac) + 1;
			y = Rnd(yBoxMac) + 1;
			if !is_bomb(x, y) {
				break;
			}
		}
		set_bomb(x, y);
		bombs -= 1;
	}

	cSec = 0;
	cBombLeft = CBOMB_START;
	cBoxVisit = 0;
	CBOX_VISIT_MAC = (xBoxMac * yBoxMac) - cBombLeft;
	set_status_play();

	display_bomb_count();

	AdjustWindow(f_adjust);
}

#[no_mangle]
pub unsafe extern "C" fn TrackMouse(x_new: c_int, y_new: c_int) {
	// Provide the classic pressed-square feedback during mouse drags.
	if x_new == xCur && y_new == yCur {
		return;
	}

	let x_old = xCur;
	let y_old = yCur;
	xCur = x_new;
	yCur = y_new;

	if bool_from_bool32(fBlock) {
		let valid_new = f_in_range(x_new, y_new);
		let valid_old = f_in_range(x_old, y_old);

		let y_old_min = max(y_old - 1, 1);
		let y_old_max = min(y_old + 1, yBoxMac);
		let y_cur_min = max(y_new - 1, 1);
		let y_cur_max = min(y_new + 1, yBoxMac);
		let x_old_min = max(x_old - 1, 1);
		let x_old_max = min(x_old + 1, xBoxMac);
		let x_cur_min = max(x_new - 1, 1);
		let x_cur_max = min(x_new + 1, xBoxMac);

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

#[no_mangle]
pub unsafe extern "C" fn MakeGuess(x: c_int, y: c_int) {
	// Toggle through flag/question mark states and update the bomb counter.
	make_guess_internal(x, y);
}

#[no_mangle]
pub unsafe extern "C" fn DoButton1Up() {
	// Handle a left-button release: start the timer, then either chord or step.
	if f_in_range(xCur, yCur) {
		if cBoxVisit == 0 && cSec == 0 {
			play_tune(TUNE_TICK);
			cSec = 1;
			display_time();
			F_TIMER = TRUE;
			if SetTimer(hwndMain, ID_TIMER, 1000, None) == 0 {
				ReportErr(ID_ERR_TIMER);
			}
		}

		if !status_play() {
			xCur = -2;
			yCur = -2;
		}

		if bool_from_bool32(fBlock) {
			step_block(xCur, yCur);
		} else if in_range_step(xCur, yCur) {
			step_square(xCur, yCur);
		}
	}

	display_button(iButtonCur);
}

#[no_mangle]
pub unsafe extern "C" fn PauseGame() {
	// Pause by silencing audio, remembering timer state, and setting the flag.
	stop_all_audio();

	if !status_pause() {
		F_OLD_TIMER_STATUS = F_TIMER;
	}
	if status_play() {
		F_TIMER = FALSE;
	}

	set_status_pause();
}

#[no_mangle]
pub unsafe extern "C" fn ResumeGame() {
	// Resume from pause by restoring the timer state and clearing the flag.
	if status_play() {
		F_TIMER = F_OLD_TIMER_STATUS;
	}
	clr_status_pause();
}

#[no_mangle]
pub unsafe extern "C" fn UpdateBombCount(bomb_adjust: c_int) {
	// Entry point used by the C UI to keep the bomb LEDs in sync.
	update_bomb_count_internal(bomb_adjust);
}

