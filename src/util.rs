use core::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::System::WindowsProgramming::GetPrivateProfileIntW;

use winsafe::co::{GDC, HELPW, KEY, MB, REG_OPTION, SM};
use winsafe::{
    self as w, GetSystemMetrics, GetTickCount64, HKEY, HMENU, HWND, IdIdiStr, IdPos, WString,
    prelude::*,
};

use crate::globals::{
    CXBORDER, CYCAPTION, CYMENU, DEFAULT_PLAYER_NAME, ERR_TITLE, GAME_NAME, MSG_CREDIT,
    MSG_VERSION_NAME,
};
use crate::pref::{
    DEFHEIGHT, DEFWIDTH, GameType, MINHEIGHT, MINWIDTH, MenuMode, PrefKey, ReadInt,
    SZ_WINMINE_REG_STR, SoundState, WritePreferences, pref_key_literal,
};
use crate::rtns::{AdjustFlag, preferences_mutex};
use crate::sound::FInitTunes;
use crate::winmine::{AdjustWindow, MenuCommand};

/// Multiplier used by the linear congruential generator that produces the app's RNG values.
const RNG_MULTIPLIER: u32 = 1_103_515_245;
/// Increment used by the linear congruential generator.
const RNG_INCREMENT: u32 = 12_345;
/// Default seed applied when the RNG would otherwise start at zero.
const RNG_DEFAULT_SEED: u32 = 0xACE1_1234;
/// Shared state of the linear congruential generator.
static RNG_STATE: AtomicU32 = AtomicU32::new(RNG_DEFAULT_SEED);

/// Icon resources embedded in the executable.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum IconId {
    /// Main application icon.
    Main = 100,
}

/// Maximum path buffer used when resolving help files.
const CCH_MAX_PATHNAME: usize = 250;

/// Legacy initialization file name used for first-run migration.
///
/// TODO: Remove this once preferences are fully migrated to the registry.
const SZ_INI_FILE: &str = "entpack.ini";

/// Seed the RNG with the specified seed value.
/// # Arguments
/// * `seed` - The seed value to initialize the RNG with. If zero, a default seed is used.
fn seed_rng(seed: u32) {
    // Ensure the RNG seed is never zero
    let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
    // Initialize the shared RNG state to the given seed value
    RNG_STATE.store(value, Ordering::Relaxed);
}

/// Generate the next pseudo-random number using a linear congruential generator.
/// # Returns
/// The next pseudo-random number.
/// # Notes
/// A linear congruential generator (LCG) is a simple algorithm for generating a sequence of pseudo-random numbers.
///
/// The formula used is:
///
/// X<sub>{n+1}</sub> = (a * X<sub>n</sub> + c) mod m
///
/// Where:
/// - X is the sequence of pseudo-random values
/// - a is the multiplier (`RNG_MULTIPLIER`)
/// - c is the increment (`RNG_INCREMENT`)
/// - m is the modulus (2<sup>32</sup> for `u32` arithmetic)
///
/// This formula is the same used in Windows' `rand()` function.
///
/// TODO: Change return type to u32 since the current RNG value is stored as a u32
/// TODO: Consider using using Rust's built-in RNG facilities
fn next_rand() -> i32 {
    let mut current = RNG_STATE.load(Ordering::Relaxed);
    loop {
        // Compute the next RNG state using LCG formula
        let next = current
            .wrapping_mul(RNG_MULTIPLIER)
            .wrapping_add(RNG_INCREMENT);
        match RNG_STATE.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return ((next >> 16) & 0x7FFF) as i32,
            Err(actual) => current = actual,
        }
    }
}

/// Return a pseudo-random number in the [0, `rnd_max`) range
/// # Arguments
/// * `rnd_max` - Upper bound (exclusive) for the random number
/// # Returns
/// A pseudo-random number in the [0, `rnd_max`) range
pub fn Rnd(rnd_max: i32) -> i32 {
    if rnd_max <= 0 {
        0
    } else {
        next_rand() % rnd_max
    }
}

/// Display an error message box for the specified error ID.
///
/// TODO: Centralize error handling
/// # Arguments
/// * `id_err` - The error ID used for selecting the error message.
pub fn ReportErr(err: &str) {
    let _ = HWND::NULL.MessageBox(err, ERR_TITLE, MB::ICONHAND);
}

