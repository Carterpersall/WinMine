use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::System::WindowsProgramming::GetPrivateProfileIntW;
use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItemInt;

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
    CCH_NAME_MAX, DEFHEIGHT, DEFWIDTH, GameType, MINHEIGHT, MINWIDTH, MenuMode, PrefKey, ReadInt,
    SZ_WINMINE_REG_STR, SoundState, WritePreferences, pref_key_literal,
};
use crate::rtns::{AdjustFlag, preferences_mutex};
use crate::sound::FInitTunes;
use crate::winmine::{AdjustWindow, FixMenus, MenuCommand};

/// Multiplier used by the linear congruential generator that replicates WinMine's RNG.
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
    let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
    RNG_STATE.store(value, Ordering::Relaxed);
}

/// Generate the next pseudo-random number using a linear congruential generator.
/// # Returns
/// The next pseudo-random number.
///
/// TODO: Change return type to u32 since the current RNG value is stored as a u32
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
/// # Arguments
/// * `id_err` - The error ID used for selecting the error message.
pub fn ReportErr(err: &str) {
    /* let msg = match id_err {
        4 => ERR_TIMER,
        5 => ERR_OUT_OF_MEMORY,
        _ => &ERR_UNKNOWN_FMT.replace("%d", &id_err.to_string()),
    }; */

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
    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return val_default,
    };

    let ini_path = WString::from_str(SZ_INI_FILE);
    let value = unsafe {
        GetPrivateProfileIntW(
            GAME_NAME.encode_utf16().collect::<Vec<u16>>().as_ptr(),
            key.as_ptr(),
            val_default,
            ini_path.as_ptr(),
        ) as i32
    };
    value.clamp(val_min, val_max)
}

/// Read a string preference from the legacy .ini file into the provided buffer.
/// # Arguments
/// * `pref` - The preference key to read.
/// * `sz_ret` - Pointer to the buffer that receives the string (UTF-16).
fn ReadIniSz(pref: PrefKey, sz_ret: *mut u16) {
    // Grab the string from entpack.ini or fall back to the default Hall of Fame name.
    if sz_ret.is_null() {
        return;
    }

    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return,
    };

    let value = match w::GetPrivateProfileString(GAME_NAME, &key.to_string(), SZ_INI_FILE) {
        Ok(Some(text)) => text,
        _ => DEFAULT_PLAYER_NAME.to_string(),
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

/// Initialize UI globals, migrate preferences from the .ini file exactly once, and seed randomness.
pub fn InitConst() {
    let ticks = (GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    CYCAPTION.store(GetSystemMetrics(SM::CYCAPTION) + 1, Ordering::Relaxed);
    CYMENU.store(GetSystemMetrics(SM::CYMENU) + 1, Ordering::Relaxed);
    CXBORDER.store(GetSystemMetrics(SM::CXBORDER) + 1, Ordering::Relaxed);

    let mut already_played = false;

    if let Ok((key_guard, _)) = HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::READ,
        None,
    ) {
        already_played = ReadInt(&key_guard, PrefKey::AlreadyPlayed, 0, 0, 1) != 0;
    }

    if already_played {
        return;
    }

    let mut prefs = match preferences_mutex().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    prefs.Height = ReadIniInt(PrefKey::Height, MINHEIGHT, DEFHEIGHT, 25);
    prefs.Width = ReadIniInt(PrefKey::Width, MINWIDTH, DEFWIDTH, 30);
    let game_raw = ReadIniInt(
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
    prefs.Mines = ReadIniInt(PrefKey::Mines, 10, 10, 999);
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

    prefs.rgTime[GameType::Begin as usize] = ReadIniInt(PrefKey::Time1, 999, 0, 999);
    prefs.rgTime[GameType::Inter as usize] = ReadIniInt(PrefKey::Time2, 999, 0, 999);
    prefs.rgTime[GameType::Expert as usize] = ReadIniInt(PrefKey::Time3, 999, 0, 999);

    ReadIniSz(PrefKey::Name1, prefs.szBegin.as_mut_ptr());
    ReadIniSz(PrefKey::Name2, prefs.szInter.as_mut_ptr());
    ReadIniSz(PrefKey::Name3, prefs.szExpert.as_mut_ptr());

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
    {
        let mut prefs = match preferences_mutex().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        prefs.fColor = ReadIniInt(PrefKey::Color, default_color, 0, 1) != 0;

        if prefs.fSound == SoundState::On {
            prefs.fSound = FInitTunes();
        }
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
    let (menu_on, menu_checks);
    {
        let mut prefs = match preferences_mutex().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.fMenu = f_active;
        menu_on = !matches!(prefs.fMenu, MenuMode::Hidden);
        menu_checks = (prefs.wGameType, prefs.fColor, prefs.fMark, prefs.fSound);
    }

    FixMenus(
        &hwnd.GetMenu().unwrap_or(HMENU::NULL),
        menu_checks.0,
        menu_checks.1,
        menu_checks.2,
        menu_checks.3,
    );

    let menu = hwnd.GetMenu().unwrap_or(HMENU::NULL);
    let menu_arg = if menu_on { &menu } else { &HMENU::NULL };
    let _ = hwnd.SetMenu(menu_arg);
    AdjustWindow(hwnd, AdjustFlag::Resize as i32);
}

/// Display the About dialog box with version and credit information.
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
///
/// TODO: Change types to u32 since `GetDlgItemInt` returns u32
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `dlg_id` - The dialog item ID.
/// * `num_lo` - Minimum allowed value.
/// * `num_hi` - Maximum allowed value.
/// # Returns
/// The clamped integer value from the dialog item.
pub fn GetDlgInt(h_dlg: &HWND, dlg_id: i32, num_lo: i32, num_hi: i32) -> i32 {
    let mut success = 0i32;
    let value = unsafe { GetDlgItemInt(h_dlg.ptr(), dlg_id, &raw mut success, 0) };
    let value = value as i32;
    value.clamp(num_lo, num_hi)
}
