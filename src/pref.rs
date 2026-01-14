use core::sync::atomic::Ordering;

use winsafe::{self as w, RegistryValue, co};

use crate::globals::DEFAULT_PLAYER_NAME;
use crate::rtns::{BOARD_HEIGHT, BOARD_WIDTH, preferences_mutex};
use crate::sound::FInitTunes;

/// Maximum length (UTF-16 code units) of player names stored in the registry.
pub const CCH_NAME_MAX: usize = 32;
/// Total count of preference keys mirrored from the WinMine registry hive.
pub const PREF_KEY_COUNT: usize = 18;

/// Preference keys used to read and write settings from the registry.
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

/// Sound effect preferences stored in the registry.
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

/// Strings corresponding to each preference key for registry access.
///
/// The order matches the `PrefKey` enum.
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

/// Structure containing all user preferences.
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

/// Read an integer preference from the registry with clamping.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to read
/// * `val_default` - Default value if the read fails
/// * `val_min` - Minimum allowed value
/// * `val_max` - Maximum allowed value
/// # Returns
/// The retrieved integer value, clamped within the specified range
pub fn ReadInt(
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

    let key_name = match pref_key_literal(key) {
        Some(name) => name,
        None => return val_default,
    };

    let value = match handle.RegQueryValueEx(Some(key_name)) {
        Ok(RegistryValue::Dword(val)) => val as i32,
        _ => return val_default,
    };

    value.max(val_min).min(val_max)
}

/// Read a string preference from the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to read
/// # Returns
/// The retrieved string, or the default name on failure
fn ReadSz(handle: &w::HKEY, key: PrefKey) -> String {
    if handle.ptr().is_null() {
        return DEFAULT_PLAYER_NAME.to_string();
    }

    let Some(key_name) = pref_key_literal(key) else {
        return DEFAULT_PLAYER_NAME.to_string();
    };

    match handle.RegQueryValueEx(Some(key_name)) {
        Ok(RegistryValue::Sz(value)) | Ok(RegistryValue::ExpandSz(value)) => value,
        _ => DEFAULT_PLAYER_NAME.to_string(),
    }
}

/// Read all user preferences from the registry into the shared PREF struct.
pub fn ReadPreferences() {
    // Fetch persisted dimensions, timers, and feature flags from the WinMine registry hive.
    let (key_guard, _) = match w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::READ,
        None,
    ) {
        Ok(result) => result,
        Err(_) => return,
    };

    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let height = ReadInt(&key_guard, PrefKey::Height, MINHEIGHT, DEFHEIGHT, 25);
    BOARD_HEIGHT.store(height, Ordering::Relaxed);
    prefs.Height = height;

    let width = ReadInt(&key_guard, PrefKey::Width, MINWIDTH, DEFWIDTH, 30);
    BOARD_WIDTH.store(width, Ordering::Relaxed);
    prefs.Width = width;

    let game_raw = ReadInt(
        &key_guard,
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
    prefs.Mines = ReadInt(&key_guard, PrefKey::Mines, 10, 10, 999);
    prefs.xWindow = ReadInt(&key_guard, PrefKey::Xpos, 80, 0, 1024);
    prefs.yWindow = ReadInt(&key_guard, PrefKey::Ypos, 80, 0, 1024);

    let sound_raw = ReadInt(
        &key_guard,
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
    prefs.fMark = ReadInt(&key_guard, PrefKey::Mark, 1, 0, 1) != 0;
    prefs.fTick = ReadInt(&key_guard, PrefKey::Tick, 0, 0, 1) != 0;
    let menu_raw = ReadInt(
        &key_guard,
        PrefKey::Menu,
        MenuMode::AlwaysOn as i32,
        MenuMode::AlwaysOn as i32,
        MenuMode::On as i32,
    );
    prefs.fMenu = menu_mode_from_raw(menu_raw);

    prefs.rgTime[GameType::Begin as usize] = ReadInt(&key_guard, PrefKey::Time1, 999, 0, 999);
    prefs.rgTime[GameType::Inter as usize] = ReadInt(&key_guard, PrefKey::Time2, 999, 0, 999);
    prefs.rgTime[GameType::Expert as usize] = ReadInt(&key_guard, PrefKey::Time3, 999, 0, 999);

    prefs.szBegin = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name1));
    prefs.szInter = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name2));
    prefs.szExpert = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name3));

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
    prefs.fColor = ReadInt(&key_guard, PrefKey::Color, default_color, 0, 1) != 0;

    // If sound is enabled, verify that the system can actually play the resources.
    if prefs.fSound == SoundState::On {
        prefs.fSound = FInitTunes();
    }
}

