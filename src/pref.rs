//! Preference management for the Minesweeper game, including reading and writing
//! settings to the Windows registry.

use winsafe::co::{GDC, KEY, REG_OPTION};
use winsafe::{AnyResult, HKEY, HWND, RegistryValue, SysResult};

use crate::globals::DEFAULT_PLAYER_NAME;
use crate::rtns::preferences_mutex;

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

impl MenuMode {
    /// Create a MenuMode from a u32 value, defaulting to AlwaysOn for invalid values.
    /// # Arguments
    /// * `val` - The u32 value to convert.
    /// # Returns
    /// A MenuMode corresponding to the given value, or AlwaysOn if the value is invalid.
    pub const fn from(val: u32) -> MenuMode {
        match val {
            0 => MenuMode::AlwaysOn,
            1 => MenuMode::Hidden,
            2 => MenuMode::On,
            _ => MenuMode::AlwaysOn,
        }
    }
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

impl GameType {
    /// Mines, height, and width tuples for the preset difficulty levels.
    const LEVEL_DATA: [(i16, u32, u32); 3] =
        [(10, MINHEIGHT, MINWIDTH), (40, 16, 16), (99, 16, 30)];

    /// Returns the preset data for a given game type, or None for custom games.
    /// # Arguments
    /// * `game`: The game type to get preset data for.
    /// # Returns
    /// The preset data as (mines, height, width), or None for a custom game.
    pub const fn preset_data(&self) -> Option<(i16, u32, u32)> {
        match self {
            GameType::Begin => Some(Self::LEVEL_DATA[0]),
            GameType::Inter => Some(Self::LEVEL_DATA[1]),
            GameType::Expert => Some(Self::LEVEL_DATA[2]),
            GameType::Other => None,
        }
    }

