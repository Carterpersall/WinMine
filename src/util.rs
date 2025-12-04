use core::ptr::{addr_of, addr_of_mut};

use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::core::{w, PCSTR, PCWSTR};
use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::Foundation::{FALSE, HWND, TRUE};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameA;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::WindowsProgramming::{
    GetPrivateProfileIntW, GetPrivateProfileStringW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    wsprintfW, GetDlgItemInt, GetSystemMetrics, LoadIconW, LoadStringW, MessageBoxW,
    HELP_HELPONHELP, SM_CXBORDER, SM_CYBORDER, SM_CYCAPTION, SM_CYMENU,
};

use winsafe::{self as w, co, prelude::*, IdPos, HICON};

use crate::globals::{
    dxpBorder, dypBorder, dypCaption, dypMenu, hInst, hMenu, hwndMain, szClass, szDefaultName,
    szTime,
};
use crate::pref::{
    Pref, CCH_NAME_MAX, DEFHEIGHT, DEFWIDTH, FMENU_ALWAYS_ON, FMENU_ON, FSOUND_ON,
    ISZ_PREF_MAX, MINHEIGHT, MINWIDTH, WGAME_BEGIN, WGAME_EXPERT, WGAME_INTER,
};
use crate::pref::{
    close_registry_handle, g_hReg, rgszPref, ReadInt, WritePreferences, SZ_WINMINE_REG_STR,
};
use crate::rtns::Preferences;
use crate::sound::FInitTunes;
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
const F_RESIZE: i32 = 0x02;
const FMENU_FLAG_OFF: i32 = 0x01;

const SZ_INI_FILE: PCWSTR = w!("entpack.ini");

fn seed_rng(seed: u32) {
    let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
    RNG_STATE.store(value, Ordering::Relaxed);
}

fn next_rand() -> i32 {
    let mut current = RNG_STATE.load(Ordering::Relaxed);
    loop {
        let next = current
            .wrapping_mul(RNG_MULTIPLIER)
            .wrapping_add(RNG_INCREMENT);
        match RNG_STATE.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return ((next >> 16) & 0x7FFF) as i32,
            Err(actual) => current = actual,
        }
    }
}

#[inline]
unsafe fn prefs_mut() -> *mut Pref {
    addr_of_mut!(Preferences)
}

