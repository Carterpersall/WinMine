use core::sync::atomic::Ordering;

use winsafe::co::{GDC, KEY, REG_OPTION};
use winsafe::{HKEY, HWND, RegistryValue};

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
    /// Game difficulty preference.
    Difficulty = 0,
    /// Number of mines on the board.
    Mines = 1,
    /// Board height in cells.
    Height = 2,
    /// Board width in cells.
    Width = 3,
    /// X position of the main window.
    Xpos = 4,
    /// Y position of the main window.
    Ypos = 5,
    /// Whether sound effects are enabled.
    Sound = 6,
    /// Whether right-click marking is enabled.
    Mark = 7,
    /// Whether the menu bar is shown.
    Menu = 8,
    /// Whether the game timer is enabled.
    Tick = 9,
    /// Whether to use color assets.
    Color = 10,
    /// Best time for Beginner level.
    Time1 = 11,
    /// Player name for Beginner level.
    Name1 = 12,
    /// Best time for Intermediate level.
    Time2 = 13,
    /// Player name for Intermediate level.
    Name2 = 14,
    /// Best time for Expert level.
    Time3 = 15,
    /// Player name for Expert level.
    Name3 = 16,
    /// Flag indicating if the user has played the game before.
    AlreadyPlayed = 17,
}

/// Sound effect preferences.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SoundState {
    /// Sound effects are disabled.
    Off = 2,
    /// Sound effects are enabled.
    On = 3,
}

/// Menu visibility preferences.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MenuMode {
    /// Menu is always shown.
    AlwaysOn = 0,
    /// Menu is always hidden.
    Hidden = 1,
    /// TODO: Is this mode used anywhere? And if it is, what does it do?
    On = 2,
}

/// Minimum board height allowed by the game.
pub const MINHEIGHT: u32 = 9;
/// Default board height used on first run.
pub const DEFHEIGHT: u32 = 9;
/// Minimum board width allowed by the game.
pub const MINWIDTH: u32 = 9;
/// Default board width used on first run.
pub const DEFWIDTH: u32 = 9;

/// Registry key path used to persist preferences.
pub const SZ_WINMINE_REG_STR: &str = "Software\\Microsoft\\winmine";

/// Difficulty presets exposed throughout the game.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum GameType {
    /// Beginner level.
    Begin = 0,
    /// Intermediate level.
    Inter = 1,
    /// Expert level.
    Expert = 2,
    /// Custom level.
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
    /// Current game difficulty (Beginner, Intermediate, Expert, Custom).
    pub wGameType: GameType,
    /// Number of mines on the board.
    pub Mines: u32,
    /// Board height in cells.
    pub Height: i32,
    /// Board width in cells.
    pub Width: i32,
    /// X position of the main window.
    pub xWindow: i32,
    /// Y position of the main window.
    pub yWindow: i32,
    /// Whether sound effects are enabled.
    pub fSound: SoundState,
    /// Whether right-click marking is enabled.
    pub fMark: bool,
    /// Whether the game timer is enabled.
    pub fTick: bool,
    /// Menu visibility mode.
    pub fMenu: MenuMode,
    /// Whether to use color assets.
    pub fColor: bool,
    /// Best times for each difficulty level.
    pub rgTime: [u32; 3],
    /// Player name for Beginner level.
    pub szBegin: [u16; CCH_NAME_MAX],
    /// Player name for Intermediate level.
    pub szInter: [u16; CCH_NAME_MAX],
    /// Player name for Expert level.
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
///
/// TODO: Change return type to option or result so this function does not need to handle defaults.
pub fn ReadInt(handle: &HKEY, key: PrefKey, val_default: u32, val_min: u32, val_max: u32) -> u32 {
    // Get the name of the preference key
    let Some(key_name) = pref_key_literal(key) else {
        return val_default;
    };

    // Attempt to read the DWORD value from the registry, returning the default if it fails
    let value = match handle.RegQueryValueEx(Some(key_name)) {
        Ok(RegistryValue::Dword(val)) => val,
        _ => return val_default,
    };

    // Clamp the value within the specified range and return it
    value.clamp(val_min, val_max)
}

