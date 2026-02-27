//! Global constants and variables used throughout the application.

/* -------------------- */
/* Constant Definitions */
/* -------------------- */

/// Base DPI used by Win32 when coordinates are expressed in 1:1 pixels.
pub(crate) const BASE_DPI: u32 = 96;

/* String Constants */

/// The name of the game.
///
/// This is used in the window title and as the window class name.
pub(crate) const GAME_NAME: &str = "Minesweeper";

/// Default name for the best-times dialog.
pub(crate) const DEFAULT_PLAYER_NAME: &str = "Anonymous";

/// Version string used in the About box.
pub(crate) const MSG_VERSION_NAME: &str = "Minesweeper";

/// Credit string used in the About box.
pub(crate) const MSG_CREDIT: &str = "by Robert Donner and Curt Johnson";
