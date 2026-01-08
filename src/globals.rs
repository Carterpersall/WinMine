use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use winsafe::guard::{DestroyIconGuard, DestroyMenuGuard};
use winsafe::prelude::Handle;
use winsafe::{self as w, HINSTANCE};

use crate::pref::CCH_NAME_MAX;

/// Base DPI used by Win32 when coordinates are expressed in 1:1 pixels.
pub const BASE_DPI: u32 = 96;

/// Current DPI used for UI scaling.
///
/// This is kept in sync with the main window DPI (via `HWND::GetDpiForWindow` and
/// `WM_DPICHANGED`). All UI measurements that represent "logical" sizes from the
/// classic WinMine assets are scaled from 96 DPI to this value.
pub static UI_DPI: AtomicU32 = AtomicU32::new(BASE_DPI);

/// Update cached system metrics which vary with DPI.
///
/// These values are consumed during window sizing in order to keep the non-client
/// area calculations stable under Per-Monitor DPI.
/// # Arguments
/// * `dpi`: The new DPI to use. If zero, `BASE_DPI` is used
pub fn update_ui_metrics_for_dpi(dpi: u32) {
    let dpi = if dpi == 0 { BASE_DPI } else { dpi };

    let caption = w::GetSystemMetricsForDpi(w::co::SM::CYCAPTION, dpi)
        .unwrap_or_else(|_| w::GetSystemMetrics(w::co::SM::CYCAPTION));
    let menu = w::GetSystemMetricsForDpi(w::co::SM::CYMENU, dpi)
        .unwrap_or_else(|_| w::GetSystemMetrics(w::co::SM::CYMENU));
    let border = w::GetSystemMetricsForDpi(w::co::SM::CXBORDER, dpi)
        .unwrap_or_else(|_| w::GetSystemMetrics(w::co::SM::CXBORDER));

    // Preserve the historical +1 fudge used throughout the codebase.
    // TODO: Why is this done?
    CYCAPTION.store(caption + 1, Ordering::Relaxed);
    CYMENU.store(menu + 1, Ordering::Relaxed);
    CXBORDER.store(border + 1, Ordering::Relaxed);
}

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
            h_menu: Mutex::new(None),
            h_icon_main: Mutex::new(None),
            sz_class: Mutex::new([0; CCH_NAME_MAX]),
            sz_time: Mutex::new([0; CCH_NAME_MAX]),
            sz_default_name: Mutex::new([0; CCH_NAME_MAX]),
        }
    }
}

/// Shared variable containing the global state
static GLOBAL_STATE: OnceLock<GlobalState> = OnceLock::new();

/// Accessor for the shared global state
/// # Returns
/// A reference to the global state singleton
pub fn global_state() -> &'static GlobalState {
    GLOBAL_STATE.get_or_init(GlobalState::default)
}
