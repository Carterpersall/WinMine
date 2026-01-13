use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::System::WindowsProgramming::GetPrivateProfileIntW;
use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItemInt;

use winsafe::{self as w, HWND, IdPos, WString, co, co::HELPW, co::SM, prelude::*};

use crate::globals::{CXBORDER, CYCAPTION, CYMENU, global_state};
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

/// Localized string resources.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum StringId {
    /// Window class name string.
    GameName = 1,
    /// "%d seconds" string used by the timer dialog.
    MsgSeconds = 7,
    /// Default high-score name.
    NameDefault = 8,
    /// Version string shown in About.
    MsgVersion = 12,
    /// Credit string shown in About.
    MsgCredit = 13,
    /// Generic error dialog title.
    ErrTitle = 3,
    /// Fallback "unknown error" template.
    ErrUnknown = 6,
}

/// Icon resources embedded in the executable.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum IconId {
    /// Main application icon.
    Main = 100,
}

/// Maximum resource ID treated as an error string; higher values use the unknown template.
pub const ID_ERR_MAX: u16 = 999;

/// Maximum length (UTF-16 code units) of dialog strings.
pub const CCH_MSG_MAX: usize = 128;
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

/// Return a pseudo-random number in the [0, rnd_max) range
/// # Arguments
/// * `rnd_max` - Upper bound (exclusive) for the random number
/// # Returns
/// A pseudo-random number in the [0, rnd_max) range
pub fn Rnd(rnd_max: i32) -> i32 {
    if rnd_max <= 0 {
        0
    } else {
        next_rand() % rnd_max
    }
}

/// Display an error message box for the specified error ID.
/// # Arguments
/// * `id_err` - The error string resource ID.
pub fn ReportErr(id_err: u16) {
    // Format either a catalog string or the "unknown error" template before showing the dialog.
    let state = global_state();
    let inst_guard = match state.h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let msg = if id_err < ID_ERR_MAX {
        inst_guard.LoadString(id_err).unwrap_or_default()
    } else {
        let template = inst_guard
            .LoadString(StringId::ErrUnknown as u16)
            .unwrap_or_default();
        template.replace("%d", &id_err.to_string())
    };

    let title = inst_guard
        .LoadString(StringId::ErrTitle as u16)
        .unwrap_or_default();
    let _ = w::HWND::NULL.MessageBox(&msg, &title, co::MB::ICONHAND);
}

/// Load a localized string resource into the provided buffer.
/// # Arguments
/// * `id` - The string resource ID to load.
/// * `cch` - Size of the buffer in UTF-16 code units.
/// # Returns
/// A `Result` containing the loaded string on success, or an error message on failure.
pub fn LoadSz(id: u16, max: usize) -> Result<String, Box<dyn std::error::Error>> {
    // 1. Acquire a lock on the global instance handle
    let inst_guard = match global_state().h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    // 2. Load the string resource
    let text = inst_guard
        .LoadString(id)
        .map_err(|e| format!("Failed to load string resource {}: {}", id, e))?;

    // 3. Validate that the string is not empty
    if text.is_empty() {
        return Err(format!("Empty string resource {}", id).into());
    }

    // 4. Truncate the string if it exceeds the specified maximum length and return it
    Ok(text[..max.min(text.len())].to_string())
}