#[inline]
unsafe fn prefs_ref() -> *const Pref {
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

fn clamp(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn make_int_resource(id: u16) -> PCWSTR {
    id as usize as *const u16
}

pub fn Rnd(rnd_max: i32) -> i32 {
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
        LoadStringW(
            hInst,
            id_err.into(),
            sz_msg.as_mut_ptr(),
            CCH_MSG_MAX as i32,
        );
    } else {
        LoadStringW(
            hInst,
            ID_ERR_UNKNOWN,
            sz_title.as_mut_ptr(),
            CCH_MSG_MAX as i32,
        );
        wsprintfW(sz_msg.as_mut_ptr(), sz_title.as_ptr(), id_err as i32);
    }

    LoadStringW(
        hInst,
        ID_ERR_TITLE,
        sz_title.as_mut_ptr(),
        CCH_MSG_MAX as i32,
    );
    MessageBoxW(
        std::ptr::null_mut(),
        sz_msg.as_ptr(),
        sz_title.as_ptr(),
        0x0000_0010,
    );
}

pub unsafe fn LoadSz(id: u16, sz: *mut u16, cch: u32) {
    // Wrapper around LoadString that raises the original fatal error if the resource is missing.
    if LoadStringW(hInst, id.into(), sz, cch as i32) == 0 {
        ReportErr(1001);
    }
}

pub unsafe fn ReadIniInt(
    isz_pref: i32,
    val_default: i32,
    val_min: i32,
    val_max: i32,
) -> i32 {
    // Pull an integer from the legacy .ini file, honoring the same clamp the game always used.
    if isz_pref < 0 || (isz_pref as usize) >= ISZ_PREF_MAX {
        return val_default;
    }

    let key = rgszPref[isz_pref as usize];
    if key.is_null() {
        return val_default;
    }

    let value = GetPrivateProfileIntW(class_ptr(), key, val_default, SZ_INI_FILE) as i32;
    clamp(value, val_min, val_max)
}

pub unsafe fn ReadIniSz(isz_pref: i32, sz_ret: *mut u16) {
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

    LoadSz(
        ID_GAMENAME as u16,
        addr_of_mut!(szClass[0]),
        CCH_NAME_MAX as u32,
    );
    LoadSz(
        ID_MSG_SEC as u16,
        addr_of_mut!(szTime[0]),
        CCH_NAME_MAX as u32,
    );
    LoadSz(
        ID_NAME_DEFAULT as u16,
        addr_of_mut!(szDefaultName[0]),
        CCH_NAME_MAX as u32,
    );

    dypCaption.store(GetSystemMetrics(SM_CYCAPTION) + 1, Ordering::Relaxed);
    dypMenu.store(GetSystemMetrics(SM_CYMENU) + 1, Ordering::Relaxed);
    dypBorder.store(GetSystemMetrics(SM_CYBORDER) + 1, Ordering::Relaxed);
    dxpBorder.store(GetSystemMetrics(SM_CXBORDER) + 1, Ordering::Relaxed);

    let mut already_played = 0;

    if let Ok((mut key_guard, _)) = w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::READ,
        None,
    ) {
        g_hReg = key_guard.leak();
        already_played = ReadInt(ISZ_PREF_ALREADY_PLAYED as i32, 0, 0, 1);
        close_registry_handle();
    }

    if already_played != 0 {
        return;
    }

    let prefs = prefs_mut();

    (*prefs).Height = ReadIniInt(ISZ_PREF_HEIGHT as i32, MINHEIGHT, DEFHEIGHT, 25);
    (*prefs).Width = ReadIniInt(ISZ_PREF_WIDTH as i32, MINWIDTH, DEFWIDTH, 30);
    (*prefs).wGameType = ReadIniInt(
        ISZ_PREF_GAME as i32,
        WGAME_BEGIN,
        WGAME_BEGIN,
        WGAME_EXPERT + 1,
    ) as u16;
    (*prefs).Mines = ReadIniInt(ISZ_PREF_MINES as i32, 10, 10, 999);
    (*prefs).xWindow = ReadIniInt(ISZ_PREF_XWINDOW as i32, 80, 0, 1024);
    (*prefs).yWindow = ReadIniInt(ISZ_PREF_YWINDOW as i32, 80, 0, 1024);

    (*prefs).fSound = ReadIniInt(ISZ_PREF_SOUND as i32, 0, 0, FSOUND_ON);
    (*prefs).fMark = bool_from_int(ReadIniInt(ISZ_PREF_MARK as i32, TRUE, 0, 1));
    (*prefs).fTick = bool_from_int(ReadIniInt(ISZ_PREF_TICK as i32, 0, 0, 1));
    (*prefs).fMenu = ReadIniInt(
        ISZ_PREF_MENU as i32,
        FMENU_ALWAYS_ON,
        FMENU_ALWAYS_ON,
        FMENU_ON,
    );

    (*prefs).rgTime[WGAME_BEGIN as usize] = ReadIniInt(ISZ_PREF_BEGIN_TIME as i32, 999, 0, 999);
    (*prefs).rgTime[WGAME_INTER as usize] = ReadIniInt(ISZ_PREF_INTER_TIME as i32, 999, 0, 999);
    (*prefs).rgTime[WGAME_EXPERT as usize] = ReadIniInt(ISZ_PREF_EXPERT_TIME as i32, 999, 0, 999);

    ReadIniSz(ISZ_PREF_BEGIN_NAME as i32, (*prefs).szBegin.as_mut_ptr());
    ReadIniSz(ISZ_PREF_INTER_NAME as i32, (*prefs).szInter.as_mut_ptr());
    ReadIniSz(
        ISZ_PREF_EXPERT_NAME as i32,
        (*prefs).szExpert.as_mut_ptr(),
    );

    let desktop = w::HWND::GetDesktopWindow();
    let default_color = match desktop.GetDC() {
        Ok(hdc) => {
            if hdc.GetDeviceCaps(co::GDC::NUMCOLORS) != 2 {
                TRUE
            } else {
                FALSE
            }
        }
        Err(_) => FALSE,
    };
    (*prefs).fColor = bool_from_int(ReadIniInt(
        ISZ_PREF_COLOR as i32,
        default_color,
        0,
        1,
    ));

    if (*prefs).fSound == FSOUND_ON {
        (*prefs).fSound = FInitTunes();
    }

    WritePreferences();
}

