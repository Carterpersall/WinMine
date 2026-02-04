//! Global constants and variables used throughout the application.

use core::sync::atomic::AtomicI32;

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

/// Version string used in the About box.
pub const MSG_VERSION_NAME: &str = "Minesweeper";

/// Credit string used in the About box.
pub const MSG_CREDIT: &str = "by Robert Donner and Curt Johnson";

/* ---------------- */
/* Global Variables */
/* ---------------- */

/// Current client width of the main window.
pub static WINDOW_WIDTH: AtomicI32 = AtomicI32::new(0);

/// Current client height of the main window.
pub static WINDOW_HEIGHT: AtomicI32 = AtomicI32::new(0);
