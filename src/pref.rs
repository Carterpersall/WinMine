//! Preference management for the Minesweeper game, including reading and writing
//! settings to the Windows registry.

use core::ops::Index;

use strum_macros::VariantArray;
use winsafe::co::{GDC, KEY, REG_OPTION};
use winsafe::{
    AnyResult, HKEY, HWND, POINT, RegistryValue, RegistryValue::Dword, RegistryValue::Sz, SysResult,
};

use crate::globals::DEFAULT_PLAYER_NAME;
use crate::sound::Sound;
use crate::util::impl_index_enum;

/// Maximum length (UTF-16 code units) of player names stored in the registry.
pub const CCH_NAME_MAX: usize = 32;

/// Preference keys used to read and write settings from the registry.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, VariantArray)]
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
    // Note: The following preferences are defined in the original
    // WinMine codebase, but are locked behind the compilation flag `WRITE_HIDDEN`,
    // which never seems to be enabled. They are therefore commented out here.
    // Whether the menu bar is shown.
    //Menu = 8,
    // Whether the game timer is enabled.
    //Tick = 9,
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

// Implement indexing for PrefKey to get the corresponding preference string from PREF_STRINGS.
impl_index_enum!(PrefKey, for<'a> [&'static str; 18]);

/// Minimum board height allowed by the game.
pub const MINHEIGHT: u32 = 9;
/// Minimum board width allowed by the game.
pub const MINWIDTH: u32 = 9;

/// Registry key path used to persist preferences.
pub const SZ_WINMINE_REG_STR: &str = "Software\\Microsoft\\winmine";

/// Difficulty presets exposed throughout the game.
///
/// TODO: Does this need to exist?
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub enum GameType {
    /// Beginner level.
    #[default]
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
    ///
    /// TODO: Look into whether this can be improved.
    const LEVEL_DATA: [(i16, u32, u32); 3] =
        [(10, MINHEIGHT, MINWIDTH), (40, 16, 16), (99, 16, 30)];

    /// Returns the preset data for a given game type, or None for custom games.
    /// # Arguments
    /// - `game`: The game type to get preset data for.
    /// # Returns
    /// - `Some((mines, height, width))` - The preset data for the given game type.
    /// - `None` - If the game type is custom.
    pub const fn preset_data(self) -> Option<(i16, u32, u32)> {
        match self {
            GameType::Begin => Some(Self::LEVEL_DATA[0]),
            GameType::Inter => Some(Self::LEVEL_DATA[1]),
            GameType::Expert => Some(Self::LEVEL_DATA[2]),
            GameType::Other => None,
        }
    }

    /// Returns the message string shown when the player achieves the fastest time for each difficulty.
    /// # Returns
    /// - The fastest time message string.
    pub const fn fastest_time_msg(self) -> &'static str {
        match self {
            GameType::Begin => {
                "You have the fastest time\rfor beginner level.\rPlease enter your name."
            }
            GameType::Inter => {
                "You have the fastest time\rfor intermediate level.\rPlease enter your name."
            }
            GameType::Expert => {
                "You have the fastest time\rfor expert level.\rPlease enter your name."
            }
            GameType::Other => "",
        }
    }
}