/// Read an integer preference from the legacy .ini file, clamping it within the specified bounds.
/// # Arguments
/// * `pref` - The preference key to read.
/// * `val_default` - Default value if the key is missing or invalid.
/// * `val_min` - Minimum allowed value.
/// * `val_max` - Maximum allowed value.
/// # Returns
/// The clamped integer preference value.
fn ReadIniInt(pref: PrefKey, val_default: i32, val_min: i32, val_max: i32) -> i32 {
    // Retrieve the key name for the preference
    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return val_default,
    };

    let ini_path = WString::from_str(SZ_INI_FILE);
    // Read the integer value from the preference storage location
    // On any modern system, this will read from the registry instead of a .ini file
    let value = unsafe {
        GetPrivateProfileIntW(
            WString::from_str(GAME_NAME).as_ptr(),
            key.as_ptr(),
            val_default,
            ini_path.as_ptr(),
        )
    };
    value.clamp(val_min, val_max)
}

/// Read a string preference from the registry into the provided buffer.
/// # Arguments
/// * `pref` - The preference key to read.
/// # Returns
/// The string preference value, or the default player name if not found.
fn ReadIniSz(pref: PrefKey) -> String {
    // Retrieve the key name for the preference
    let Some(key) = pref_key_literal(pref) else {
        return DEFAULT_PLAYER_NAME.to_string();
    };

    // Return the string value from the registry
    match w::GetPrivateProfileString(GAME_NAME, key, SZ_INI_FILE) {
        Ok(Some(text)) => text,
        _ => DEFAULT_PLAYER_NAME.to_string(),
    }
}

