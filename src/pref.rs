// Registry-backed preference helpers mirrored from pref.c.
use core::sync::atomic::{AtomicBool, Ordering};

use winsafe::{self as w, RegistryValue, co, guard::RegCloseKeyGuard};

use crate::globals::global_state;
use crate::rtns::{preferences_mutex, xBoxMac, yBoxMac};
use crate::sound::FInitTunes;

/// Maximum length (UTF-16 code units) of player names stored in the registry.
pub const CCH_NAME_MAX: usize = 32;
/// Total count of preference keys mirrored from the WinMine registry hive.
pub const PREF_KEY_COUNT: usize = 18;

/// Preference key identifiers matching the legacy registry order.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum PrefKey {
    Difficulty = 0,
    Mines = 1,
    Height = 2,
    Width = 3,
    Xpos = 4,
    Ypos = 5,
    Sound = 6,
    Mark = 7,
    Menu = 8,
    Tick = 9,
    Color = 10,
    Time1 = 11,
    Name1 = 12,
    Time2 = 13,
    Name2 = 14,
    Time3 = 15,
    Name3 = 16,
    AlreadyPlayed = 17,
}

/// Discrete sound preference persisted to the registry.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SoundState {
    Off = 2,
    On = 3,
}

/// Menu visibility preferences stored in the registry.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MenuMode {
    AlwaysOn = 0,
    Hidden = 1,
    On = 2,
}

/// Minimum board height allowed by the game.
pub const MINHEIGHT: i32 = 9;
/// Default board height used on first run.
pub const DEFHEIGHT: i32 = 9;
/// Minimum board width allowed by the game.
pub const MINWIDTH: i32 = 9;
/// Default board width used on first run.
pub const DEFWIDTH: i32 = 9;

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
const PREF_STRINGS: [&str; PREF_KEY_COUNT] = [
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
    pub fSound: SoundState,
    pub fMark: bool,
    pub fTick: bool,
    pub fMenu: MenuMode,
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
    key: PrefKey,
    val_default: i32,
    val_min: i32,
    val_max: i32,
) -> i32 {
    // Registry integer fetch with clamping equivalent to the legacy ReadInt helper.
    if handle.ptr().is_null() {
        return val_default;
    }

    let key_name = match pref_name_string(key) {
        Some(name) => name,
        None => return val_default,
    };

    let value = match handle.RegQueryValueEx(Some(&key_name)) {
        Ok(RegistryValue::Dword(val)) => val as i32,
        _ => return val_default,
    };

    clamp_i32(value, val_min, val_max)
}