impl From<u32> for GameType {
    /// Create a `GameType` from a `u32` value, defaulting to `Other` for invalid values.
    /// # Arguments
    /// - `val` - The `u32` value to convert.
    /// # Returns
    /// - A `GameType` corresponding to the given value, or `Other` if the value is invalid.
    fn from(val: u32) -> GameType {
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
const PREF_STRINGS: [&str; 18] = [
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
#[derive(Default)]
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
    pub height: usize,
    /// Board width in cells.
    pub width: usize,
    /// Position of the main window.
    pub wnd_pos: POINT,
    /// Whether sound effects are enabled.
    pub sound_enabled: bool,
    /// Whether right-click marking is enabled.
    pub mark_enabled: bool,
    /// Whether to use color assets.
    pub color: bool,
    /// Best times for each difficulty level.
    ///
    /// TODO: Make this a struct or split into separate fields.
    pub best_times: [u16; 3],
    /// Player name for Beginner level.
    pub beginner_name: String,
    /// Player name for Intermediate level.
    pub inter_name: String,
    /// Player name for Expert level.
    pub expert_name: String,
}

impl Pref {
    /// Read an integer preference from the registry with clamping.
    /// # Arguments
    /// - `handle` - Open registry key handle
    /// - `key` - Preference key to read
    /// # Returns
    /// - `Ok(u32)` - The retrieved integer value
    /// - `Err` - If the preference key is invalid or if the registry value is not a DWORD
    pub fn read_int(handle: &HKEY, key: PrefKey) -> AnyResult<u32> {
        // Get the name of the preference key
        let key_name = PREF_STRINGS[key];

        // Attempt to read the DWORD value from the registry, returning the default if it fails
        match handle.RegQueryValueEx(Some(PREF_STRINGS[key]))? {
            RegistryValue::Dword(val) => Ok(val),
            val => Err(format!("Preference key {key_name} is not a DWORD: {val:?}").into()),
        }
    }

    /// Read a string preference from the registry.
    ///
    /// TODO: Does this need to exist?
    /// # Arguments
    /// - `handle` - Open registry key handle
    /// - `key` - Preference key to read
    /// # Returns
    /// - `String` - The retrieved string, or the default name on failure
    fn read_sz(handle: &HKEY, key: PrefKey) -> String {
        // Attempt to read the string value from the registry, returning the default if it fails
        match handle.RegQueryValueEx(Some(PREF_STRINGS[key])) {
            Ok(RegistryValue::Sz(value) | RegistryValue::ExpandSz(value)) => value,
            _ => DEFAULT_PLAYER_NAME.to_owned(),
        }
    }

    /// Read all user preferences from the registry into the shared PREF struct.
    /// # Returns
    /// - `Ok(())` - If preferences were successfully read and loaded
    /// - `Err` - If there was an error accessing the registry or reading preferences
    /// # Notes
    /// - Preferences are clamped to valid ranges where applicable.
    /// - If an error occurs while reading some specific preference,
    ///   the default value for that preference will be used instead.
    pub fn read_preferences(&mut self) -> SysResult<()> {
        /// Default board height used if not set in the registry.
        const DEFHEIGHT: u32 = 9;
        /// Default board width used if not set in the registry.
        const DEFWIDTH: u32 = 9;

        // Create or open the preferences registry key with read access
        let (key_guard, _) = HKEY::CURRENT_USER.RegCreateKeyEx(
            SZ_WINMINE_REG_STR,
            None,
            REG_OPTION::default(),
            KEY::READ,
            None,
        )?;

        // Get the height of the board
        self.height = Self::read_int(&key_guard, PrefKey::Height)
            .unwrap_or(DEFHEIGHT)
            .clamp(MINHEIGHT, 25) as usize;

        // Get the width of the board
        self.width = Self::read_int(&key_guard, PrefKey::Width)
            .unwrap_or(DEFWIDTH)
            .clamp(MINWIDTH, 30) as usize;

        // Get the game difficulty
        self.game_type =
            GameType::from(Self::read_int(&key_guard, PrefKey::Difficulty).unwrap_or(0));
        // Get the number of mines on the board and the window position
        self.mines = Self::read_int(&key_guard, PrefKey::Mines)
            .unwrap_or(10)
            .clamp(10, 999) as i16;
        self.wnd_pos = POINT {
            x: Self::read_int(&key_guard, PrefKey::Xpos).unwrap_or(80) as i32,
            y: Self::read_int(&key_guard, PrefKey::Ypos).unwrap_or(80) as i32,
        };
        // Get sound, marking, ticking, and menu preferences
        self.sound_enabled = matches!(Self::read_int(&key_guard, PrefKey::Sound), Ok(3));
        self.mark_enabled = Self::read_int(&key_guard, PrefKey::Mark).unwrap_or(1) != 0;

        // Get best times and player names for each difficulty level
        self.best_times[GameType::Begin as usize] = Self::read_int(&key_guard, PrefKey::Time1)
            .unwrap_or(999)
            .clamp(0, 999) as u16;
        self.best_times[GameType::Inter as usize] = Self::read_int(&key_guard, PrefKey::Time2)
            .unwrap_or(999)
            .clamp(0, 999) as u16;
        self.best_times[GameType::Expert as usize] = Self::read_int(&key_guard, PrefKey::Time3)
            .unwrap_or(999)
            .clamp(0, 999) as u16;
        self.beginner_name = Self::read_sz(&key_guard, PrefKey::Name1);
        self.inter_name = Self::read_sz(&key_guard, PrefKey::Name2);
        self.expert_name = Self::read_sz(&key_guard, PrefKey::Name3);

        // Determine whether to favor color assets (NUMCOLORS may return -1 on true color displays).
        let default_color = match HWND::GetDesktopWindow().GetDC() {
            Ok(hdc) if hdc.GetDeviceCaps(GDC::NUMCOLORS) != 2 => 1,
            _ => 0,
        };
        self.color = Self::read_int(&key_guard, PrefKey::Color).unwrap_or(default_color) != 0;
        // If sound is enabled, initialize the sound system
        if self.sound_enabled {
            self.sound_enabled = Sound::reset();
        }
        Ok(())
    }

    /// Write all user preferences from the shared PREF struct into the registry.
    /// # Returns
    /// - `Ok(())` - If preferences were successfully written to the registry
    /// - `Err` - If there was an error writing to the registry
    pub fn write_preferences(&self) -> AnyResult<()> {
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

        // Save all preferences to the registry
        Self::write(
            &key_guard,
            PrefKey::Difficulty,
            Dword(self.game_type as u32),
        )?;
        Self::write(&key_guard, PrefKey::Height, Dword(self.height as u32))?;
        Self::write(&key_guard, PrefKey::Width, Dword(self.width as u32))?;
        Self::write(&key_guard, PrefKey::Mines, Dword(self.mines as u32))?;
        Self::write(
            &key_guard,
            PrefKey::Mark,
            Dword(u32::from(self.mark_enabled)),
        )?;
        Self::write(&key_guard, PrefKey::AlreadyPlayed, Dword(1))?;

        Self::write(&key_guard, PrefKey::Color, Dword(u32::from(self.color)))?;
        Self::write(
            &key_guard,
            PrefKey::Sound,
            if self.sound_enabled {
                Dword(3)
            } else {
                Dword(2)
            },
        )?;
        Self::write(&key_guard, PrefKey::Xpos, Dword(self.wnd_pos.x as u32))?;
        Self::write(&key_guard, PrefKey::Ypos, Dword(self.wnd_pos.y as u32))?;
        Self::write(
            &key_guard,
            PrefKey::Time1,
            Dword(self.best_times[GameType::Begin as usize] as u32),
        )?;
        Self::write(
            &key_guard,
            PrefKey::Time2,
            Dword(self.best_times[GameType::Inter as usize] as u32),
        )?;
        Self::write(
            &key_guard,
            PrefKey::Time3,
            Dword(self.best_times[GameType::Expert as usize] as u32),
        )?;

        Self::write(&key_guard, PrefKey::Name1, Sz(self.beginner_name.clone()))?;
        Self::write(&key_guard, PrefKey::Name2, Sz(self.inter_name.clone()))?;
        Self::write(&key_guard, PrefKey::Name3, Sz(self.expert_name.clone()))?;
        Ok(())
    }

    /// Write a preference to the registry.
    ///
    /// TODO: Does this function need to exist?
    /// # Arguments
    /// - `handle` - Open registry key handle
    /// - `key` - Preference key to write
    /// - `val` - Registry value to store
    /// # Returns
    /// - `Ok(())` - If the value was successfully written to the registry
    /// - `Err` - If there was an error writing to the registry
    fn write(handle: &HKEY, key: PrefKey, val: RegistryValue) -> SysResult<()> {
        // Store the value in the registry
        handle.RegSetValueEx(Some(PREF_STRINGS[key]), val)
    }
}
