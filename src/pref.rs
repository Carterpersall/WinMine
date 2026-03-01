//! Preference management for the Minesweeper game, including reading and writing
//! settings to the Windows registry.

use strum_macros::VariantArray;
use winsafe::co::{GDC, KEY, REG_OPTION};
use winsafe::{
    AnyResult, HKEY, HWND, POINT, RegistryValue, RegistryValue::Dword, RegistryValue::Sz, SysResult,
};

use crate::globals::DEFAULT_PLAYER_NAME;
use crate::sound::Sound;

/// Maximum length (UTF-16 code units) of player names stored in the registry.
pub(crate) const CCH_NAME_MAX: usize = 32;

/// Preference keys used to read and write settings from the registry.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, VariantArray)]
enum PrefKey {
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

impl PrefKey {
    /// Get the preference key as a string slice that corresponds to the registry value name.
    /// # Returns
    /// - `Some(&'static str)` - The string slice corresponding to the preference key.
    /// # Notes
    /// - The returned string slice is used as the value name when reading and writing preferences to the registry.
    /// - `Some` is always returned to allow for easier passing of the return value to registry functions, which expect an `Option<&str>`.
    const fn string(self) -> Option<&'static str> {
        Some(match self {
            PrefKey::Difficulty => "Difficulty",
            PrefKey::Mines => "Mines",
            PrefKey::Height => "Height",
            PrefKey::Width => "Width",
            PrefKey::Xpos => "Xpos",
            PrefKey::Ypos => "Ypos",
            PrefKey::Sound => "Sound",
            PrefKey::Mark => "Mark",
            //PrefKey::Menu => "Menu",
            //PrefKey::Tick => "Tick",
            PrefKey::Color => "Color",
            PrefKey::Time1 => "Time1",
            PrefKey::Name1 => "Name1",
            PrefKey::Time2 => "Time2",
            PrefKey::Name2 => "Name2",
            PrefKey::Time3 => "Time3",
            PrefKey::Name3 => "Name3",
            PrefKey::AlreadyPlayed => "AlreadyPlayed",
        })
    }
}

/// Minimum board height allowed by the game.
pub(crate) const MINHEIGHT: u32 = 9;
/// Maximum board height allowed by the game.
pub(crate) const MAXHEIGHT: u32 = 24;
/// Minimum board width allowed by the game.
pub(crate) const MINWIDTH: u32 = 9;
/// Maximum board width allowed by the game.
pub(crate) const MAXWIDTH: u32 = 30;
/// Minimum number of mines allowed on the board.
pub(crate) const MINMINES: u32 = 10;
/// Maximum number of mines allowed on the board.
pub(crate) const MAXMINES: u32 = 999;

/// Registry key path used to persist preferences.
const SZ_WINMINE_REG_STR: &str = "Software\\Microsoft\\winmine";

/// Difficulty presets exposed throughout the game.
#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub(crate) enum GameType {
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
    /// Returns the message string shown when the player achieves the fastest time for each difficulty.
    /// # Returns
    /// - The fastest time message string.
    pub(crate) const fn fastest_time_msg(self) -> &'static str {
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

/// Structure containing all user preferences.
#[derive(Default)]
pub(crate) struct Pref {
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
    /// Player name for Beginner level.
    pub beginner_name: String,
    /// Best time for the Beginner level.
    pub beginner_time: u16,
    /// Player name for Intermediate level.
    pub inter_name: String,
    /// Best time for the Intermediate level.
    pub inter_time: u16,
    /// Player name for Expert level.
    pub expert_name: String,
    /// Best time for the Expert level.
    pub expert_time: u16,
}

impl Pref {
    /// Read an integer preference from the registry with clamping.
    /// # Arguments
    /// - `handle` - Open registry key handle
    /// - `key` - Preference key to read
    /// # Returns
    /// - `Ok(u32)` - The retrieved integer value
    /// - `Err` - If the preference key is invalid or if the registry value is not a DWORD
    fn read_int(handle: &HKEY, key: PrefKey) -> AnyResult<u32> {
        // Get the name of the preference key
        let key_name = key.string();

        // Attempt to read the DWORD value from the registry, returning the default if it fails
        match handle.RegQueryValueEx(key_name)? {
            Dword(val) => Ok(val),
            val => Err(format!("Preference key {key_name:?} is not a DWORD: {val:?}").into()),
        }
    }

