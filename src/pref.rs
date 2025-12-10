// Registry-backed preference helpers mirrored from pref.c.
use core::sync::atomic::{AtomicBool, Ordering};

use winsafe::{self as w, RegistryValue, co, guard::RegCloseKeyGuard};

use crate::globals::global_state;
use crate::rtns::{preferences_mutex, xBoxMac, yBoxMac};
use crate::sound::FInitTunes;

/// Maximum length (UTF-16 code units) of player names stored in the registry.
pub const CCH_NAME_MAX: usize = 32;
/// Total count of preference keys mirrored from the WinMine registry hive.
pub const ISZ_PREF_MAX: usize = 18;

/// Flag value indicating sound is enabled.
pub const FSOUND_ON: i32 = 3;
/// Flag value indicating sound is disabled.
pub const FSOUND_OFF: i32 = 2;

/// Minimum board height allowed by the game.
pub const MINHEIGHT: i32 = 9;
/// Default board height used on first run.
pub const DEFHEIGHT: i32 = 9;
/// Minimum board width allowed by the game.
pub const MINWIDTH: i32 = 9;
/// Default board width used on first run.
pub const DEFWIDTH: i32 = 9;

/// Menu visibility flag meaning "always show the menu bar".
pub const FMENU_ALWAYS_ON: i32 = 0;
/// Menu visibility flag meaning "hideable menu bar".
pub const FMENU_ON: i32 = 2;

/// Registry key path used to persist preferences.
pub const SZ_WINMINE_REG_STR: &str = "Software\\Microsoft\\winmine";

/// Difficulty presets exposed throughout the game.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum GameType {
    Begin = 0,
    Inter = 1,
    Expert = 2,
    Other = 3,
}

// Registry value names, ordered to match the legacy iszPref constants.
const PREF_STRINGS: [&str; ISZ_PREF_MAX] = [
    "Difficulty",
    "Mines",
    "Height",
    "Width",
    "Xpos",
    "Ypos",
    "Sound",
    "Mark",
    "Menu",
    "Tick",
    "Color",
    "Time1",
    "Name1",
    "Time2",
    "Name2",
    "Time3",
    "Name3",
    "AlreadyPlayed",
];

pub struct Pref {
    pub wGameType: GameType,
    pub Mines: i32,
    pub Height: i32,
    pub Width: i32,
    pub xWindow: i32,
    pub yWindow: i32,
    pub fSound: i32,
    pub fMark: bool,
    pub fTick: bool,
    pub fMenu: i32,
    pub fColor: bool,
    pub rgTime: [i32; 3],
    pub szBegin: [u16; CCH_NAME_MAX],
    pub szInter: [u16; CCH_NAME_MAX],
    pub szExpert: [u16; CCH_NAME_MAX],
}

// Flag consulted by the C UI layer to decide when to persist settings.
pub static fUpdateIni: AtomicBool = AtomicBool::new(false);

pub unsafe fn ReadInt(
    handle: &w::HKEY,
    isz_pref: i32,
    val_default: i32,
    val_min: i32,
    val_max: i32,
) -> i32 {
    // Registry integer fetch with clamping equivalent to the legacy ReadInt helper.
    if handle.ptr().is_null() {
        return val_default;
    }

    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return val_default,
    };

    let value = match handle.RegQueryValueEx(Some(&key_name)) {
        Ok(RegistryValue::Dword(val)) => val as i32,
        _ => return val_default,
    };

    clamp_i32(value, val_min, val_max)
}

pub unsafe fn ReadSz(handle: &w::HKEY, isz_pref: i32, sz_ret: *mut u16) {
    // Pull a high-score name (or similar) from the hive, falling back to the default string.
    if sz_ret.is_null() {
        return;
    }

    if handle.ptr().is_null() {
        unsafe {
            copy_default_name(sz_ret);
        }
        return;
    }

    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => {
            unsafe {
                copy_default_name(sz_ret);
            }
            return;
        }
    };

    match handle.RegQueryValueEx(Some(&key_name)) {
        Ok(RegistryValue::Sz(value)) | Ok(RegistryValue::ExpandSz(value)) => unsafe {
            copy_str_to_wide(&value, sz_ret, CCH_NAME_MAX);
        },
        _ => unsafe { copy_default_name(sz_ret) },
    }
}

pub unsafe fn ReadPreferences() {
    // Fetch persisted dimensions, timers, and feature flags from the WinMine registry hive.
    let (mut key_guard, _) = match w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::READ,
        None,
    ) {
        Ok(result) => result,
        Err(_) => return,
    };

    let handle = key_guard.leak();

    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    unsafe {
        let height = ReadInt(&handle, 2, MINHEIGHT, DEFHEIGHT, 25);
        yBoxMac.store(height, Ordering::Relaxed);
        prefs.Height = height;

        let width = ReadInt(&handle, 3, MINWIDTH, DEFWIDTH, 30);
        xBoxMac.store(width, Ordering::Relaxed);
        prefs.Width = width;

        let game_raw = ReadInt(&handle, 0, GameType::Begin as i32, GameType::Begin as i32, GameType::Expert as i32 + 1);
        prefs.wGameType = match game_raw {
            0 => GameType::Begin,
            1 => GameType::Inter,
            2 => GameType::Expert,
            _ => GameType::Other,
        };
        prefs.Mines = ReadInt(&handle, 1, 10, 10, 999);
        prefs.xWindow = ReadInt(&handle, 4, 80, 0, 1024);
        prefs.yWindow = ReadInt(&handle, 5, 80, 0, 1024);

        prefs.fSound = ReadInt(&handle, 6, 0, 0, FSOUND_ON);
        prefs.fMark = ReadInt(&handle, 7, 1, 0, 1) != 0;
        prefs.fTick = ReadInt(&handle, 9, 0, 0, 1) != 0;
        prefs.fMenu = ReadInt(&handle, 8, FMENU_ALWAYS_ON, FMENU_ALWAYS_ON, FMENU_ON);

        prefs.rgTime[GameType::Begin as usize] = ReadInt(&handle, 11, 999, 0, 999);
        prefs.rgTime[GameType::Inter as usize] = ReadInt(&handle, 13, 999, 0, 999);
        prefs.rgTime[GameType::Expert as usize] = ReadInt(&handle, 15, 999, 0, 999);

        ReadSz(&handle, 12, prefs.szBegin.as_mut_ptr());
        ReadSz(&handle, 14, prefs.szInter.as_mut_ptr());
        ReadSz(&handle, 16, prefs.szExpert.as_mut_ptr());
    }

    // Determine whether to favor color assets (NUMCOLORS may return -1 on true color displays).
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
    prefs.fColor = unsafe { ReadInt(&handle, 10, default_color, 0, 1) } != 0;

    // If sound is enabled, verify that the system can actually play the resources.
    if prefs.fSound == FSOUND_ON {
        prefs.fSound = FInitTunes();
    }

    unsafe {
        let _ = RegCloseKeyGuard::new(handle);
    }
}

