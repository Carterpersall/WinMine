use core::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Mutex, OnceLock};

use winsafe::guard::{DestroyIconGuard, DestroyMenuGuard};
use winsafe::prelude::Handle;
use winsafe::{HINSTANCE, HWND};

use crate::pref::CCH_NAME_MAX;

/// Aggregated status flags shared between modules.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum StatusFlag {
    Play = 0x01,
    Pause = 0x02,
    Icon = 0x08,
    Demo = 0x10,
}

/// True while the process starts minimized.
pub static INIT_MINIMIZED: AtomicBool = AtomicBool::new(false);

/// Tracks whether the left mouse button is currently held.
pub static LEFT_CLK_DOWN: AtomicBool = AtomicBool::new(false);

/// Tracks whether the UI should temporarily block button handling.
pub static BLK_BTN_INPUT: AtomicBool = AtomicBool::new(false);

/// Signals that the next click should be ignored (used after window activation).
pub static IGNORE_NEXT_CLICK: AtomicBool = AtomicBool::new(false);

/// Indicates that the app is paused because a menu is open.
pub static APP_PAUSED: AtomicBool = AtomicBool::new(false);

/// Cached system caption height used during window sizing.
pub static CYCAPTION: AtomicI32 = AtomicI32::new(0);

/// Cached system menu height used during window sizing.
pub static CYMENU: AtomicI32 = AtomicI32::new(0);

/// Cached system border width used during window sizing.
pub static CXBORDER: AtomicI32 = AtomicI32::new(0);

/// Current client width of the main window.
pub static WINDOW_WIDTH: AtomicI32 = AtomicI32::new(0);

/// Current client height of the main window.
pub static WINDOW_HEIGHT: AtomicI32 = AtomicI32::new(0);

/// Additional vertical adjustment applied during window sizing.
pub static WND_Y_OFFSET: AtomicI32 = AtomicI32::new(0);

/// Aggregated status flags shared between modules.
pub static GAME_STATUS: AtomicI32 =
    AtomicI32::new(StatusFlag::Icon as i32 | StatusFlag::Demo as i32);

/// Shared Win32 handles and string buffers used throughout the app.
pub struct GlobalState {
    pub h_inst: Mutex<HINSTANCE>,
    pub hwnd_main: Mutex<HWND>,
    pub h_menu: Mutex<Option<DestroyMenuGuard>>,
    pub h_icon_main: Mutex<Option<DestroyIconGuard>>,
    pub sz_class: Mutex<[u16; CCH_NAME_MAX]>,
    pub sz_time: Mutex<[u16; CCH_NAME_MAX]>,
    pub sz_default_name: Mutex<[u16; CCH_NAME_MAX]>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            h_inst: Mutex::new(HINSTANCE::NULL),
            hwnd_main: Mutex::new(HWND::NULL),
            h_menu: Mutex::new(None),
            h_icon_main: Mutex::new(None),
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