/// Initialize UI globals, migrate preferences from the .ini file exactly once, and seed randomness.
pub fn InitConst() {
    // Seed the RNG using the low 16 bits of the current tick count
    let ticks = (GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    // Get the system metrics for caption height, menu height, and border width
    CYCAPTION.store(GetSystemMetrics(SM::CYCAPTION) + 1, Ordering::Relaxed);
    CYMENU.store(GetSystemMetrics(SM::CYMENU) + 1, Ordering::Relaxed);
    CXBORDER.store(GetSystemMetrics(SM::CXBORDER) + 1, Ordering::Relaxed);

    // Check if the user has already played the game to avoid overwriting existing preferences
    if let Ok((key_guard, _)) = HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::READ,
        None,
    ) && ReadInt(&key_guard, PrefKey::AlreadyPlayed, 0, 0, 1) != 0
    {
        return;
    };

    // If the user has not played before, migrate preferences from the .ini file to the registry
    // TODO: Any non 16-bit Windows program should use the registry, so remove .ini support entirely
    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    prefs.Height = ReadIniInt(PrefKey::Height, MINHEIGHT as i32, DEFHEIGHT as i32, 25);
    prefs.Width = ReadIniInt(PrefKey::Width, MINWIDTH as i32, DEFWIDTH as i32, 30);
    let game_raw = ReadIniInt(
        PrefKey::Difficulty,
        GameType::Begin as i32,
        GameType::Begin as i32,
        GameType::Other as i32,
    );
    prefs.wGameType = match game_raw {
        0 => GameType::Begin,
        1 => GameType::Inter,
        2 => GameType::Expert,
        _ => GameType::Other,
    };
    prefs.Mines = ReadIniInt(PrefKey::Mines, 10, 10, 999) as i16;
    prefs.xWindow = ReadIniInt(PrefKey::Xpos, 80, 0, 1024);
    prefs.yWindow = ReadIniInt(PrefKey::Ypos, 80, 0, 1024);

    let sound_raw = ReadIniInt(
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
    prefs.fMark = ReadIniInt(PrefKey::Mark, 1, 0, 1) != 0;
    prefs.fTick = ReadIniInt(PrefKey::Tick, 0, 0, 1) != 0;
    let menu_raw = ReadIniInt(
        PrefKey::Menu,
        MenuMode::AlwaysOn as i32,
        MenuMode::AlwaysOn as i32,
        MenuMode::On as i32,
    );
    prefs.fMenu = match menu_raw {
        1 => MenuMode::Hidden,
        2 => MenuMode::On,
        _ => MenuMode::AlwaysOn,
    };

    prefs.rgTime[GameType::Begin as usize] = ReadIniInt(PrefKey::Time1, 999, 0, 999) as u16;
    prefs.rgTime[GameType::Inter as usize] = ReadIniInt(PrefKey::Time2, 999, 0, 999) as u16;
    prefs.rgTime[GameType::Expert as usize] = ReadIniInt(PrefKey::Time3, 999, 0, 999) as u16;

    prefs.szBegin = ReadIniSz(PrefKey::Name1);
    prefs.szInter = ReadIniSz(PrefKey::Name2);
    prefs.szExpert = ReadIniSz(PrefKey::Name3);

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
    prefs.fColor = ReadIniInt(PrefKey::Color, default_color, 0, 1) != 0;

    if prefs.fSound == SoundState::On {
        prefs.fSound = FInitTunes();
    }

    if let Err(e) = WritePreferences() {
        eprintln!("Failed to write preferences during initialization: {e}");
    }
}

/// Check or uncheck a menu item based on the specified command ID.
///
/// TODO: This function no longer needs to exist
/// # Arguments
/// * `idm` - The menu command ID.
/// * `f_check` - `true` to check the item, `false` to uncheck it.
pub fn CheckEm(hmenu: &HMENU, idm: MenuCommand, f_check: bool) {
    if let Some(menu) = hmenu.as_opt() {
        let _ = menu.CheckMenuItem(IdPos::Id(idm as u16), f_check);
    }
}

/// Show or hide the menu bar based on the specified mode.
/// # Arguments
/// * `hwnd` - Handle to the main window.
/// * `f_active` - The desired menu mode.
pub fn SetMenuBar(hwnd: &HWND, f_active: MenuMode) {
    // Persist the menu visibility preference, refresh accelerator state, and resize the window.
    let (menu_on, game_type, color, mark, sound) = {
        let mut prefs = match preferences_mutex().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.fMenu = f_active;
        (
            !matches!(prefs.fMenu, MenuMode::Hidden),
            prefs.wGameType,
            prefs.fColor,
            prefs.fMark,
            prefs.fSound,
        )
    };

    // Update the menu checkmarks to reflect the current preferences
    let hmenu = hwnd.GetMenu().unwrap_or(HMENU::NULL);
    CheckEm(&hmenu, MenuCommand::Begin, game_type == GameType::Begin);
    CheckEm(&hmenu, MenuCommand::Inter, game_type == GameType::Inter);
    CheckEm(&hmenu, MenuCommand::Expert, game_type == GameType::Expert);
    CheckEm(&hmenu, MenuCommand::Custom, game_type == GameType::Other);

    CheckEm(&hmenu, MenuCommand::Color, color);
    CheckEm(&hmenu, MenuCommand::Mark, mark);
    CheckEm(&hmenu, MenuCommand::Sound, sound == SoundState::On);

    // Show or hide the menu bar as set in preferences
    let menu = hwnd.GetMenu().unwrap_or(HMENU::NULL);
    let menu_arg = if menu_on { &menu } else { &HMENU::NULL };
    let _ = hwnd.SetMenu(menu_arg);
    AdjustWindow(hwnd, AdjustFlag::Resize as i32);
}

/// Display the About dialog box with version and credit information.
///
/// TODO: Remove this function
/// # Arguments
/// * `hwnd` - Handle to the main window.
pub fn DoAbout(hwnd: &HWND) {
    let icon_guard = hwnd
        .hinstance()
        .LoadIcon(IdIdiStr::Id(IconId::Main as u16))
        .ok();
    let icon = icon_guard.as_deref();

    let _ = hwnd.ShellAbout(MSG_VERSION_NAME, None, Some(MSG_CREDIT), icon);
}

/// Display the Help dialog for the given command.
///
/// TODO: Refactor this function to only use the help dialog built into the resource file
/// # Arguments
/// * `w_command` - The help command (e.g., HELPONHELP).
/// * `l_param` - Additional parameter for the help command.
pub fn DoHelp(hwnd: &HWND, w_command: HELPW, l_param: u32) {
    // htmlhelp.dll expects either the localized .chm next to the EXE or the fallback NTHelp file.
    let mut buffer = [0u8; CCH_MAX_PATHNAME];

    if w_command == HELPW::HELPONHELP {
        const HELP_FILE: &[u8] = b"NTHelp.chm\0";
        buffer[..HELP_FILE.len()].copy_from_slice(HELP_FILE);
    } else {
        let exe_path = hwnd.hinstance().GetModuleFileName().unwrap_or_default();
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
    }

    unsafe {
        HtmlHelpA(hwnd.ptr(), buffer.as_ptr(), l_param, 0);
    }
}

/// Retrieve an integer value from a dialog item, clamping it within the specified bounds.
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `dlg_id` - The dialog item ID.
/// * `num_lo` - Minimum allowed value.
/// * `num_hi` - Maximum allowed value.
/// # Returns
/// The clamped integer value from the dialog item, or an error if retrieval or parsing fails.
pub fn GetDlgInt(
    h_dlg: &HWND,
    dlg_id: i32,
    num_lo: u32,
    num_hi: u32,
) -> Result<u32, Box<dyn core::error::Error + Send + Sync>> {
    h_dlg
        // Get a handle to the dialog item
        .GetDlgItem(dlg_id as u16)
        // Retrieve the integer value from the dialog item
        .and_then(|dlg| dlg.GetWindowText())
        // If there is an error, convert it into a form that can be propagated down the chain
        .map_err(Into::into)
        // Parse the retrieved text into a u32
        .and_then(|text| text.parse::<u32>().map_err(Into::into))
        // Clamp the parsed value within the specified bounds
        .map(|value| value.clamp(num_lo, num_hi))
}