pub unsafe fn WritePreferences() {
    // Persist the current PREF struct back to the registry, mirroring the Win32 version.
    let (mut key_guard, _) = match w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::WRITE,
        None,
    ) {
        Ok(result) => result,
        Err(_) => return,
    };
    let handle = key_guard.leak();

    let prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Persist the difficulty, board dimensions, and flags exactly as the original did.
    unsafe {
        WriteInt(&handle, 0, prefs.wGameType as i32);
        WriteInt(&handle, 2, prefs.Height);
        WriteInt(&handle, 3, prefs.Width);
        WriteInt(&handle, 1, prefs.Mines);
        WriteInt(&handle, 7, bool_to_i32(prefs.fMark));
        WriteInt(&handle, 17, 1);

        WriteInt(&handle, 10, bool_to_i32(prefs.fColor));
        WriteInt(&handle, 6, prefs.fSound);
        WriteInt(&handle, 4, prefs.xWindow);
        WriteInt(&handle, 5, prefs.yWindow);

        WriteInt(&handle, 11, prefs.rgTime[GameType::Begin as usize]);
        WriteInt(&handle, 13, prefs.rgTime[GameType::Inter as usize]);
        WriteInt(&handle, 15, prefs.rgTime[GameType::Expert as usize]);

        WriteSz(&handle, 12, prefs.szBegin.as_ptr());
        WriteSz(&handle, 14, prefs.szInter.as_ptr());
        WriteSz(&handle, 16, prefs.szExpert.as_ptr());

        let _ = RegCloseKeyGuard::new(handle);
    }
}

pub unsafe fn WriteInt(handle: &w::HKEY, isz_pref: i32, val: i32) {
    // Simple DWORD setter used by both the registry migration and the dialog code.
    if handle.ptr().is_null() {
        return;
    }
    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Dword(val as u32));
}

pub unsafe fn WriteSz(handle: &w::HKEY, isz_pref: i32, sz: *const u16) {
    // Stores zero-terminated UTF-16 values such as player names.
    if handle.ptr().is_null() || sz.is_null() {
        return;
    }
    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let value = match unsafe { wide_ptr_to_string(sz) } {
        Some(text) => text,
        None => return,
    };

    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Sz(value));
}

pub(crate) fn pref_key_literal(index: i32) -> Option<&'static str> {
    if index < 0 {
        return None;
    }
    PREF_STRINGS.get(index as usize).copied()
}

fn pref_name_string(index: i32) -> Option<String> {
    pref_key_literal(index).map(|s| s.to_string())
}

fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn bool_to_i32(flag: bool) -> i32 {
    if flag { 1 } else { 0 }
}

unsafe fn copy_str_to_wide(src: &str, dst: *mut u16, capacity: usize) {
    if dst.is_null() || capacity == 0 {
        return;
    }

    let mut buffer: Vec<u16> = src.encode_utf16().collect();
    buffer.push(0);
    unsafe {
        copy_wide_with_capacity(buffer.as_ptr(), dst, capacity);
    }
}

unsafe fn wide_ptr_to_string(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    let len = unsafe { wide_len(ptr) };
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf16_lossy(slice))
}

unsafe fn copy_wide_with_capacity(src: *const u16, dst: *mut u16, capacity: usize) {
    if src.is_null() || dst.is_null() || capacity == 0 {
        return;
    }

    unsafe {
        let src_slice = core::slice::from_raw_parts(src, capacity);
        let dst_slice = core::slice::from_raw_parts_mut(dst, capacity);
        for (i, ch) in src_slice.iter().copied().enumerate() {
            dst_slice[i] = ch;
            if ch == 0 {
                return;
            }
        }
        dst_slice[capacity.saturating_sub(1)] = 0;
    }
}

unsafe fn wide_len(mut ptr: *const u16) -> usize {
    if ptr.is_null() {
        return 0;
    }
    let mut len = 0usize;
    unsafe {
        while *ptr != 0 {
            len += 1;
            ptr = ptr.add(1);
        }
    }
    len
}

unsafe fn copy_default_name(dst: *mut u16) {
    if dst.is_null() {
        return;
    }

    let state = global_state();
    let guard = match state.sz_default_name.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let src = guard.as_ptr();
    unsafe {
        copy_wide_with_capacity(src, dst, CCH_NAME_MAX);
    }
}
