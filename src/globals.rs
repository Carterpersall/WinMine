use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use winsafe::{self as w, GetSystemMetrics, GetSystemMetricsForDpi};

/* -------------------- */
/* Constant Definitions */
/* -------------------- */

/// Base DPI used by Win32 when coordinates are expressed in 1:1 pixels.
pub const BASE_DPI: u32 = 96;

/* String Constants */

/// The name of the game.
///
/// This is used in the window title and as the window class name.
pub const GAME_NAME: &str = "Minesweeper";

/// Dialog title for error message boxes.
pub const ERR_TITLE: &str = "Minesweeper Error";

/// Error message given when allocating a timer fails.
pub const ERR_TIMER: &str =
    "Unable to allocate a timer.  Please exit some of your applications and try again.";

/// Out-of-memory error message.
pub const ERR_OUT_OF_MEMORY: &str = "Out of Memory";

/// Default name for the best-times dialog.
pub const DEFAULT_PLAYER_NAME: &str = "Anonymous";

/// Time display formatting.
pub const TIME_FORMAT: &str = "%d seconds";

/// Prompt for fastest beginner time.
pub const MSG_FASTEST_BEGINNER: &str =
    "You have the fastest time\rfor beginner level.\rPlease enter your name.";

/// Prompt for fastest intermediate time.
pub const MSG_FASTEST_INTERMEDIATE: &str =
    "You have the fastest time\rfor intermediate level.\rPlease enter your name.";

/// Prompt for fastest expert time.
pub const MSG_FASTEST_EXPERT: &str =
    "You have the fastest time\rfor expert level.\rPlease enter your name.";

/// Version string used in the About box.
pub const MSG_VERSION_NAME: &str = "Minesweeper";

/// Credit string used in the About box.
pub const MSG_CREDIT: &str = "by Robert Donner and Curt Johnson";

/// Flags defining the current game status.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum StatusFlag {
    /// Game is currently being played.
    Play = 0x01,
    /// Game is currently paused.
    Pause = 0x02,
    /// Game is minimized.
    Icon = 0x08,
    /// Demo mode is active.
    Demo = 0x10,
}

/* ---------------- */
/* Global Variables */
/* ---------------- */

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

    let caption = GetSystemMetricsForDpi(w::co::SM::CYCAPTION, dpi)
        .unwrap_or_else(|_| GetSystemMetrics(w::co::SM::CYCAPTION));
    let menu = GetSystemMetricsForDpi(w::co::SM::CYMENU, dpi)
        .unwrap_or_else(|_| GetSystemMetrics(w::co::SM::CYMENU));
    let border = GetSystemMetricsForDpi(w::co::SM::CXBORDER, dpi)
        .unwrap_or_else(|_| GetSystemMetrics(w::co::SM::CXBORDER));
    // Preserve the historical +1 fudge used throughout the codebase.
    // TODO: Why is this done?
    // TODO: Rename these so that they don't conflict with the `SM_` constants.
    CYCAPTION.store(caption + 1, Ordering::Relaxed);
    CYMENU.store(menu + 1, Ordering::Relaxed);
    CXBORDER.store(border + 1, Ordering::Relaxed);
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