    /// Read a string preference from the registry.
    /// # Arguments
    /// - `handle` - Open registry key handle
    /// - `key` - Preference key to read
    /// # Returns
    /// - `String` - The retrieved string, or the default name on failure
    fn read_sz(handle: &HKEY, key: PrefKey) -> String {
        // Attempt to read the string value from the registry, returning the default if it fails
        match handle.RegQueryValueEx(key.string()) {
            Ok(Sz(value) | RegistryValue::ExpandSz(value)) => value,
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
    pub(crate) fn read_preferences(&mut self) -> SysResult<()> {
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
            .clamp(MINHEIGHT, MAXHEIGHT) as usize;

        // Get the width of the board
        self.width = Self::read_int(&key_guard, PrefKey::Width)
            .unwrap_or(DEFWIDTH)
            .clamp(MINWIDTH, MAXWIDTH) as usize;

        // Get the game difficulty
        self.game_type =
            GameType::from(Self::read_int(&key_guard, PrefKey::Difficulty).unwrap_or(0));
        // Get the number of mines on the board and the window position
        self.mines = Self::read_int(&key_guard, PrefKey::Mines)
            .unwrap_or(10)
            .clamp(MINMINES, MAXMINES) as i16;
        self.wnd_pos = POINT {
            x: Self::read_int(&key_guard, PrefKey::Xpos).unwrap_or(80) as i32,
            y: Self::read_int(&key_guard, PrefKey::Ypos).unwrap_or(80) as i32,
        };
        // Get sound, marking, ticking, and menu preferences
        self.sound_enabled = matches!(Self::read_int(&key_guard, PrefKey::Sound), Ok(3));
        self.mark_enabled = Self::read_int(&key_guard, PrefKey::Mark).unwrap_or(1) != 0;

        // Get best times and player names for each difficulty level
        self.beginner_time = Self::read_int(&key_guard, PrefKey::Time1)
            .unwrap_or(999)
            .clamp(0, 999) as u16;
        self.inter_time = Self::read_int(&key_guard, PrefKey::Time2)
            .unwrap_or(999)
            .clamp(0, 999) as u16;
        self.expert_time = Self::read_int(&key_guard, PrefKey::Time3)
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
    pub(crate) fn write_preferences(&self) -> AnyResult<()> {
        // Create or open the preferences registry key with write access
        let (hkey, _) = match HKEY::CURRENT_USER.RegCreateKeyEx(
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
        hkey.RegSetValueEx(PrefKey::Difficulty.string(), Dword(self.game_type as u32))?;
        hkey.RegSetValueEx(PrefKey::Height.string(), Dword(self.height as u32))?;
        hkey.RegSetValueEx(PrefKey::Width.string(), Dword(self.width as u32))?;
        hkey.RegSetValueEx(PrefKey::Mines.string(), Dword(self.mines as u32))?;
        hkey.RegSetValueEx(PrefKey::Mark.string(), Dword(u32::from(self.mark_enabled)))?;
        hkey.RegSetValueEx(PrefKey::AlreadyPlayed.string(), Dword(1))?;

        hkey.RegSetValueEx(PrefKey::Color.string(), Dword(u32::from(self.color)))?;
        hkey.RegSetValueEx(
            PrefKey::Sound.string(),
            if self.sound_enabled {
                Dword(3)
            } else {
                Dword(2)
            },
        )?;
        hkey.RegSetValueEx(PrefKey::Xpos.string(), Dword(self.wnd_pos.x as u32))?;
        hkey.RegSetValueEx(PrefKey::Ypos.string(), Dword(self.wnd_pos.y as u32))?;
        hkey.RegSetValueEx(PrefKey::Time1.string(), Dword(self.beginner_time as u32))?;
        hkey.RegSetValueEx(PrefKey::Time2.string(), Dword(self.inter_time as u32))?;
        hkey.RegSetValueEx(PrefKey::Time3.string(), Dword(self.expert_time as u32))?;

        hkey.RegSetValueEx(PrefKey::Name1.string(), Sz(self.beginner_name.clone()))?;
        hkey.RegSetValueEx(PrefKey::Name2.string(), Sz(self.inter_name.clone()))?;
        hkey.RegSetValueEx(PrefKey::Name3.string(), Sz(self.expert_name.clone()))?;
        Ok(())
    }
}
