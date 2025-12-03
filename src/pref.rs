// Registry-backed preference helpers mirrored from pref.c.
use core::ptr::{addr_of, addr_of_mut, null_mut};
use core::sync::atomic::{AtomicBool, Ordering};

use windows_sys::core::{w, BOOL, PCWSTR};
use windows_sys::Win32::Foundation::{FALSE, HWND, TRUE};
use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, NUMCOLORS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_WRITE, REG_DWORD, REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

use crate::globals::szDefaultName;
use crate::rtns::{xBoxMac, yBoxMac, Preferences};
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

pub const SZ_WINMINE_REG: PCWSTR = w!("Software\\Microsoft\\winmine");

// Registry value names, ordered to match the legacy iszPref constants.
const PREF_STRINGS: [PCWSTR; ISZ_PREF_MAX] = [
    w!("Difficulty"),
    w!("Mines"),
    w!("Height"),
    w!("Width"),
    w!("Xpos"),
    w!("Ypos"),
    w!("Sound"),
    w!("Mark"),
    w!("Menu"),
    w!("Tick"),
    w!("Color"),
    w!("Time1"),
    w!("Name1"),
    w!("Time2"),
    w!("Name2"),
    w!("Time3"),
    w!("Name3"),
    w!("AlreadyPlayed"),
];

// Rust mirror of the PREF struct so we can mutate the shared C globals.
#[repr(C)]
pub struct Pref {
    pub wGameType: u16,
    pub Mines: i32,
    pub Height: i32,
    pub Width: i32,
    pub xWindow: i32,
    pub yWindow: i32,
    pub fSound: i32,
    pub fMark: BOOL,
    pub fTick: BOOL,
    pub fMenu: i32,
    pub fColor: BOOL,
    pub rgTime: [i32; 3],
    pub szBegin: [u16; CCH_NAME_MAX],
    pub szInter: [u16; CCH_NAME_MAX],
    pub szExpert: [u16; CCH_NAME_MAX],
}

// Flag consulted by the C UI layer to decide when to persist settings.
pub static fUpdateIni: AtomicBool = AtomicBool::new(false);

pub static mut g_hReg: HKEY = std::ptr::null_mut();

pub static mut rgszPref: [PCWSTR; ISZ_PREF_MAX] = PREF_STRINGS;
pub unsafe fn ReadInt(
    isz_pref: i32,
    val_default: i32,
    val_min: i32,
    val_max: i32,
) -> i32 {
    // Registry integer fetch with clamping equivalent to the legacy ReadInt helper.
    let handle = g_hReg;
    if handle.is_null() {
        return val_default;
    }

    let key_name = match pref_name(isz_pref) {
        Some(name) => name,
        None => return val_default,
    };

    let mut value: u32 = 0;
    let mut size = core::mem::size_of::<u32>() as u32;
    let status = RegQueryValueExW(
        handle,
        key_name,
        null_mut(),
        null_mut(),
        &mut value as *mut u32 as *mut u8,
        &mut size,
    );

    if status != 0 {
        return val_default;
    }

    clamp_i32(value as i32, val_min, val_max)
}

pub unsafe fn ReadSz(isz_pref: i32, sz_ret: *mut u16) {
    // Pull a high-score name (or similar) from the hive, falling back to the default string.
    if sz_ret.is_null() {
        return;
    }

    let handle = g_hReg;
    if handle.is_null() {
        copy_wide_with_capacity(default_name_ptr(), sz_ret, CCH_NAME_MAX);
        return;
    }

    let key_name = match pref_name(isz_pref) {
        Some(name) => name,
        None => {
            copy_wide_with_capacity(default_name_ptr(), sz_ret, CCH_NAME_MAX);
            return;
        }
    };

    let mut size = (CCH_NAME_MAX * core::mem::size_of::<u16>()) as u32;
    let status = RegQueryValueExW(
        handle,
        key_name,
        null_mut(),
        null_mut(),
        sz_ret as *mut u8,
        &mut size,
    );

    if status != 0 {
        copy_wide_with_capacity(default_name_ptr(), sz_ret, CCH_NAME_MAX);
    }
}

