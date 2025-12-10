use core::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Mutex, OnceLock};

use winsafe::prelude::Handle;
use winsafe::{HICON, HINSTANCE, HMENU, HWND};

use crate::pref::CCH_NAME_MAX;

/// Bit flag indicating the window is currently minimized to an icon.
const F_ICON_BIT: i32 = 0x08;
/// Bit flag indicating the game is showing the demo/end-state.
const F_DEMO_BIT: i32 = 0x10;

/// True while the process starts minimized.
pub static bInitMinimized: AtomicBool = AtomicBool::new(false);

/// Tracks whether the left mouse button is currently held.
pub static fButton1Down: AtomicBool = AtomicBool::new(false);

/// Tracks whether the UI should temporarily block button handling.
pub static fBlock: AtomicBool = AtomicBool::new(false);

/// Signals that the next click should be ignored (used after window activation).
pub static fIgnoreClick: AtomicBool = AtomicBool::new(false);

/// Indicates that the app is paused because a menu is open.
pub static fLocalPause: AtomicBool = AtomicBool::new(false);

/// Cached system caption height used during window sizing.
pub static dypCaption: AtomicI32 = AtomicI32::new(0);

/// Cached system menu height used during window sizing.
pub static dypMenu: AtomicI32 = AtomicI32::new(0);

/// Cached system border height used during window sizing.
pub static dypBorder: AtomicI32 = AtomicI32::new(0);

/// Cached system border width used during window sizing.
pub static dxpBorder: AtomicI32 = AtomicI32::new(0);

/// Current client width of the main window.
pub static dxWindow: AtomicI32 = AtomicI32::new(0);

/// Current client height of the main window.
pub static dyWindow: AtomicI32 = AtomicI32::new(0);

/// Additional vertical adjustment applied during window sizing.
pub static dypAdjust: AtomicI32 = AtomicI32::new(0);

/// Additional horizontal frame adjustment applied during window sizing.
pub static dxFrameExtra: AtomicI32 = AtomicI32::new(0);

/// Aggregated status flags shared between modules.
pub static fStatus: AtomicI32 = AtomicI32::new(F_ICON_BIT | F_DEMO_BIT);

/// Shared Win32 handles and string buffers used throughout the app.
pub struct GlobalState {
    pub h_inst: Mutex<HINSTANCE>,
    pub hwnd_main: Mutex<HWND>,
    pub h_menu: Mutex<HMENU>,
    pub h_icon_main: Mutex<HICON>,
    pub sz_class: Mutex<[u16; CCH_NAME_MAX]>,
    pub sz_time: Mutex<[u16; CCH_NAME_MAX]>,
    pub sz_default_name: Mutex<[u16; CCH_NAME_MAX]>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            h_inst: Mutex::new(HINSTANCE::NULL),
            hwnd_main: Mutex::new(HWND::NULL),
            h_menu: Mutex::new(HMENU::NULL),
            h_icon_main: Mutex::new(HICON::NULL),
            sz_class: Mutex::new([0; CCH_NAME_MAX]),
            sz_time: Mutex::new([0; CCH_NAME_MAX]),
            sz_default_name: Mutex::new([0; CCH_NAME_MAX]),
        }
    }
}

static GLOBAL_STATE: OnceLock<GlobalState> = OnceLock::new();

pub fn global_state() -> &'static GlobalState {
    GLOBAL_STATE.get_or_init(GlobalState::default)
}
