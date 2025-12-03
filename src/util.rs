use core::ffi::c_int;
use core::ptr::{addr_of, addr_of_mut, null_mut};

use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::core::{w, BOOL, PCSTR, PCWSTR};
use windows_sys::Win32::Foundation::{HWND, TRUE, FALSE};
use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, HDC, NUMCOLORS};
use windows_sys::Win32::System::WindowsProgramming::{GetPrivateProfileIntW, GetPrivateProfileStringW};
use windows_sys::Win32::System::Registry::{RegCloseKey, RegCreateKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameA;
use windows_sys::Win32::UI::Shell::ShellAboutW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
	CheckMenuItem, GetDesktopWindow, GetDlgItemInt, GetSystemMetrics, LoadIconW, LoadStringW, MessageBoxW, HMENU,
	SetMenu, wsprintfW, HELP_HELPONHELP, MF_CHECKED, MF_UNCHECKED, SM_CXBORDER, SM_CYCAPTION, SM_CYBORDER, SM_CYMENU,
};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::pref::{
	self, CCH_NAME_MAX, DEFHEIGHT, DEFWIDTH, FMENU_ALWAYS_ON, FMENU_ON, FSOUND_ON, ISZ_PREF_MAX, MINHEIGHT, MINWIDTH,
	PREF, WGAME_BEGIN, WGAME_EXPERT, WGAME_INTER,
};
use crate::pref::{ReadInt, WritePreferences, g_hReg, rgszPref};
use crate::rtns::Preferences;
use crate::sound::FInitTunes;
use crate::globals::{dypBorder, dxpBorder, dypCaption, dypMenu, hInst, hMenu, hwndMain, szClass, szDefaultName, szTime};
use crate::winmine::{AdjustWindow, FixMenus};

const RNG_MULTIPLIER: u32 = 1_103_515_245;
const RNG_INCREMENT: u32 = 12_345;
const RNG_DEFAULT_SEED: u32 = 0xACE1_1234;

static RNG_STATE: AtomicU32 = AtomicU32::new(RNG_DEFAULT_SEED);

const ID_GAMENAME: u32 = 1;
const ID_MSG_SEC: u32 = 7;
const ID_NAME_DEFAULT: u32 = 8;
const ID_MSG_VERSION: u32 = 12;
const ID_MSG_CREDIT: u32 = 13;
const ID_ICON_MAIN: u16 = 100;
const ID_ERR_TITLE: u32 = 3;
const ID_ERR_UNKNOWN: u32 = 6;
const ID_ERR_MAX: u32 = 999;

const ISZ_PREF_GAME: usize = 0;
const ISZ_PREF_MINES: usize = 1;
const ISZ_PREF_HEIGHT: usize = 2;
const ISZ_PREF_WIDTH: usize = 3;
const ISZ_PREF_XWINDOW: usize = 4;
const ISZ_PREF_YWINDOW: usize = 5;
const ISZ_PREF_SOUND: usize = 6;
const ISZ_PREF_MARK: usize = 7;
const ISZ_PREF_MENU: usize = 8;
const ISZ_PREF_TICK: usize = 9;
const ISZ_PREF_COLOR: usize = 10;
const ISZ_PREF_BEGIN_TIME: usize = 11;
const ISZ_PREF_BEGIN_NAME: usize = 12;
const ISZ_PREF_INTER_TIME: usize = 13;
const ISZ_PREF_INTER_NAME: usize = 14;
const ISZ_PREF_EXPERT_TIME: usize = 15;
const ISZ_PREF_EXPERT_NAME: usize = 16;
const ISZ_PREF_ALREADY_PLAYED: usize = 17;

const CCH_MSG_MAX: usize = 128;
const CCH_MAX_PATHNAME: usize = 250;
const F_RESIZE: c_int = 0x02;
const FMENU_FLAG_OFF: c_int = 0x01;

const SZ_INI_FILE: PCWSTR = w!("entpack.ini");

