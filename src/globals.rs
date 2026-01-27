//! Global constants and variables used throughout the application.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32};

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

/// Default name for the best-times dialog.
pub const DEFAULT_PLAYER_NAME: &str = "Anonymous";

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

/* ---------------- */
/* Global Variables */
/* ---------------- */

/// Current DPI used for UI scaling.
///
/// This is kept in sync with the main window DPI (via `HWND::GetDpiForWindow` and
/// `WM_DPICHANGED`). All UI measurements that represent "logical" sizes from the
/// classic WinMine assets are scaled from 96 DPI to this value.
pub static UI_DPI: AtomicU32 = AtomicU32::new(BASE_DPI);

/// Tracks whether a drag operation is active.
pub static DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Signals that the next click should be ignored (used after window activation).
pub static IGNORE_NEXT_CLICK: AtomicBool = AtomicBool::new(false);

/// Current client width of the main window.
pub static WINDOW_WIDTH: AtomicI32 = AtomicI32::new(0);

/// Current client height of the main window.
pub static WINDOW_HEIGHT: AtomicI32 = AtomicI32::new(0);
