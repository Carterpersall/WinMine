use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;
use windows_sys::Win32::System::WindowsProgramming::GetPrivateProfileIntW;
use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItemInt;

use winsafe::{self as w, IdPos, WString, co, co::HELPW, co::SM, prelude::*};

use crate::globals::{CXBORDER, CYCAPTION, CYMENU, global_state};
use crate::pref::{
    CCH_NAME_MAX, DEFHEIGHT, DEFWIDTH, GameType, MINHEIGHT, MINWIDTH, MenuMode, PrefKey, SoundState,
};
use crate::pref::{ReadInt, SZ_WINMINE_REG_STR, WritePreferences, pref_key_literal};
use crate::rtns::{AdjustFlag, preferences_mutex};
use crate::sound::FInitTunes;
use crate::winmine::{AdjustWindow, FixMenus, MenuCommand};

/// Multiplier used by the linear congruential generator that replicates WinMine's RNG.
const RNG_MULTIPLIER: u32 = 1_103_515_245;
/// Increment used by the linear congruential generator.
const RNG_INCREMENT: u32 = 12_345;
/// Default seed applied when the RNG would otherwise start at zero.
const RNG_DEFAULT_SEED: u32 = 0xACE1_1234;

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
const SZ_INI_FILE: &str = "entpack.ini";

fn seed_rng(seed: u32) {
    let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
    RNG_STATE.store(value, Ordering::Relaxed);
}

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

#[inline]
fn class_ptr() -> *const u16 {
    let state = global_state();
    let guard = match state.sz_class.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.as_ptr()
}

fn clamp(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
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

pub fn LoadSz(id: u16, sz: *mut u16, cch: u32) -> Result<(), Box<dyn std::error::Error>> {
    // Wrapper around LoadString that raises the original fatal error if the resource is missing.
    let state = global_state();
    let inst_guard = match state.h_inst.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let text = inst_guard
        .LoadString(id)
        .map_err(|e| format!("Failed to load string resource {}: {}", id, e))?;
    if text.is_empty() {
        return Err(format!("Empty string resource {}", id).into());
    }

    if sz.is_null() || cch == 0 {
        return Err("Invalid buffer parameters".into());
    }

    let max = cch as usize;
    let slice = unsafe { core::slice::from_raw_parts_mut(sz, max) };
    for (i, code_unit) in text.encode_utf16().chain(Some(0)).take(max).enumerate() {
        slice[i] = code_unit;
    }
    Ok(())
}

pub fn ReadIniInt(pref: PrefKey, val_default: i32, val_min: i32, val_max: i32) -> i32 {
    // Pull an integer from the legacy .ini file, honoring the same clamp the game always used.
    let key = match pref_key_literal(pref) {
        Some(name) => WString::from_str(name),
        None => return val_default,
    };

    let ini_path = WString::from_str(SZ_INI_FILE);
    let value = unsafe {
        GetPrivateProfileIntW(class_ptr(), key.as_ptr(), val_default, ini_path.as_ptr()) as i32
    };
    clamp(value, val_min, val_max)
}

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
    let class_buf = {
        let guard = match state.sz_class.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    };
    let default_buf = {
        let guard = match state.sz_default_name.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    };

    let section = utf16_buffer_to_string(&class_buf);
    let key_text = key.to_string();
    let default_name = utf16_buffer_to_string(&default_buf);

    let value = match w::GetPrivateProfileString(&section, &key_text, SZ_INI_FILE) {
        Ok(Some(text)) => text,
        _ => default_name,
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

pub fn InitConst() {
    // Initialize UI globals, migrate preferences from the .ini file exactly once, and seed randomness.
    let ticks = (w::GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    let state = global_state();
    if let Ok(mut class_buf) = state.sz_class.lock()
        && let Err(e) = LoadSz(
            StringId::GameName as u16,
            class_buf.as_mut_ptr(),
            CCH_NAME_MAX as u32,
        )
    {
        eprintln!("Failed to load game name string: {}", e);
    }
    if let Ok(mut time_buf) = state.sz_time.lock()
        && let Err(e) = LoadSz(
            StringId::MsgSeconds as u16,
            time_buf.as_mut_ptr(),
            CCH_NAME_MAX as u32,
        )
    {
        eprintln!("Failed to load time format string: {}", e);
    }
    if let Ok(mut default_buf) = state.sz_default_name.lock()
        && let Err(e) = LoadSz(
            StringId::NameDefault as u16,
            default_buf.as_mut_ptr(),
            CCH_NAME_MAX as u32,
        )
    {
        eprintln!("Failed to load default name string: {}", e);
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
        unsafe {
            already_played = ReadInt(&key_guard, PrefKey::AlreadyPlayed, 0, 0, 1) != 0;
        }
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
    prefs.fMark = bool_from_int(ReadIniInt(PrefKey::Mark, 1, 0, 1));
    prefs.fTick = bool_from_int(ReadIniInt(PrefKey::Tick, 0, 0, 1));
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

        prefs.fColor = bool_from_int(ReadIniInt(PrefKey::Color, default_color, 0, 1));

        if prefs.fSound == SoundState::On {
            prefs.fSound = FInitTunes();
        }
    }

    unsafe {
        if let Err(e) = WritePreferences() {
            eprintln!("Failed to write preferences during initialization: {}", e);
        }
    }
}

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

pub fn SetMenuBar(f_active: MenuMode) {
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
    let (menu_handle, hwnd_main) = {
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
        let hwnd_main = {
            let guard = match state.hwnd_main.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            unsafe { w::HWND::from_ptr(guard.ptr()) }
        };
        (menu_handle, hwnd_main)
    };

    if let Some(hwnd) = hwnd_main.as_opt() {
        let null_menu = w::HMENU::NULL;
        let menu_arg = if menu_on { &menu_handle } else { &null_menu };
        let _ = hwnd.SetMenu(menu_arg);
        AdjustWindow(AdjustFlag::Resize as i32);
    }
}

pub fn DoAbout() {
    // Show the stock About box with the localized title and credit strings.
    let hwnd_guard = {
        let state = global_state();
        match state.hwnd_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    };

    let hwnd = match hwnd_guard.as_opt() {
        Some(hwnd) => hwnd,
        None => return,
    };

    let mut sz_version = [0u16; CCH_MSG_MAX];
    let mut sz_credit = [0u16; CCH_MSG_MAX];

    if let Err(e) = LoadSz(
        StringId::MsgVersion as u16,
        sz_version.as_mut_ptr(),
        CCH_MSG_MAX as u32,
    ) {
        eprintln!("Failed to load version string: {}", e);
        return;
    }
    if let Err(e) = LoadSz(
        StringId::MsgCredit as u16,
        sz_credit.as_mut_ptr(),
        CCH_MSG_MAX as u32,
    ) {
        eprintln!("Failed to load credit string: {}", e);
        return;
    }

    let title = utf16_buffer_to_string(&sz_version);
    let credit = utf16_buffer_to_string(&sz_credit);
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

fn utf16_buffer_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub fn GetDlgInt(h_dlg: &w::HWND, dlg_id: i32, num_lo: i32, num_hi: i32) -> i32 {
    let mut success = 0i32;
    let value = unsafe { GetDlgItemInt(h_dlg.ptr(), dlg_id, &mut success, 0) };
    let value = value as i32;
    clamp(value, num_lo, num_hi)
}

fn bool_from_int(value: i32) -> bool {
    value != 0
}