/// Write all user preferences from the shared PREF struct into the registry.
/// # Returns
/// Result indicating success or failure
pub fn WritePreferences() -> Result<(), Box<dyn std::error::Error>> {
    // Persist the current PREF struct back to the registry, mirroring the Win32 version.
    let (key_guard, _) = match w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::WRITE,
        None,
    ) {
        Ok(result) => result,
        Err(e) => return Err(format!("Failed to open registry key: {}", e).into()),
    };

    let prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Persist the difficulty, board dimensions, and flags exactly as the original did.
    WriteInt(&key_guard, PrefKey::Difficulty, prefs.wGameType as i32)?;
    WriteInt(&key_guard, PrefKey::Height, prefs.Height)?;
    WriteInt(&key_guard, PrefKey::Width, prefs.Width)?;
    WriteInt(&key_guard, PrefKey::Mines, prefs.Mines)?;
    WriteInt(&key_guard, PrefKey::Mark, prefs.fMark as i32)?;
    WriteInt(&key_guard, PrefKey::AlreadyPlayed, 1)?;

    WriteInt(&key_guard, PrefKey::Color, prefs.fColor as i32)?;
    WriteInt(&key_guard, PrefKey::Sound, prefs.fSound as i32)?;
    WriteInt(&key_guard, PrefKey::Xpos, prefs.xWindow)?;
    WriteInt(&key_guard, PrefKey::Ypos, prefs.yWindow)?;

    WriteInt(
        &key_guard,
        PrefKey::Time1,
        prefs.rgTime[GameType::Begin as usize],
    )?;
    WriteInt(
        &key_guard,
        PrefKey::Time2,
        prefs.rgTime[GameType::Inter as usize],
    )?;
    WriteInt(
        &key_guard,
        PrefKey::Time3,
        prefs.rgTime[GameType::Expert as usize],
    )?;

    WriteSz(&key_guard, PrefKey::Name1, prefs.szBegin.as_ptr())?;
    WriteSz(&key_guard, PrefKey::Name2, prefs.szInter.as_ptr())?;
    WriteSz(&key_guard, PrefKey::Name3, prefs.szExpert.as_ptr())?;
    Ok(())
}

/// Write an integer preference to the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to write
/// * `val` - Integer value to store
/// # Returns
/// Result indicating success or failure
fn WriteInt(handle: &w::HKEY, key: PrefKey, val: i32) -> Result<(), Box<dyn std::error::Error>> {
    // Simple DWORD setter used by both the registry migration and the dialog code.
    if handle.ptr().is_null() {
        return Err("Invalid registry handle".into());
    }
    let key_name = match pref_key_literal(key) {
        Some(name) => name,
        None => return Err("Invalid preference key".into()),
    };

    handle.RegSetValueEx(Some(key_name), RegistryValue::Dword(val as u32))?;
    Ok(())
}

/// Write a string preference to the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to write
/// * `sz` - Pointer to zero-terminated UTF-16 string to store
/// # Returns
/// Result indicating success or failure
fn WriteSz(
    handle: &w::HKEY,
    key: PrefKey,
    sz: *const u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Stores zero-terminated UTF-16 values such as player names.
    if handle.ptr().is_null() {
        return Err("Invalid registry handle".into());
    }
    if sz.is_null() {
        return Err("Invalid string pointer".into());
    }
    let key_name = match pref_key_literal(key) {
        Some(name) => name,
        None => return Err("Invalid preference key".into()),
    };

    let value = match wide_ptr_to_string(sz) {
        Some(text) => text,
        None => return Err("Invalid string data".into()),
    };

    handle.RegSetValueEx(Some(key_name), RegistryValue::Sz(value))?;
    Ok(())
}

/// Retrieve the string literal for a given preference key.
/// # Arguments
/// * `key` - Preference key to look up
/// # Returns
/// Option containing the string literal, or None if the key is invalid
pub(crate) fn pref_key_literal(key: PrefKey) -> Option<&'static str> {
    PREF_STRINGS.get(key as usize).copied()
}

/// Convert a string slice into a fixed-size UTF-16 array suitable for registry storage
/// # Arguments
/// * `src` - Source string slice to convert
/// # Returns
/// Fixed-size UTF-16 array with null termination
fn string_to_fixed_wide(src: &str) -> [u16; CCH_NAME_MAX] {
    // Create a zero-filled fixed-size UTF-16 array
    let mut out = [0u16; CCH_NAME_MAX];
    // Encode the string into UTF-16 and copy up to CCH_NAME_MAX - 1 characters to the array (reserving space for null terminator)
    src.encode_utf16()
        .take(CCH_NAME_MAX - 1)
        .enumerate()
        .for_each(|(i, ch)| out[i] = ch);
    out
}

/// Convert a pointer to a zero-terminated UTF-16 string into a String.
/// # Arguments
/// * `ptr` - Pointer to the UTF-16 string
/// # Returns
/// Option containing the String, or None if the pointer is null
fn wide_ptr_to_string(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    let len = wide_len(ptr);
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf16_lossy(slice))
}

/// Calculate the length of a zero-terminated UTF-16 string.
/// # Arguments
/// * `ptr` - Pointer to the UTF-16 string
/// # Returns
/// Length of the string in UTF-16 code units
fn wide_len(mut ptr: *const u16) -> usize {
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

/// Convert a raw integer value into a MenuMode enum.
/// # Arguments
/// * `value` - Raw integer value from preferences
/// # Returns
/// Corresponding MenuMode enum variant
fn menu_mode_from_raw(value: i32) -> MenuMode {
    match value {
        0 => MenuMode::AlwaysOn,
        1 => MenuMode::Hidden,
        2 => MenuMode::On,
        _ => MenuMode::AlwaysOn,
    }
}