fn seed_rng(seed: u32) {
	let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
	RNG_STATE.store(value, Ordering::Relaxed);
}

fn next_rand() -> c_int {
	let mut current = RNG_STATE.load(Ordering::Relaxed);
	loop {
		let next = current
			.wrapping_mul(RNG_MULTIPLIER)
			.wrapping_add(RNG_INCREMENT);
		match RNG_STATE.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => return ((next >> 16) & 0x7FFF) as c_int,
			Err(actual) => current = actual,
		}
	}
}

#[inline]
unsafe fn prefs_mut() -> *mut PREF {
	addr_of_mut!(Preferences)
}

#[inline]
unsafe fn prefs_ref() -> *const PREF {
	addr_of!(Preferences)
}

#[inline]
unsafe fn class_ptr() -> PCWSTR {
	addr_of!(szClass[0])
}

#[inline]
unsafe fn default_name_ptr() -> PCWSTR {
	addr_of!(szDefaultName[0])
}

fn clamp(value: c_int, min: c_int, max: c_int) -> c_int {
	value.max(min).min(max)
}

fn make_int_resource(id: u16) -> PCWSTR {
	id as usize as *const u16
}

pub fn Rnd(rnd_max: c_int) -> c_int {
	// Return a pseudo-random number in the [0, rnd_max) range like the C helper did.
	if rnd_max <= 0 {
		0
	} else {
		next_rand() % rnd_max
	}
}


pub unsafe fn ReportErr(id_err: u16) {
	// Format either a catalog string or the "unknown error" template before showing the dialog.
	let mut sz_msg = [0u16; CCH_MSG_MAX];
	let mut sz_title = [0u16; CCH_MSG_MAX];

	if (id_err as u32) < ID_ERR_MAX {
		LoadStringW(hInst, id_err.into(), sz_msg.as_mut_ptr(), CCH_MSG_MAX as i32);
	} else {
		LoadStringW(hInst, ID_ERR_UNKNOWN, sz_title.as_mut_ptr(), CCH_MSG_MAX as i32);
		wsprintfW(sz_msg.as_mut_ptr(), sz_title.as_ptr(), id_err as c_int);
	}

	LoadStringW(hInst, ID_ERR_TITLE, sz_title.as_mut_ptr(), CCH_MSG_MAX as i32);
	MessageBoxW(std::ptr::null_mut(), sz_msg.as_ptr(), sz_title.as_ptr(), 0x0000_0010);
}


pub unsafe fn LoadSz(id: u16, sz: *mut u16, cch: u32) {
	// Wrapper around LoadString that raises the original fatal error if the resource is missing.
	if LoadStringW(hInst, id.into(), sz, cch as i32) == 0 {
		ReportErr(1001);
	}
}


pub unsafe fn ReadIniInt(isz_pref: c_int, val_default: c_int, val_min: c_int, val_max: c_int) -> c_int {
	// Pull an integer from the legacy .ini file, honoring the same clamp the game always used.
	if isz_pref < 0 || (isz_pref as usize) >= ISZ_PREF_MAX {
		return val_default;
	}

	let key = rgszPref[isz_pref as usize];
	if key.is_null() {
		return val_default;
	}

	let value = GetPrivateProfileIntW(class_ptr(), key, val_default, SZ_INI_FILE) as c_int;
	clamp(value, val_min, val_max)
}


pub unsafe fn ReadIniSz(isz_pref: c_int, sz_ret: *mut u16) {
	// Grab the string from entpack.ini or fall back to the default Hall of Fame name.
	if sz_ret.is_null() || isz_pref < 0 || (isz_pref as usize) >= ISZ_PREF_MAX {
		return;
	}

	let key = rgszPref[isz_pref as usize];
	if key.is_null() {
		return;
	}

	GetPrivateProfileStringW(
		class_ptr(),
		key,
		default_name_ptr(),
		sz_ret,
		CCH_NAME_MAX as u32,
		SZ_INI_FILE,
	);
}


