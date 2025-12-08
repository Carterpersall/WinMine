// Registry-backed preference helpers mirrored from pref.c.
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicBool, Ordering};

use winsafe::{self as w, RegistryValue, co, guard::RegCloseKeyGuard, prelude::*};

use crate::globals::szDefaultName;
use crate::rtns::{Preferences, xBoxMac, yBoxMac};
use crate::sound::FInitTunes;

pub const CCH_NAME_MAX: usize = 32;
pub const ISZ_PREF_MAX: usize = 18;

pub const FSOUND_ON: i32 = 3;
pub const FSOUND_OFF: i32 = 2;

pub const MINHEIGHT: i32 = 9;
pub const DEFHEIGHT: i32 = 9;
pub const MINWIDTH: i32 = 9;
pub const DEFWIDTH: i32 = 9;

pub const WGAME_BEGIN: i32 = 0;
pub const WGAME_INTER: i32 = 1;
pub const WGAME_EXPERT: i32 = 2;

pub const FMENU_ALWAYS_ON: i32 = 0;
pub const FMENU_ON: i32 = 2;

pub const SZ_WINMINE_REG_STR: &str = "Software\\Microsoft\\winmine";

// Registry value names, ordered to match the legacy iszPref constants.
const PREF_STRINGS: [&str; ISZ_PREF_MAX] = [
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
    pub wGameType: u16,
    pub Mines: i32,
    pub Height: i32,
    pub Width: i32,
    pub xWindow: i32,
    pub yWindow: i32,
    pub fSound: i32,
    pub fMark: bool,
    pub fTick: bool,
    pub fMenu: i32,
    pub fColor: bool,
    pub rgTime: [i32; 3],
    pub szBegin: [u16; CCH_NAME_MAX],
    pub szInter: [u16; CCH_NAME_MAX],
    pub szExpert: [u16; CCH_NAME_MAX],
}

// Flag consulted by the C UI layer to decide when to persist settings.
pub static fUpdateIni: AtomicBool = AtomicBool::new(false);

pub static mut g_hReg: w::HKEY = w::HKEY::NULL;

pub unsafe fn ReadInt(isz_pref: i32, val_default: i32, val_min: i32, val_max: i32) -> i32 {
    // Registry integer fetch with clamping equivalent to the legacy ReadInt helper.
    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };

    if handle == w::HKEY::NULL {
        return val_default;
    }

    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return val_default,
    };

    let value = match handle.RegQueryValueEx(Some(&key_name)) {
        Ok(RegistryValue::Dword(val)) => val as i32,
        _ => return val_default,
    };

    clamp_i32(value, val_min, val_max)
}