    /// Create a GameType from a u32 value, defaulting to Other for invalid values.
    /// # Arguments
    /// * `val` - The u32 value to convert.
    /// # Returns
    /// A GameType corresponding to the given value, or Other if the value is invalid.
    pub const fn from(val: u32) -> GameType {
        match val {
            0 => GameType::Begin,
            1 => GameType::Inter,
            2 => GameType::Expert,
            _ => GameType::Other,
        }
    }
}

/// Strings corresponding to each preference key for registry access.
///
/// The order matches the `PrefKey` enum.
const PREF_STRINGS: [&str; PREF_KEY_COUNT] = [
    "Difficulty",
    "mines",
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
    pub game_type: GameType,
    /// Number of mines on the board.
    ///
    /// The maximum number of bombs is `min(999, (height - 1) * (width - 1))`.
    ///
    /// Note: The actual maximum number of bombs is `(max_height - 1) * (max_width - 1) = 667`.
    pub mines: i16,
    /// Board height in cells.
    pub height: i32,
    /// Board width in cells.
    pub width: i32,
    /// X position of the main window.
    pub wnd_x_pos: i32,
    /// Y position of the main window.
    pub wnd_y_pos: i32,
    /// Whether sound effects are enabled.
    pub sound_state: SoundState,
    /// Whether right-click marking is enabled.
    pub mark_enabled: bool,
    /// Whether the game timer is enabled.
    pub timer: bool,
    /// Menu visibility mode.
    pub menu_mode: MenuMode,
    /// Whether to use color assets.
    pub color: bool,
    /// Best times for each difficulty level.
    pub best_times: [u16; 3],
    /// Player name for Beginner level.
    pub beginner_name: String,
    /// Player name for Intermediate level.
    pub inter_name: String,
    /// Player name for Expert level.
    pub expert_name: String,
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
/// TODO: Should this function take bounds as arguments, or should clamping be done by the caller?
pub fn read_int(handle: &HKEY, key: PrefKey) -> AnyResult<u32> {
    // Get the name of the preference key
    let Some(key_name) = PREF_STRINGS.get(key as usize).copied() else {
        return Err(format!("Invalid preference key: {}", key as u8).into());
    };

    // Attempt to read the DWORD value from the registry, returning the default if it fails
    match handle.RegQueryValueEx(Some(key_name))? {
        RegistryValue::Dword(val) => Ok(val),
        val => Err(format!("Preference key {} is not a DWORD: {:?}", key_name, val).into()),
    }
}

/// Read a string preference from the registry.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to read
/// # Returns
/// The retrieved string, or the default name on failure
fn read_sz(handle: &HKEY, key: PrefKey) -> String {
    // Get the name of the preference key
    let Some(key_name) = PREF_STRINGS.get(key as usize).copied() else {
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
pub fn read_preferences() -> SysResult<()> {
    // Create or open the preferences registry key with read access
    let (key_guard, _) = HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::READ,
        None,
    )?;

    let mut prefs = preferences_mutex();

    // Get the height of the board
    prefs.height = read_int(&key_guard, PrefKey::Height)
        .unwrap_or(DEFHEIGHT)
        .clamp(MINHEIGHT, 25) as i32;

    // Get the width of the board
    prefs.width = read_int(&key_guard, PrefKey::Width)
        .unwrap_or(DEFWIDTH)
        .clamp(MINWIDTH, 30) as i32;

    // Get the game difficulty
    prefs.game_type = GameType::from(read_int(&key_guard, PrefKey::Difficulty).unwrap_or(0));
    // Get the number of mines on the board and the window position
    prefs.mines = read_int(&key_guard, PrefKey::Mines)
        .unwrap_or(10)
        .clamp(10, 999) as i16;
    // TODO: The original bounds for window position were 0-1024, which made sense on 1990s displays, but is too small for modern screens.
    prefs.wnd_x_pos = read_int(&key_guard, PrefKey::Xpos)
        .unwrap_or(80)
        .clamp(0, 1024) as i32;
    prefs.wnd_y_pos = read_int(&key_guard, PrefKey::Ypos)
        .unwrap_or(80)
        .clamp(0, 1024) as i32;

    // Get sound, marking, ticking, and menu preferences
    prefs.sound_state = match read_int(&key_guard, PrefKey::Sound) {
        Ok(val) if val == SoundState::On as u32 => SoundState::On,
        _ => SoundState::Off,
    };
    prefs.mark_enabled = read_int(&key_guard, PrefKey::Mark).unwrap_or(1) != 0;
    prefs.timer = read_int(&key_guard, PrefKey::Tick).unwrap_or(0) != 0;
    prefs.menu_mode = MenuMode::from(read_int(&key_guard, PrefKey::Menu).unwrap_or(0));

    // Get best times and player names for each difficulty level
    prefs.best_times[GameType::Begin as usize] = read_int(&key_guard, PrefKey::Time1)
        .unwrap_or(999)
        .clamp(0, 999) as u16;
    prefs.best_times[GameType::Inter as usize] = read_int(&key_guard, PrefKey::Time2)
        .unwrap_or(999)
        .clamp(0, 999) as u16;
    prefs.best_times[GameType::Expert as usize] = read_int(&key_guard, PrefKey::Time3)
        .unwrap_or(999)
        .clamp(0, 999) as u16;
    prefs.beginner_name = read_sz(&key_guard, PrefKey::Name1);
    prefs.inter_name = read_sz(&key_guard, PrefKey::Name2);
    prefs.expert_name = read_sz(&key_guard, PrefKey::Name3);

    // Determine whether to favor color assets (NUMCOLORS may return -1 on true color displays).
    let desktop = HWND::GetDesktopWindow();
    let default_color = match desktop.GetDC() {
        Ok(hdc) if hdc.GetDeviceCaps(GDC::NUMCOLORS) != 2 => 1,
        _ => 0,
    };
    prefs.color = read_int(&key_guard, PrefKey::Color).unwrap_or(default_color) != 0;

    // If sound is enabled, initialize the sound system
    if prefs.sound_state == SoundState::On {
        prefs.sound_state = SoundState::init();
    }
    Ok(())
}

/// Write all user preferences from the shared PREF struct into the registry.
/// # Returns
/// An `Ok(())` if successful, or an error if writing failed.
pub fn write_preferences() -> AnyResult<()> {
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

    let prefs = preferences_mutex();

    // Save all preferences to the registry
    write_int(&key_guard, PrefKey::Difficulty, prefs.game_type as u32)?;
    write_int(&key_guard, PrefKey::Height, prefs.height as u32)?;
    write_int(&key_guard, PrefKey::Width, prefs.width as u32)?;
    write_int(&key_guard, PrefKey::Mines, prefs.mines as u32)?;
    write_int(&key_guard, PrefKey::Mark, u32::from(prefs.mark_enabled))?;
    write_int(&key_guard, PrefKey::AlreadyPlayed, 1)?;

    write_int(&key_guard, PrefKey::Color, u32::from(prefs.color))?;
    write_int(&key_guard, PrefKey::Sound, prefs.sound_state as u32)?;
    write_int(&key_guard, PrefKey::Xpos, prefs.wnd_x_pos as u32)?;
    write_int(&key_guard, PrefKey::Ypos, prefs.wnd_y_pos as u32)?;

    write_int(
        &key_guard,
        PrefKey::Time1,
        prefs.best_times[GameType::Begin as usize] as u32,
    )?;
    write_int(
        &key_guard,
        PrefKey::Time2,
        prefs.best_times[GameType::Inter as usize] as u32,
    )?;
    write_int(
        &key_guard,
        PrefKey::Time3,
        prefs.best_times[GameType::Expert as usize] as u32,
    )?;

    write_sz(&key_guard, PrefKey::Name1, &prefs.beginner_name)?;
    write_sz(&key_guard, PrefKey::Name2, &prefs.inter_name)?;
    write_sz(&key_guard, PrefKey::Name3, &prefs.expert_name)?;
    Ok(())
}

/// Write an integer preference to the registry.
///
/// TODO: Take `RegistryValue` directly, allowing write functions to be merged.
/// # Arguments
/// * `handle` - Open registry key handle
/// * `key` - Preference key to write
/// * `val` - Integer value to store
/// # Returns
/// An `Ok(())` if successful, or an error if writing failed.
fn write_int(handle: &HKEY, key: PrefKey, val: u32) -> AnyResult<()> {
    // Get the name of the preference key
    let Some(key_name) = PREF_STRINGS.get(key as usize).copied() else {
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
/// * `sz` - String to store
/// # Returns
/// An `Ok(())` if successful, or an error if writing failed.
fn write_sz(handle: &HKEY, key: PrefKey, sz: &String) -> AnyResult<()> {
    // Get the name of the preference key
    let Some(key_name) = PREF_STRINGS.get(key as usize).copied() else {
        return Err("Invalid preference key".into());
    };

    // Store the string value in the registry
    handle.RegSetValueEx(Some(key_name), RegistryValue::Sz(sz.to_string()))?;
    Ok(())
}