pub unsafe fn InitConst() {
	// Initialize UI globals, migrate preferences from the .ini file exactly once, and seed randomness.
	seed_rng((GetTickCount() & 0xFFFF) as u32);

	LoadSz(ID_GAMENAME as u16, addr_of_mut!(szClass[0]), CCH_NAME_MAX as u32);
	LoadSz(ID_MSG_SEC as u16, addr_of_mut!(szTime[0]), CCH_NAME_MAX as u32);
	LoadSz(ID_NAME_DEFAULT as u16, addr_of_mut!(szDefaultName[0]), CCH_NAME_MAX as u32);

	dypCaption = GetSystemMetrics(SM_CYCAPTION) + 1;
	dypMenu = GetSystemMetrics(SM_CYMENU) + 1;
	dypBorder = GetSystemMetrics(SM_CYBORDER) + 1;
	dxpBorder = GetSystemMetrics(SM_CXBORDER) + 1;

	let mut disposition = 0u32;
	let mut key: HKEY = std::ptr::null_mut();
	let mut already_played = 0;

	if RegCreateKeyExW(
		HKEY_CURRENT_USER,
		pref::SZ_WINMINE_REG,
		0,
		null_mut(),
		0,
		KEY_READ,
		null_mut(),
		&mut key,
		&mut disposition,
	) == 0
	{
		g_hReg = key;
		already_played = ReadInt(ISZ_PREF_ALREADY_PLAYED as c_int, 0, 0, 1);
		RegCloseKey(key);
		g_hReg = std::ptr::null_mut();
	}

	if already_played != 0 {
		return;
	}

	let prefs = prefs_mut();

	(*prefs).Height = ReadIniInt(ISZ_PREF_HEIGHT as c_int, MINHEIGHT, DEFHEIGHT, 25);
	(*prefs).Width = ReadIniInt(ISZ_PREF_WIDTH as c_int, MINWIDTH, DEFWIDTH, 30);
	(*prefs).wGameType = ReadIniInt(ISZ_PREF_GAME as c_int, WGAME_BEGIN, WGAME_BEGIN, WGAME_EXPERT + 1) as u16;
	(*prefs).Mines = ReadIniInt(ISZ_PREF_MINES as c_int, 10, 10, 999);
	(*prefs).xWindow = ReadIniInt(ISZ_PREF_XWINDOW as c_int, 80, 0, 1024);
	(*prefs).yWindow = ReadIniInt(ISZ_PREF_YWINDOW as c_int, 80, 0, 1024);

	(*prefs).fSound = ReadIniInt(ISZ_PREF_SOUND as c_int, 0, 0, FSOUND_ON);
	(*prefs).fMark = bool_from_int(ReadIniInt(ISZ_PREF_MARK as c_int, TRUE as c_int, 0, 1));
	(*prefs).fTick = bool_from_int(ReadIniInt(ISZ_PREF_TICK as c_int, 0, 0, 1));
	(*prefs).fMenu = ReadIniInt(ISZ_PREF_MENU as c_int, FMENU_ALWAYS_ON, FMENU_ALWAYS_ON, FMENU_ON);

	(*prefs).rgTime[WGAME_BEGIN as usize] = ReadIniInt(ISZ_PREF_BEGIN_TIME as c_int, 999, 0, 999);
	(*prefs).rgTime[WGAME_INTER as usize] = ReadIniInt(ISZ_PREF_INTER_TIME as c_int, 999, 0, 999);
	(*prefs).rgTime[WGAME_EXPERT as usize] = ReadIniInt(ISZ_PREF_EXPERT_TIME as c_int, 999, 0, 999);

	ReadIniSz(ISZ_PREF_BEGIN_NAME as c_int, (*prefs).szBegin.as_mut_ptr());
	ReadIniSz(ISZ_PREF_INTER_NAME as c_int, (*prefs).szInter.as_mut_ptr());
	ReadIniSz(ISZ_PREF_EXPERT_NAME as c_int, (*prefs).szExpert.as_mut_ptr());

	let desktop = GetDesktopWindow();
	let hdc: HDC = GetDC(desktop);
	let default_color = if !hdc.is_null() && GetDeviceCaps(hdc, NUMCOLORS as i32) != 2 {
		TRUE
	} else {
		FALSE
	};
	if !hdc.is_null() {
		ReleaseDC(desktop, hdc);
	}
	(*prefs).fColor = bool_from_int(ReadIniInt(ISZ_PREF_COLOR as c_int, default_color as c_int, 0, 1));

	if (*prefs).fSound == FSOUND_ON {
		(*prefs).fSound = FInitTunes();
	}

	WritePreferences();
}