pub unsafe fn ReadSz(handle: &w::HKEY, key: PrefKey, sz_ret: *mut u16) {
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

    let key_name = match pref_name_string(key) {
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
        let height = ReadInt(&handle, PrefKey::Height, MINHEIGHT, DEFHEIGHT, 25);
        yBoxMac.store(height, Ordering::Relaxed);
        prefs.Height = height;

        let width = ReadInt(&handle, PrefKey::Width, MINWIDTH, DEFWIDTH, 30);
        xBoxMac.store(width, Ordering::Relaxed);
        prefs.Width = width;

        let game_raw = ReadInt(
            &handle,
            PrefKey::Difficulty,
            GameType::Begin as i32,
            GameType::Begin as i32,
            GameType::Expert as i32 + 1,
        );
        prefs.wGameType = match game_raw {
            0 => GameType::Begin,
            1 => GameType::Inter,
            2 => GameType::Expert,
            _ => GameType::Other,
        };
        prefs.Mines = ReadInt(&handle, PrefKey::Mines, 10, 10, 999);
        prefs.xWindow = ReadInt(&handle, PrefKey::Xpos, 80, 0, 1024);
        prefs.yWindow = ReadInt(&handle, PrefKey::Ypos, 80, 0, 1024);

        let sound_raw = ReadInt(
            &handle,
            PrefKey::Sound,
            SoundState::Off as i32,
            SoundState::Off as i32,
            SoundState::On as i32,
        );
        prefs.fSound = if sound_raw == SoundState::On as i32 {
            SoundState::On
        } else {
            SoundState::Off
        };
        prefs.fMark = ReadInt(&handle, PrefKey::Mark, 1, 0, 1) != 0;
        prefs.fTick = ReadInt(&handle, PrefKey::Tick, 0, 0, 1) != 0;
        let menu_raw = ReadInt(
            &handle,
            PrefKey::Menu,
            MenuMode::AlwaysOn as i32,
            MenuMode::AlwaysOn as i32,
            MenuMode::On as i32,
        );
        prefs.fMenu = menu_mode_from_raw(menu_raw);

        prefs.rgTime[GameType::Begin as usize] = ReadInt(&handle, PrefKey::Time1, 999, 0, 999);
        prefs.rgTime[GameType::Inter as usize] = ReadInt(&handle, PrefKey::Time2, 999, 0, 999);
        prefs.rgTime[GameType::Expert as usize] = ReadInt(&handle, PrefKey::Time3, 999, 0, 999);

        ReadSz(&handle, PrefKey::Name1, prefs.szBegin.as_mut_ptr());
        ReadSz(&handle, PrefKey::Name2, prefs.szInter.as_mut_ptr());
        ReadSz(&handle, PrefKey::Name3, prefs.szExpert.as_mut_ptr());
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
    prefs.fColor = unsafe { ReadInt(&handle, PrefKey::Color, default_color, 0, 1) } != 0;

    // If sound is enabled, verify that the system can actually play the resources.
    if prefs.fSound == SoundState::On {
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
        WriteInt(&handle, PrefKey::Difficulty, prefs.wGameType as i32);
        WriteInt(&handle, PrefKey::Height, prefs.Height);
        WriteInt(&handle, PrefKey::Width, prefs.Width);
        WriteInt(&handle, PrefKey::Mines, prefs.Mines);
        WriteInt(&handle, PrefKey::Mark, bool_to_i32(prefs.fMark));
        WriteInt(&handle, PrefKey::AlreadyPlayed, 1);

        WriteInt(&handle, PrefKey::Color, bool_to_i32(prefs.fColor));
        WriteInt(&handle, PrefKey::Sound, prefs.fSound as i32);
        WriteInt(&handle, PrefKey::Xpos, prefs.xWindow);
        WriteInt(&handle, PrefKey::Ypos, prefs.yWindow);

        WriteInt(
            &handle,
            PrefKey::Time1,
            prefs.rgTime[GameType::Begin as usize],
        );
        WriteInt(
            &handle,
            PrefKey::Time2,
            prefs.rgTime[GameType::Inter as usize],
        );
        WriteInt(
            &handle,
            PrefKey::Time3,
            prefs.rgTime[GameType::Expert as usize],
        );

        WriteSz(&handle, PrefKey::Name1, prefs.szBegin.as_ptr());
        WriteSz(&handle, PrefKey::Name2, prefs.szInter.as_ptr());
        WriteSz(&handle, PrefKey::Name3, prefs.szExpert.as_ptr());

        let _ = RegCloseKeyGuard::new(handle);
    }
}

pub unsafe fn WriteInt(handle: &w::HKEY, key: PrefKey, val: i32) {
    // Simple DWORD setter used by both the registry migration and the dialog code.
    if handle.ptr().is_null() {
        return;
    }
    let key_name = match pref_name_string(key) {
        Some(name) => name,
        None => return,
    };

    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Dword(val as u32));
}

pub unsafe fn WriteSz(handle: &w::HKEY, key: PrefKey, sz: *const u16) {
    // Stores zero-terminated UTF-16 values such as player names.
    if handle.ptr().is_null() || sz.is_null() {
        return;
    }
    let key_name = match pref_name_string(key) {
        Some(name) => name,
        None => return,
    };

    let value = match unsafe { wide_ptr_to_string(sz) } {
        Some(text) => text,
        None => return,
    };

    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Sz(value));
}

pub(crate) fn pref_key_literal(key: PrefKey) -> Option<&'static str> {
    PREF_STRINGS.get(key as usize).copied()
}

fn pref_name_string(key: PrefKey) -> Option<String> {
    pref_key_literal(key).map(|s| s.to_string())
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

fn menu_mode_from_raw(value: i32) -> MenuMode {
    match value {
        0 => MenuMode::AlwaysOn,
        1 => MenuMode::Hidden,
        2 => MenuMode::On,
        _ => MenuMode::AlwaysOn,
    }
}