/// Read an integer preference from the legacy .ini file, clamping it within the specified bounds.
/// # Arguments
/// * `pref` - The preference key to read.
/// * `val_default` - Default value if the key is missing or invalid.
/// * `val_min` - Minimum allowed value.
/// * `val_max` - Maximum allowed value.
/// # Returns
/// The clamped integer preference value.
pub fn ReadIniInt(pref: PrefKey, val_default: i32, val_min: i32, val_max: i32) -> i32 {
    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return val_default,
    };

    let class_str = match global_state().sz_class.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let ini_path = WString::from_str(SZ_INI_FILE);
    let value = unsafe {
        GetPrivateProfileIntW(
            class_str.encode_utf16().collect::<Vec<u16>>().as_ptr(),
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
pub fn ReadIniSz(pref: PrefKey, sz_ret: *mut u16) {
    // Grab the string from entpack.ini or fall back to the default Hall of Fame name.
    if sz_ret.is_null() {
        return;
    }

    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return,
    };

    let state = global_state();
    let section = {
        let guard = match state.sz_class.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    };
    let default_name = match state.sz_default_name.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let key_text = key.to_string();

    let value = match w::GetPrivateProfileString(section, &key_text, SZ_INI_FILE) {
        Ok(Some(text)) => text,
        _ => default_name.to_string(),
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
    let ticks = (w::GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    let state = global_state();
    match LoadSz(StringId::GameName as u16, CCH_NAME_MAX) {
        Ok(text) => {
            if let Ok(mut class_buf) = state.sz_class.lock() {
                // Leak the string to obtain a &'static str reference.
                // This is safe because sz_class is only read after this initialization.
                *class_buf = Box::leak(text.into_boxed_str());
            }
        }
        Err(e) => eprintln!("Failed to load game name string: {}", e),
    }

    match LoadSz(StringId::MsgSeconds as u16, CCH_NAME_MAX) {
        Ok(text) => {
            if let Ok(mut time_buf) = state.sz_time.lock() {
                // Leak the string to obtain a &'static str reference.
                // This is safe because sz_time is only read after this initialization.
                *time_buf = Box::leak(text.into_boxed_str());
            }
        }
        Err(e) => eprintln!("Failed to load time format string: {}", e),
    }

    match LoadSz(StringId::NameDefault as u16, CCH_NAME_MAX) {
        Ok(text) => {
            if let Ok(mut default_buf) = state.sz_default_name.lock() {
                // Leak the string to obtain a &'static str reference.
                // This is safe because sz_default_name is only read after this initialization.
                *default_buf = Box::leak(text.into_boxed_str());
            }
        }
        Err(e) => eprintln!("Failed to load default name string: {}", e),
    }

    CYCAPTION.store(w::GetSystemMetrics(SM::CYCAPTION) + 1, Ordering::Relaxed);
    CYMENU.store(w::GetSystemMetrics(SM::CYMENU) + 1, Ordering::Relaxed);
    CXBORDER.store(w::GetSystemMetrics(SM::CXBORDER) + 1, Ordering::Relaxed);

    let mut already_played = false;

    if let Ok((key_guard, _)) = w::HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        co::REG_OPTION::default(),
        co::KEY::READ,
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
        eprintln!("Failed to write preferences during initialization: {}", e);
    }
}

/// Check or uncheck a menu item based on the specified command ID.
/// # Arguments
/// * `idm` - The menu command ID.
/// * `f_check` - `true` to check the item, `false` to uncheck it.
pub fn CheckEm(idm: MenuCommand, f_check: bool) {
    // Maintain the old menu checkmark toggles (e.g. question marks, sound).
    let state = global_state();
    let menu_guard = match state.h_menu.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(menu) = menu_guard.as_ref() {
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

    FixMenus(menu_checks.0, menu_checks.1, menu_checks.2, menu_checks.3);

    let state = global_state();
    let menu_handle = {
        let guard = match state.h_menu.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .as_ref()
            .map(|menu| unsafe { w::HMENU::from_ptr(menu.ptr()) })
            .unwrap_or(w::HMENU::NULL)
    };

    if let Some(hwnd) = hwnd.as_opt() {
        let null_menu = w::HMENU::NULL;
        let menu_arg = if menu_on { &menu_handle } else { &null_menu };
        let _ = hwnd.SetMenu(menu_arg);
        AdjustWindow(hwnd, AdjustFlag::Resize as i32);
    }
}

/// Display the About dialog box with version and credit information.
/// # Arguments
/// * `hwnd` - Handle to the main window.
pub fn DoAbout(hwnd: &HWND) {
    let title = match LoadSz(StringId::MsgVersion as u16, CCH_MSG_MAX) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to load version string: {}", e);
            return;
        }
    };
    let credit = match LoadSz(StringId::MsgCredit as u16, CCH_MSG_MAX) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to load credit string: {}", e);
            return;
        }
    };
    let inst_guard = match global_state().h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let icon_guard = inst_guard
        .LoadIcon(w::IdIdiStr::Id(IconId::Main as u16))
        .ok();
    let icon = icon_guard.as_deref();

    let _ = hwnd.ShellAbout(&title, None, Some(&credit), icon);
}

/// Display the Help dialog for the given command.
/// # Arguments
/// * `w_command` - The help command (e.g., HELPONHELP).
/// * `l_param` - Additional parameter for the help command.
pub fn DoHelp(w_command: u16, l_param: u32) {
    // htmlhelp.dll expects either the localized .chm next to the EXE or the fallback NTHelp file.
    let mut buffer = [0u8; CCH_MAX_PATHNAME];
    let inst_guard = match global_state().h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if (w_command as u32) != HELPW::HELPONHELP.raw() {
        let exe_path = inst_guard.GetModuleFileName().unwrap_or_default();
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

/// Retrieve an integer value from a dialog item, clamping it within the specified bounds.
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `dlg_id` - The dialog item ID.
/// * `num_lo` - Minimum allowed value.
/// * `num_hi` - Maximum allowed value.
/// # Returns
/// The clamped integer value from the dialog item.
pub fn GetDlgInt(h_dlg: &w::HWND, dlg_id: i32, num_lo: i32, num_hi: i32) -> i32 {
    let mut success = 0i32;
    let value = unsafe { GetDlgItemInt(h_dlg.ptr(), dlg_id, &mut success, 0) };
    let value = value as i32;
    value.clamp(num_lo, num_hi)
}