pub unsafe fn CheckEm(idm: u16, f_check: BOOL) {
	// Maintain the old menu checkmark toggles (e.g. question marks, sound).
	CheckMenuItem(hMenu, idm.into(), if f_check != 0 { MF_CHECKED } else { MF_UNCHECKED });
}


pub unsafe fn SetMenuBar(f_active: c_int) {
	// Persist the menu visibility preference, refresh accelerator state, and resize the window.
	(*prefs_mut()).fMenu = f_active;
	FixMenus();

	let menu_on = ((*prefs_ref()).fMenu & FMENU_FLAG_OFF) == 0;
	SetMenu(hwndMain, if menu_on { hMenu } else { 0 as HMENU });
	AdjustWindow(F_RESIZE);
}


pub unsafe fn DoAbout() {
	// Show the stock About box with the localized title and credit strings.
	let mut sz_version = [0u16; CCH_MSG_MAX];
	let mut sz_credit = [0u16; CCH_MSG_MAX];

	LoadSz(ID_MSG_VERSION as u16, sz_version.as_mut_ptr(), CCH_MSG_MAX as u32);
	LoadSz(ID_MSG_CREDIT as u16, sz_credit.as_mut_ptr(), CCH_MSG_MAX as u32);

	ShellAboutW(
		hwndMain,
		sz_version.as_ptr(),
		sz_credit.as_ptr(),
		LoadIconW(hInst, make_int_resource(ID_ICON_MAIN)),
	);
}


pub unsafe fn DoHelp(w_command: u16, l_param: u32) {
	// htmlhelp.dll expects either the localized .chm next to the EXE or the fallback NTHelp file.
	let mut buffer = [0u8; CCH_MAX_PATHNAME];

	if (w_command as u32) != (HELP_HELPONHELP as u32) {
		let len = GetModuleFileNameA(hInst, buffer.as_mut_ptr(), CCH_MAX_PATHNAME as u32) as usize;
		let mut dot = None;
		for i in (0..len).rev() {
			if buffer[i] == b'.' {
				dot = Some(i);
				break;
			}
			if buffer[i] == b'\\' {
				break;
			}
		}
		let pos = dot.unwrap_or(len);
		const EXT: &[u8] = b".chm\0";
		let mut i = 0;
		while i < EXT.len() {
			buffer[pos + i] = EXT[i];
			i += 1;
		}
	} else {
		const HELP_FILE: &[u8] = b"NTHelp.chm\0";
		buffer[..HELP_FILE.len()].copy_from_slice(HELP_FILE);
	}

	HtmlHelpA(GetDesktopWindow(), buffer.as_ptr() as PCSTR, l_param, 0);
}


pub unsafe fn GetDlgInt(h_dlg: HWND, dlg_id: c_int, num_lo: c_int, num_hi: c_int) -> c_int {
	// Mirror GetDlgInt from util.c: clamp user input to the legal range before the caller consumes it.
	let mut success = 0i32;
	let value = GetDlgItemInt(h_dlg, dlg_id, &mut success, FALSE);
	let value = value as c_int;
	clamp(value, num_lo, num_hi)
}

fn bool_from_int(value: c_int) -> BOOL {
	if value != 0 { TRUE } else { FALSE }
}