/// Read a string preference from the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to read
/// # Returns
/// The retrieved string, or the default name on failure
fn ReadSz(handle: &HKEY, key: PrefKey) -> String {
    // Get the name of the preference key
    let Some(key_name) = pref_key_literal(key) else {
        return DEFAULT_PLAYER_NAME.to_string();
    };

    // Attempt to read the string value from the registry, returning the default if it fails
    match handle.RegQueryValueEx(Some(key_name)) {
        Ok(RegistryValue::Sz(value) | RegistryValue::ExpandSz(value)) => value,
        _ => DEFAULT_PLAYER_NAME.to_string(),
    }
}

/// Read all user preferences from the registry into the shared PREF struct.
///
/// TODO: Should this return a Result to indicate failure? It currently just uses defaults on failure,
/// which would cause the current settings to be overwritten on the next save.
pub fn ReadPreferences() {
    // Create or open the preferences registry key with read access
    let Ok((key_guard, _)) = HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::READ,
        None,
    ) else {
        return;
    };

    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Get the height of the board
    let height = ReadInt(&key_guard, PrefKey::Height, MINHEIGHT, DEFHEIGHT, 25) as i32;
    BOARD_HEIGHT.store(height, Ordering::Relaxed);
    prefs.Height = height;

    // Get the width of the board
    let width = ReadInt(&key_guard, PrefKey::Width, MINWIDTH, DEFWIDTH, 30) as i32;
    BOARD_WIDTH.store(width, Ordering::Relaxed);
    prefs.Width = width;

    // Get the game difficulty
    let game_raw = ReadInt(
        &key_guard,
        PrefKey::Difficulty,
        GameType::Begin as u32,
        GameType::Begin as u32,
        GameType::Other as u32,
    );
    // Convert the raw integer into the corresponding GameType enum variant
    prefs.wGameType = match game_raw {
        0 => GameType::Begin,
        1 => GameType::Inter,
        2 => GameType::Expert,
        _ => GameType::Other,
    };
    // Get the number of mines on the board and the window position
    prefs.Mines = ReadInt(&key_guard, PrefKey::Mines, 10, 10, 999);
    // TODO: These values are either not saved properly or are ignored when the window is created
    prefs.xWindow = ReadInt(&key_guard, PrefKey::Xpos, 80, 0, 1024) as i32;
    prefs.yWindow = ReadInt(&key_guard, PrefKey::Ypos, 80, 0, 1024) as i32;

    // Get sound, marking, ticking, and menu preferences
    let sound_raw = ReadInt(
        &key_guard,
        PrefKey::Sound,
        SoundState::Off as u32,
        SoundState::Off as u32,
        SoundState::On as u32,
    );
    prefs.fSound = if sound_raw == SoundState::On as u32 {
        SoundState::On
    } else {
        SoundState::Off
    };
    prefs.fMark = ReadInt(&key_guard, PrefKey::Mark, 1, 0, 1) != 0;
    prefs.fTick = ReadInt(&key_guard, PrefKey::Tick, 0, 0, 1) != 0;
    let menu_raw = ReadInt(
        &key_guard,
        PrefKey::Menu,
        MenuMode::AlwaysOn as u32,
        MenuMode::AlwaysOn as u32,
        MenuMode::On as u32,
    );
    prefs.fMenu = match menu_raw {
        0 => MenuMode::AlwaysOn,
        1 => MenuMode::Hidden,
        2 => MenuMode::On,
        // Unreachable due to `ReadInt`'s clamping
        _ => MenuMode::AlwaysOn,
    };

    // Get best times and player names for each difficulty level
    prefs.rgTime[GameType::Begin as usize] = ReadInt(&key_guard, PrefKey::Time1, 999, 0, 999);
    prefs.rgTime[GameType::Inter as usize] = ReadInt(&key_guard, PrefKey::Time2, 999, 0, 999);
    prefs.rgTime[GameType::Expert as usize] = ReadInt(&key_guard, PrefKey::Time3, 999, 0, 999);

    prefs.szBegin = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name1));
    prefs.szInter = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name2));
    prefs.szExpert = string_to_fixed_wide(&ReadSz(&key_guard, PrefKey::Name3));

    // Determine whether to favor color assets (NUMCOLORS may return -1 on true color displays).
    let desktop = HWND::GetDesktopWindow();
    let default_color = match desktop.GetDC() {
        Ok(hdc) => {
            if hdc.GetDeviceCaps(GDC::NUMCOLORS) != 2 {
                1
            } else {
                0
            }
        }
        Err(_) => 0,
    };
    prefs.fColor = ReadInt(&key_guard, PrefKey::Color, default_color, 0, 1) != 0;

    // If sound is enabled, initialize the sound system
    if prefs.fSound == SoundState::On {
        prefs.fSound = FInitTunes();
    }
}