pub unsafe fn ReadSz(isz_pref: i32, sz_ret: *mut u16) {
    // Pull a high-score name (or similar) from the hive, falling back to the default string.
    if sz_ret.is_null() {
        return;
    }

    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };

    if handle == w::HKEY::NULL {
        unsafe {
            copy_default_name(sz_ret);
        }
        return;
    }

    let key_name = match pref_name_string(isz_pref) {
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

    unsafe {
        g_hReg = key_guard.leak();
    }

    let prefs = addr_of_mut!(Preferences);
    unsafe {
        let height = ReadInt(2, MINHEIGHT, DEFHEIGHT, 25);
        yBoxMac.store(height, Ordering::Relaxed);
        (*prefs).Height = height;

        let width = ReadInt(3, MINWIDTH, DEFWIDTH, 30);
        xBoxMac.store(width, Ordering::Relaxed);
        (*prefs).Width = width;

        (*prefs).wGameType = ReadInt(0, WGAME_BEGIN, WGAME_BEGIN, WGAME_EXPERT + 1) as u16;
        (*prefs).Mines = ReadInt(1, 10, 10, 999);
        (*prefs).xWindow = ReadInt(4, 80, 0, 1024);
        (*prefs).yWindow = ReadInt(5, 80, 0, 1024);

        (*prefs).fSound = ReadInt(6, 0, 0, FSOUND_ON);
        (*prefs).fMark = ReadInt(7, 1, 0, 1) != 0;
        (*prefs).fTick = ReadInt(9, 0, 0, 1) != 0;
        (*prefs).fMenu = ReadInt(8, FMENU_ALWAYS_ON, FMENU_ALWAYS_ON, FMENU_ON);

        (*prefs).rgTime[WGAME_BEGIN as usize] = ReadInt(11, 999, 0, 999);
        (*prefs).rgTime[WGAME_INTER as usize] = ReadInt(13, 999, 0, 999);
        (*prefs).rgTime[WGAME_EXPERT as usize] = ReadInt(15, 999, 0, 999);

        ReadSz(12, addr_of_mut!((*prefs).szBegin[0]));
        ReadSz(14, addr_of_mut!((*prefs).szInter[0]));
        ReadSz(16, addr_of_mut!((*prefs).szExpert[0]));
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
    unsafe {
        (*prefs).fColor = ReadInt(10, default_color, 0, 1) != 0;
    }

    // If sound is enabled, verify that the system can actually play the resources.
    unsafe {
        if (*prefs).fSound == FSOUND_ON {
            (*prefs).fSound = FInitTunes();
        }
    }

    unsafe {
        close_registry_handle();
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

    unsafe {
        g_hReg = key_guard.leak();
    }

    let prefs = addr_of!(Preferences);

    // Persist the difficulty, board dimensions, and flags exactly as the original did.
    unsafe {
        WriteInt(0, (*prefs).wGameType as i32);
        WriteInt(2, (*prefs).Height);
        WriteInt(3, (*prefs).Width);
        WriteInt(1, (*prefs).Mines);
        WriteInt(7, bool_to_i32((*prefs).fMark));
        WriteInt(17, 1);

        WriteInt(10, bool_to_i32((*prefs).fColor));
        WriteInt(6, (*prefs).fSound);
        WriteInt(4, (*prefs).xWindow);
        WriteInt(5, (*prefs).yWindow);

        WriteInt(11, (*prefs).rgTime[WGAME_BEGIN as usize]);
        WriteInt(13, (*prefs).rgTime[WGAME_INTER as usize]);
        WriteInt(15, (*prefs).rgTime[WGAME_EXPERT as usize]);

        WriteSz(12, addr_of!((*prefs).szBegin[0]));
        WriteSz(14, addr_of!((*prefs).szInter[0]));
        WriteSz(16, addr_of!((*prefs).szExpert[0]));
    }

    unsafe {
        close_registry_handle();
    }
}

pub unsafe fn WriteInt(isz_pref: i32, val: i32) {
    // Simple DWORD setter used by both the registry migration and the dialog code.
    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };

    if handle == w::HKEY::NULL {
        return;
    }

    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };
    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Dword(val as u32));
}

pub unsafe fn WriteSz(isz_pref: i32, sz: *const u16) {
    // Stores zero-terminated UTF-16 values such as player names.
    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };

    if handle == w::HKEY::NULL || sz.is_null() {
        return;
    }

    let key_name = match pref_name_string(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let value = match unsafe { wide_ptr_to_string(sz) } {
        Some(text) => text,
        None => return,
    };

    let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };
    let _ = handle.RegSetValueEx(Some(&key_name), RegistryValue::Sz(value));
}

pub(crate) fn pref_key_literal(index: i32) -> Option<&'static str> {
    if index < 0 {
        return None;
    }
    PREF_STRINGS.get(index as usize).copied()
}

fn pref_name_string(index: i32) -> Option<String> {
    pref_key_literal(index).map(|s| s.to_string())
}

fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn bool_to_i32(flag: bool) -> i32 {
    if flag { 1 } else { 0 }
}

unsafe fn default_name_ptr() -> *const u16 {
    unsafe { addr_of!(szDefaultName[0]) }
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
        let mut i = 0usize;
        while i + 1 < capacity {
            let ch = *src.add(i);
            *dst.add(i) = ch;
            if ch == 0 {
                return;
            }
            i += 1;
        }

        *dst.add(capacity - 1) = 0;
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
    unsafe {
        copy_wide_with_capacity(default_name_ptr(), dst, CCH_NAME_MAX);
    }
}

pub(crate) unsafe fn close_registry_handle() {
    if unsafe { g_hReg != w::HKEY::NULL } {
        let handle = unsafe { core::ptr::read(addr_of!(g_hReg)) };
        unsafe {
            g_hReg = w::HKEY::NULL;
        }
        unsafe {
            let _ = RegCloseKeyGuard::new(handle);
        }
    }
}