pub unsafe fn CheckEm(idm: u16, f_check: bool) {
    // Maintain the old menu checkmark toggles (e.g. question marks, sound).
    if hMenu.is_null() {
        return;
    }

    let menu = unsafe { w::HMENU::from_ptr(hMenu as _) };
    let _ = menu.CheckMenuItem(IdPos::Id(idm), f_check);
}

pub unsafe fn SetMenuBar(f_active: i32) {
    // Persist the menu visibility preference, refresh accelerator state, and resize the window.
    (*prefs_mut()).fMenu = f_active;
    FixMenus();

    let menu_on = ((*prefs_ref()).fMenu & FMENU_FLAG_OFF) == 0;
    if hwndMain.is_null() {
        return;
    }

    let hwnd = unsafe { w::HWND::from_ptr(hwndMain as _) };
    let menu_handle = if menu_on && !hMenu.is_null() {
        unsafe { w::HMENU::from_ptr(hMenu as _) }
    } else {
        w::HMENU::NULL
    };
    let _ = hwnd.SetMenu(&menu_handle);
    AdjustWindow(F_RESIZE);
}

pub unsafe fn DoAbout() {
    // Show the stock About box with the localized title and credit strings.
    if hwndMain.is_null() {
        return;
    }

    let mut sz_version = [0u16; CCH_MSG_MAX];
    let mut sz_credit = [0u16; CCH_MSG_MAX];

    LoadSz(
        ID_MSG_VERSION as u16,
        sz_version.as_mut_ptr(),
        CCH_MSG_MAX as u32,
    );
    LoadSz(
        ID_MSG_CREDIT as u16,
        sz_credit.as_mut_ptr(),
        CCH_MSG_MAX as u32,
    );

    let title = utf16_buffer_to_string(&sz_version);
    let credit = utf16_buffer_to_string(&sz_credit);
    let icon_raw = LoadIconW(hInst, make_int_resource(ID_ICON_MAIN));
    let icon = if icon_raw.is_null() {
        None
    } else {
        Some(unsafe { HICON::from_ptr(icon_raw as _) })
    };

    let hwnd = unsafe { w::HWND::from_ptr(hwndMain as _) };
    let _ = hwnd.ShellAbout(&title, None, Some(&credit), icon.as_ref());
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

    let desktop = w::HWND::GetDesktopWindow();
    HtmlHelpA(desktop.ptr() as _, buffer.as_ptr() as PCSTR, l_param, 0);
}

fn utf16_buffer_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub unsafe fn GetDlgInt(h_dlg: HWND, dlg_id: i32, num_lo: i32, num_hi: i32) -> i32 {
    // Mirror GetDlgInt from util.c: clamp user input to the legal range before the caller consumes it.
    let mut success = 0i32;
    let value = GetDlgItemInt(h_dlg, dlg_id, &mut success, FALSE);
    let value = value as i32;
    clamp(value, num_lo, num_hi)
}

fn bool_from_int(value: i32) -> bool {
    value != 0
}