/// Write all user preferences from the shared PREF struct into the registry.
/// # Returns
/// Result indicating success or failure
pub fn WritePreferences() -> Result<(), Box<dyn core::error::Error>> {
    // Create or open the preferences registry key with write access
    let (key_guard, _) = match HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::WRITE,
        None,
    ) {
        Ok(result) => result,
        Err(e) => return Err(format!("Failed to open registry key: {e}").into()),
    };

    let prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Save all preferences to the registry
    WriteInt(&key_guard, PrefKey::Difficulty, prefs.wGameType as u32)?;
    WriteInt(&key_guard, PrefKey::Height, prefs.Height as u32)?;
    WriteInt(&key_guard, PrefKey::Width, prefs.Width as u32)?;
    WriteInt(&key_guard, PrefKey::Mines, prefs.Mines as u32)?;
    WriteInt(&key_guard, PrefKey::Mark, u32::from(prefs.fMark))?;
    WriteInt(&key_guard, PrefKey::AlreadyPlayed, 1)?;

    WriteInt(&key_guard, PrefKey::Color, u32::from(prefs.fColor))?;
    WriteInt(&key_guard, PrefKey::Sound, prefs.fSound as u32)?;
    WriteInt(&key_guard, PrefKey::Xpos, prefs.xWindow as u32)?;
    WriteInt(&key_guard, PrefKey::Ypos, prefs.yWindow as u32)?;

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
fn WriteInt(handle: &HKEY, key: PrefKey, val: u32) -> Result<(), Box<dyn core::error::Error>> {
    // Get the name of the preference key
    let Some(key_name) = pref_key_literal(key) else {
        return Err("Invalid preference key".into());
    };

    // Store the DWORD value in the registry
    handle.RegSetValueEx(Some(key_name), RegistryValue::Dword(val))?;
    Ok(())
}

/// Write a string preference to the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to write
/// * `sz` - Pointer to zero-terminated UTF-16 string to store
///
/// TODO: Change the sz argument to be a &str or String instead of a pointer to a UTF-16 string.
/// # Returns
/// Result indicating success or failure
fn WriteSz(handle: &HKEY, key: PrefKey, sz: *const u16) -> Result<(), Box<dyn core::error::Error>> {
    if sz.is_null() {
        return Err("Invalid string pointer".into());
    }

    // Get the name of the preference key
    let Some(key_name) = pref_key_literal(key) else {
        return Err("Invalid preference key".into());
    };

    // Convert the UTF-16 pointer to a Rust String
    let Some(value) = wide_ptr_to_string(sz) else {
        return Err("Invalid string data".into());
    };

    // Store the string value in the registry
    handle.RegSetValueEx(Some(key_name), RegistryValue::Sz(value))?;
    Ok(())
}

/// Retrieve the string literal for a given preference key.
///
/// TODO: Remove this function.
/// # Arguments
/// * `key` - Preference key to look up
/// # Returns
/// Option containing the string literal, or None if the key is invalid
pub fn pref_key_literal(key: PrefKey) -> Option<&'static str> {
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
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf16_lossy(slice))
}

/// Calculate the length of a zero-terminated UTF-16 string.
/// # Arguments
/// * `ptr` - Pointer to the UTF-16 string
/// # Returns
/// Length of the string in UTF-16 code units
const fn wide_len(mut ptr: *const u16) -> usize {
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
