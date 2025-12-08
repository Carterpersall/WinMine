use core::ptr::{addr_of, addr_of_mut};

use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::System::WindowsProgramming::GetPrivateProfileIntW;
use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItemInt;

use winsafe::{self as w, IdPos, WString, co, co::HELPW, co::SM, prelude::*};

use crate::globals::{
    dxpBorder, dypBorder, dypCaption, dypMenu, hInst, hMenu, hwndMain, szClass, szDefaultName,
    szTime,
};
use crate::pref::{
    CCH_NAME_MAX, DEFHEIGHT, DEFWIDTH, FMENU_ALWAYS_ON, FMENU_ON, FSOUND_ON, ISZ_PREF_MAX,
    MINHEIGHT, MINWIDTH, WGAME_BEGIN, WGAME_EXPERT, WGAME_INTER,
};
use crate::pref::{
    ReadInt, SZ_WINMINE_REG_STR, WritePreferences, close_registry_handle, g_hReg, pref_key_literal,
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

const SZ_INI_FILE: &str = "entpack.ini";

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
fn class_ptr() -> *const u16 {
    unsafe { addr_of!(szClass[0]) }
}

fn clamp(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

pub fn Rnd(rnd_max: i32) -> i32 {
    // Return a pseudo-random number in the [0, rnd_max) range like the C helper did.
    if rnd_max <= 0 {
        0
    } else {
        next_rand() % rnd_max
    }
}

pub fn ReportErr(id_err: u16) {
    // Format either a catalog string or the "unknown error" template before showing the dialog.
    let msg = if (id_err as u32) < ID_ERR_MAX {
        unsafe { hInst.LoadString(id_err).unwrap_or_default() }
    } else {
        let template = unsafe { hInst.LoadString(ID_ERR_UNKNOWN as u16).unwrap_or_default() };
        template.replace("%d", &id_err.to_string())
    };

    let title = unsafe { hInst.LoadString(ID_ERR_TITLE as u16).unwrap_or_default() };
    let _ = w::HWND::NULL.MessageBox(&msg, &title, co::MB::ICONHAND);
}

pub fn LoadSz(id: u16, sz: *mut u16, cch: u32) {
    // Wrapper around LoadString that raises the original fatal error if the resource is missing.
    let text = unsafe { hInst.LoadString(id).unwrap_or_default() };
    if text.is_empty() {
        ReportErr(1001);
        return;
    }

    if sz.is_null() || cch == 0 {
        return;
    }

    let max = cch as usize;
    let slice = unsafe { core::slice::from_raw_parts_mut(sz, max) };
    for (i, code_unit) in text.encode_utf16().chain(Some(0)).take(max).enumerate() {
        slice[i] = code_unit;
    }
}

pub fn ReadIniInt(isz_pref: i32, val_default: i32, val_min: i32, val_max: i32) -> i32 {
    // Pull an integer from the legacy .ini file, honoring the same clamp the game always used.
    if isz_pref < 0 || (isz_pref as usize) >= ISZ_PREF_MAX {
        return val_default;
    }

    let key = match pref_key_literal(isz_pref) {
        Some(name) => WString::from_str(name),
        None => return val_default,
    };

    let ini_path = WString::from_str(SZ_INI_FILE);
    let value = unsafe {
        GetPrivateProfileIntW(class_ptr(), key.as_ptr(), val_default, ini_path.as_ptr()) as i32
    };
    clamp(value, val_min, val_max)
}

pub fn ReadIniSz(isz_pref: i32, sz_ret: *mut u16) {
    // Grab the string from entpack.ini or fall back to the default Hall of Fame name.
    if sz_ret.is_null() || isz_pref < 0 || (isz_pref as usize) >= ISZ_PREF_MAX {
        return;
    }

    let key = match pref_key_literal(isz_pref) {
        Some(name) => WString::from_str(name),
        None => return,
    };

    let section = utf16_buffer_to_string(unsafe { &szClass });
    let key_text = key.to_string();
    let default_name = utf16_buffer_to_string(unsafe { &szDefaultName });

    let value = match w::GetPrivateProfileString(&section, &key_text, SZ_INI_FILE) {
        Ok(Some(text)) => text,
        _ => default_name,
    };

    let slice = unsafe { core::slice::from_raw_parts_mut(sz_ret, CCH_NAME_MAX) };
    for (i, code_unit) in value
        .encode_utf16()
        .chain(Some(0))
        .take(CCH_NAME_MAX)
        .enumerate()
    {
        slice[i] = code_unit;
    }
}

pub fn InitConst() {
    // Initialize UI globals, migrate preferences from the .ini file exactly once, and seed randomness.
    let ticks = (w::GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    unsafe {
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
    }

    dypCaption.store(w::GetSystemMetrics(SM::CYCAPTION) + 1, Ordering::Relaxed);
    dypMenu.store(w::GetSystemMetrics(SM::CYMENU) + 1, Ordering::Relaxed);
    dypBorder.store(w::GetSystemMetrics(SM::CYBORDER) + 1, Ordering::Relaxed);
    dxpBorder.store(w::GetSystemMetrics(SM::CXBORDER) + 1, Ordering::Relaxed);

    let mut already_played = false;

    if let Ok((mut key_guard, _)) = w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::READ,
        None,
    ) {
        unsafe {
            g_hReg = key_guard.leak();
            already_played = ReadInt(ISZ_PREF_ALREADY_PLAYED as i32, 0, 0, 1) != 0;
            close_registry_handle();
        }
    }

    if already_played {
        return;
    }

    unsafe {
        let prefs = &mut Preferences;

        prefs.Height = ReadIniInt(ISZ_PREF_HEIGHT as i32, MINHEIGHT, DEFHEIGHT, 25);
        prefs.Width = ReadIniInt(ISZ_PREF_WIDTH as i32, MINWIDTH, DEFWIDTH, 30);
        prefs.wGameType = ReadIniInt(
            ISZ_PREF_GAME as i32,
            WGAME_BEGIN,
            WGAME_BEGIN,
            WGAME_EXPERT + 1,
        ) as u16;
        prefs.Mines = ReadIniInt(ISZ_PREF_MINES as i32, 10, 10, 999);
        prefs.xWindow = ReadIniInt(ISZ_PREF_XWINDOW as i32, 80, 0, 1024);
        prefs.yWindow = ReadIniInt(ISZ_PREF_YWINDOW as i32, 80, 0, 1024);

        prefs.fSound = ReadIniInt(ISZ_PREF_SOUND as i32, 0, 0, FSOUND_ON);
        prefs.fMark = bool_from_int(ReadIniInt(ISZ_PREF_MARK as i32, 1, 0, 1));
        prefs.fTick = bool_from_int(ReadIniInt(ISZ_PREF_TICK as i32, 0, 0, 1));
        prefs.fMenu = ReadIniInt(
            ISZ_PREF_MENU as i32,
            FMENU_ALWAYS_ON,
            FMENU_ALWAYS_ON,
            FMENU_ON,
        );

        prefs.rgTime[WGAME_BEGIN as usize] = ReadIniInt(ISZ_PREF_BEGIN_TIME as i32, 999, 0, 999);
        prefs.rgTime[WGAME_INTER as usize] = ReadIniInt(ISZ_PREF_INTER_TIME as i32, 999, 0, 999);
        prefs.rgTime[WGAME_EXPERT as usize] = ReadIniInt(ISZ_PREF_EXPERT_TIME as i32, 999, 0, 999);

        ReadIniSz(ISZ_PREF_BEGIN_NAME as i32, prefs.szBegin.as_mut_ptr());
        ReadIniSz(ISZ_PREF_INTER_NAME as i32, prefs.szInter.as_mut_ptr());
        ReadIniSz(ISZ_PREF_EXPERT_NAME as i32, prefs.szExpert.as_mut_ptr());
    }

    let desktop = w::HWND::GetDesktopWindow();
    let default_color = match desktop.GetDC() {
        Ok(hdc) => {
            if hdc.GetDeviceCaps(co::GDC::NUMCOLORS) != 2 {
                1
            } else {
                0
            }
        }
        Err(_) => 0,
    };
    unsafe {
        let prefs = &mut Preferences;
        prefs.fColor = bool_from_int(ReadIniInt(ISZ_PREF_COLOR as i32, default_color, 0, 1));

        if prefs.fSound == FSOUND_ON {
            prefs.fSound = FInitTunes();
        }
    }

    unsafe {
        WritePreferences();
    }
}

pub fn CheckEm(idm: u16, f_check: bool) {
    // Maintain the old menu checkmark toggles (e.g. question marks, sound).
    if let Some(menu) = unsafe { hMenu.as_opt() } {
        let _ = menu.CheckMenuItem(IdPos::Id(idm), f_check);
    }
}

pub fn SetMenuBar(f_active: i32) {
    // Persist the menu visibility preference, refresh accelerator state, and resize the window.
    let (menu_handle, menu_on) = unsafe {
        Preferences.fMenu = f_active;
        FixMenus();
        let menu_on = (Preferences.fMenu & FMENU_FLAG_OFF) == 0;
        let handle = if menu_on {
            hMenu.as_opt().unwrap_or(&w::HMENU::NULL)
        } else {
            &w::HMENU::NULL
        };
        (handle, menu_on)
    };

    if let Some(hwnd) = unsafe { hwndMain.as_opt() } {
        let menu_arg = if menu_on {
            menu_handle
        } else {
            &w::HMENU::NULL
        };
        let _ = hwnd.SetMenu(menu_arg);
        AdjustWindow(F_RESIZE);
    }
}

pub fn DoAbout() {
    // Show the stock About box with the localized title and credit strings.
    let hwnd = match unsafe { hwndMain.as_opt() } {
        Some(hwnd) => hwnd,
        None => return,
    };

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
    let icon_guard = unsafe { hInst.LoadIcon(w::IdIdiStr::Id(ID_ICON_MAIN)) }.ok();
    let icon = icon_guard.as_deref();

    let _ = hwnd.ShellAbout(&title, None, Some(&credit), icon);
}

pub fn DoHelp(w_command: u16, l_param: u32) {
    // htmlhelp.dll expects either the localized .chm next to the EXE or the fallback NTHelp file.
    let mut buffer = [0u8; CCH_MAX_PATHNAME];

    if (w_command as u32) != HELPW::HELPONHELP.raw() {
        let exe_path = unsafe { hInst.GetModuleFileName() }.unwrap_or_default();
        let mut bytes = exe_path.into_bytes();
        if bytes.len() + 1 > CCH_MAX_PATHNAME {
            bytes.truncate(CCH_MAX_PATHNAME - 1);
        }
        bytes.push(0);
        let len = bytes.len() - 1;
        buffer[..bytes.len()].copy_from_slice(&bytes);

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
        while i < EXT.len() && pos + i < buffer.len() {
            buffer[pos + i] = EXT[i];
            i += 1;
        }
    } else {
        const HELP_FILE: &[u8] = b"NTHelp.chm\0";
        buffer[..HELP_FILE.len()].copy_from_slice(HELP_FILE);
    }

    let desktop = w::HWND::GetDesktopWindow();
    unsafe {
        HtmlHelpA(desktop.ptr() as _, buffer.as_ptr(), l_param, 0);
    }
}

fn utf16_buffer_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub fn GetDlgInt(h_dlg: &w::HWND, dlg_id: i32, num_lo: i32, num_hi: i32) -> i32 {
    // Mirror GetDlgInt from util.c: clamp user input to the legal range before the caller consumes it.
    let mut success = 0i32;
    let value = unsafe { GetDlgItemInt(h_dlg.ptr(), dlg_id, &mut success, 0) };
    let value = value as i32;
    clamp(value, num_lo, num_hi)
}

fn bool_from_int(value: i32) -> bool {
    value != 0
}