pub unsafe fn ReadPreferences() {
    // Fetch persisted dimensions, timers, and feature flags from the WinMine registry hive.
    let mut disposition = 0u32;
    let mut key: HKEY = std::ptr::null_mut();

    // Open (or create) the WinMine registry hive; if it fails we keep defaults.
    if RegCreateKeyExW(
        HKEY_CURRENT_USER,
        SZ_WINMINE_REG,
        0,
        null_mut(),
        0,
        KEY_READ,
        null_mut(),
        &mut key,
        &mut disposition,
    ) != 0
    {
        return;
    }

    g_hReg = key;

    let prefs = addr_of_mut!(Preferences);

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
    (*prefs).fMark = bool_to_bool(ReadInt(7, TRUE, 0, 1));
    (*prefs).fTick = bool_to_bool(ReadInt(9, FALSE, 0, 1));
    (*prefs).fMenu = ReadInt(8, FMENU_ALWAYS_ON, FMENU_ALWAYS_ON, FMENU_ON);

    (*prefs).rgTime[WGAME_BEGIN as usize] = ReadInt(11, 999, 0, 999);
    (*prefs).rgTime[WGAME_INTER as usize] = ReadInt(13, 999, 0, 999);
    (*prefs).rgTime[WGAME_EXPERT as usize] = ReadInt(15, 999, 0, 999);

    ReadSz(12, addr_of_mut!((*prefs).szBegin[0]));
    ReadSz(14, addr_of_mut!((*prefs).szInter[0]));
    ReadSz(16, addr_of_mut!((*prefs).szExpert[0]));

    // Determine whether to favor color assets (NUMCOLORS may return -1 on true color displays).
    let desktop: HWND = GetDesktopWindow();
    let hdc = GetDC(desktop);
    let default_color = if !hdc.is_null() && GetDeviceCaps(hdc, NUMCOLORS as i32) != 2 {
        TRUE
    } else {
        FALSE
    };
    if !hdc.is_null() {
        ReleaseDC(desktop, hdc);
    }
    (*prefs).fColor = bool_to_bool(ReadInt(10, default_color, 0, 1));

    // If sound is enabled, verify that the system can actually play the resources.
    if (*prefs).fSound == FSOUND_ON {
        (*prefs).fSound = FInitTunes();
    }

    RegCloseKey(g_hReg);
    g_hReg = std::ptr::null_mut();
}

pub unsafe fn WritePreferences() {
    // Persist the current PREF struct back to the registry, mirroring the Win32 version.
    let mut disposition = 0u32;
    let mut key: HKEY = std::ptr::null_mut();

    // Try to reopen the hive with write access; on failure we leave preferences untouched.
    if RegCreateKeyExW(
        HKEY_CURRENT_USER,
        SZ_WINMINE_REG,
        0,
        null_mut(),
        0,
        KEY_WRITE,
        null_mut(),
        &mut key,
        &mut disposition,
    ) != 0
    {
        return;
    }

    g_hReg = key;

    let prefs = addr_of!(Preferences);

    // Persist the difficulty, board dimensions, and flags exactly as the original did.
    WriteInt(0, (*prefs).wGameType as i32);
    WriteInt(2, (*prefs).Height);
    WriteInt(3, (*prefs).Width);
    WriteInt(1, (*prefs).Mines);
    WriteInt(7, (*prefs).fMark);
    WriteInt(17, 1);

    WriteInt(10, (*prefs).fColor);
    WriteInt(6, (*prefs).fSound);
    WriteInt(4, (*prefs).xWindow);
    WriteInt(5, (*prefs).yWindow);

    WriteInt(11, (*prefs).rgTime[WGAME_BEGIN as usize]);
    WriteInt(13, (*prefs).rgTime[WGAME_INTER as usize]);
    WriteInt(15, (*prefs).rgTime[WGAME_EXPERT as usize]);

    WriteSz(12, addr_of!((*prefs).szBegin[0]));
    WriteSz(14, addr_of!((*prefs).szInter[0]));
    WriteSz(16, addr_of!((*prefs).szExpert[0]));

    RegCloseKey(g_hReg);
    g_hReg = std::ptr::null_mut();
}

pub unsafe fn WriteInt(isz_pref: i32, val: i32) {
    // Simple DWORD setter used by both the registry migration and the dialog code.
    let handle = g_hReg;
    if handle.is_null() {
        return;
    }

    let key_name = match pref_name(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let data = val as u32;
    RegSetValueExW(
        handle,
        key_name,
        0,
        REG_DWORD,
        &data as *const u32 as *const u8,
        core::mem::size_of::<u32>() as u32,
    );
}

pub unsafe fn WriteSz(isz_pref: i32, sz: *const u16) {
    // Stores zero-terminated UTF-16 values such as player names.
    let handle = g_hReg;
    if handle.is_null() || sz.is_null() {
        return;
    }

    let key_name = match pref_name(isz_pref) {
        Some(name) => name,
        None => return,
    };

    let len = wide_len(sz) + 1;
    let byte_len = len * core::mem::size_of::<u16>();

    RegSetValueExW(
        handle,
        key_name,
        0,
        REG_SZ,
        sz as *const u8,
        byte_len as u32,
    );
}

fn pref_name(index: i32) -> Option<PCWSTR> {
    if index < 0 {
        return None;
    }
    let idx = index as usize;
    PREF_STRINGS.get(idx).copied()
}

fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.max(min).min(max)
}

fn bool_to_bool(value: i32) -> BOOL {
    if value != 0 {
        TRUE
    } else {
        FALSE
    }
}

unsafe fn default_name_ptr() -> *const u16 {
    addr_of!(szDefaultName[0])
}

unsafe fn copy_wide_with_capacity(src: *const u16, dst: *mut u16, capacity: usize) {
    if src.is_null() || dst.is_null() || capacity == 0 {
        return;
    }

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

unsafe fn wide_len(mut ptr: *const u16) -> usize {
    if ptr.is_null() {
        return 0;
    }
    let mut len = 0usize;
    while *ptr != 0 {
        len += 1;
        ptr = ptr.add(1);
    }
    len
}
