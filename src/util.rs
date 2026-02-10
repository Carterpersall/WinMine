//! Utility functions and helpers used across the application.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use winsafe::co::{KEY, REG_OPTION};
use winsafe::{AnyResult, GetTickCount64, HKEY, HMENU, HWND, IdPos, LOWORD, prelude::*};

use crate::globals::BASE_DPI;
use crate::pref::{GameType, SZ_WINMINE_REG_STR};
use crate::winmine::WinMineMainWindow;

#[derive(Copy, Clone)]
pub enum ResourceId {
    /// Value representing no resource, used as a sentinel in arrays of resource IDs.
    None = 0,

    /* MINE Resources */
    /// Main application icon.
    Icon = 100,
    /// Bitmap resource for colored blocks.
    BlocksBmp = 410,
    /// Bitmap resource for black-and-white blocks.
    BWBlocksBmp = 411,
    /// Bitmap resource for colored LED display.
    LedBmp = 420,
    /// Bitmap resource for black-and-white LED display.
    BWLedBmp = 421,
    /// Bitmap resource for colored buttons.
    ButtonBmp = 430,
    /// Bitmap resource for black-and-white buttons.
    BWButtonBmp = 431,
    /// Sound resource for tick sound.
    TuneTick = 432,
    /// Sound resource for winning sound.
    TuneWon = 433,
    /// Sound resource for losing sound.
    TuneLost = 434,

    /* Preferences Dialog */
    /// Preferences dialog identifier.
    PrefDlg = 80,
    /// OK button in preferences dialog.
    OkBtn = 101,
    /// Cancel button in preferences dialog.
    CancelBtn = 109,
    /// Text label for mines in preferences dialog.
    MinesText = 111,
    /// Text label for height in preferences dialog.
    HeightText = 112,
    /// Text label for width in preferences dialog.
    WidthText = 113,
    /// Edit control for board height.
    HeightEdit = 141,
    /// Edit control for board width.
    WidthEdit = 142,
    /// Edit control for number of mines.
    MinesEdit = 143,
    /// Text label for custom settings.
    CustomText = 151,

    /* Enter Name Dialog */
    /// Enter name dialog identifier.
    EnterDlg = 600,
    /// Best times text label.
    BestText = 601,
    /// Edit control for player name.
    NameEdit = 602,

    /* Best Times Dialog */
    /// Best times dialog identifier.
    BestDlg = 700,
    /// Time display for beginner level.
    BeginTime = 701,
    /// Name display for beginner level.
    BeginName = 702,
    /// Time display for intermediate level.
    InterTime = 703,
    /// Name display for intermediate level.
    InterName = 704,
    /// Time display for expert level.
    ExpertTime = 705,
    /// Name display for expert level.
    ExpertName = 706,
    /// Reset best times button.
    ResetBtn = 707,
    /// Static text control 1.
    SText1 = 708,
    /// Static text control 2.
    SText2 = 709,
    /// Static text control 3.
    SText3 = 710,

    /* Menus */
    /// Main menu identifier.
    Menu = 500,
    /// Menu accelerator table.
    MenuAccel = 501,

    /// New game menu item.
    NewGame = 510,
    /// Exit menu item.
    Exit = 512,

    /// Skill level submenu.
    SkillSubmenu = 520,
    /// Beginner level menu item.
    Begin = 521,
    /// Intermediate level menu item.
    Inter = 522,
    /// Expert level menu item.
    Expert = 523,
    /// Custom level menu item.
    Custom = 524,
    /// Sound toggle menu item.
    Sound = 526,
    /// Marking toggle menu item.
    Mark = 527,
    /// Best times menu item.
    Best = 528,
    /// Color toggle menu item.
    Color = 529,

    /// Help submenu.
    HelpSubmenu = 590,
    /// "How to play" menu item.
    HowToPlay = 591,
    /// "Help on Help" menu item.
    HelpOnHelp = 592,
    /// About dialog menu item.
    About = 593,

    /* Context Sensitive Help */
    /// Help context ID for height edit control in preferences dialog.
    PrefEditHeight = 1000,
    /// Help context ID for width edit control in preferences dialog.
    PrefEditWidth = 1001,
    /// Help context ID for mines edit control in preferences dialog.
    PrefEditMines = 1002,
    /// Help context ID for reset button in best times dialog.
    BestBtnReset = 1003,
    /// Help context ID for static text controls.
    SText = 1004,
}

impl From<ResourceId> for u16 {
    fn from(res_id: ResourceId) -> Self {
        res_id as u16
    }
}

/// A wrapper around `RwLock` that handles poisoning by returning the inner data.
pub struct StateLock<T>(RwLock<T>);

impl<T> StateLock<T> {
    /// Create a new `StateLock` wrapping the given value.
    /// # Arguments
    /// * `value` - The value to wrap in the `RwLock`.
    pub const fn new(value: T) -> Self {
        Self(RwLock::new(value))
    }

    /// Get a read lock on the inner value, handling poisoning.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        match self.0.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Get a write lock on the inner value, handling poisoning.
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        match self.0.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Shared state of the linear congruential generator.
static RNG_STATE: AtomicU32 = AtomicU32::new(0);

/// Seed the RNG with the specified seed value.
/// # Arguments
/// * `seed` - The seed value to initialize the RNG with.
/// # Notes
/// This function replicates the functionality of the C standard library's `srand()` function.
fn seed_rng(seed: u16) {
    // Initialize the shared RNG state to the given seed value
    RNG_STATE.store(seed as u32, Ordering::Relaxed);
}

/// Generate the next pseudo-random number using a linear congruential generator.
/// # Returns
/// The next pseudo-random number.
/// # Notes
/// A linear congruential generator (LCG) is a simple algorithm for generating a sequence of pseudo-random numbers.
///
/// The formula used is:
///
/// X<sub>{n+1}</sub> = (a * X<sub>n</sub> + c) mod m
///
/// Where:
/// - X is the sequence of pseudo-random values
/// - a is the multiplier (`RNG_MULTIPLIER`)
/// - c is the increment (`RNG_INCREMENT`)
/// - m is the modulus (2<sup>32</sup> for `u32` arithmetic)
///
/// This formula is the same used in Windows' `rand()` function.
///
/// TODO: Consider using using Rust's built-in RNG facilities
fn rand() -> u32 {
    /// Multiplier used by the linear congruential generator that produces the app's RNG values.
    const RNG_MULTIPLIER: u32 = 214_013;
    /// Increment used by the linear congruential generator.
    const RNG_INCREMENT: u32 = 2_531_011;

    let mut current = RNG_STATE.load(Ordering::Relaxed);
    loop {
        // Compute the next RNG state using LCG formula
        let next = current
            .wrapping_mul(RNG_MULTIPLIER)
            .wrapping_add(RNG_INCREMENT);
        match RNG_STATE.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (next >> 16) & 0x7FFF,
            Err(actual) => current = actual,
        }
    }
}

/// Return a pseudo-random number in the [0, `rnd_max`) range
/// # Arguments
/// * `rnd_max` - Upper bound (exclusive) for the random number
/// # Returns
/// A pseudo-random number in the [0, `rnd_max`) range
pub fn rnd(rnd_max: u32) -> u32 {
    rand() % rnd_max
}

/// Initialize UI globals and seed the RNG state.
///
/// TODO: Does this function need to exist? It is only called once during startup.
pub fn init_const() {
    // Seed the RNG using the low 16 bits of the current tick count
    let ticks = LOWORD(GetTickCount64() as u32);
    seed_rng(ticks);

    // Create or open the registry key for storing preferences
    // TODO: Handle errors
    // TODO: Now that the ini migration code is gone, what happens when there are no existing preferences? Does the AlreadyPlayed flag need to exist anymore?
    let _ = HKEY::CURRENT_USER.RegCreateKeyEx(
        SZ_WINMINE_REG_STR,
        None,
        REG_OPTION::default(),
        KEY::READ,
        None,
    );
}

/// Check or uncheck a menu item based on the specified command ID.
///
/// TODO: This function no longer needs to exist
/// # Arguments
/// * `idm` - The menu command ID.
/// * `f_check` - `true` to check the item, `false` to uncheck it.
/// # Returns
/// An `Ok(())` if successful, or an error if checking/unchecking failed.
pub fn menu_check(hmenu: &HMENU, idm: ResourceId, f_check: bool) -> AnyResult<()> {
    if let Some(menu) = hmenu.as_opt() {
        menu.CheckMenuItem(IdPos::Id(idm as u16), f_check)?;
    }
    Ok(())
}

impl WinMineMainWindow {
    /// Update the menu bar to reflect current preferences.
    /// # Returns
    /// An `Ok(())` if successful, or an error if updating the menu bar failed.
    pub fn set_menu_bar(&self) -> AnyResult<()> {
        // Persist the menu visibility preference, refresh accelerator state, and resize the window.
        let (game_type, color, mark, sound) = {
            let state = self.state.read();
            (
                state.prefs.game_type,
                state.prefs.color,
                state.prefs.mark_enabled,
                state.prefs.sound_enabled,
            )
        };

        // Update the menu checkmarks to reflect the current preferences
        let hmenu = self.wnd.hwnd().GetMenu().unwrap_or(HMENU::NULL);
        menu_check(&hmenu, ResourceId::Begin, game_type == GameType::Begin)?;
        menu_check(&hmenu, ResourceId::Inter, game_type == GameType::Inter)?;
        menu_check(&hmenu, ResourceId::Expert, game_type == GameType::Expert)?;
        menu_check(&hmenu, ResourceId::Custom, game_type == GameType::Other)?;

        menu_check(&hmenu, ResourceId::Color, color)?;
        menu_check(&hmenu, ResourceId::Mark, mark)?;
        menu_check(&hmenu, ResourceId::Sound, sound)?;

        Ok(())
    }
}

/// Retrieve an integer value from a dialog item, clamping it within the specified bounds.
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `dlg_id` - Resource ID of the dialog item.
/// * `num_lo` - Minimum allowed value.
/// * `num_hi` - Maximum allowed value.
/// # Returns
/// The clamped integer value from the dialog item, or an error if retrieval or parsing fails.
pub fn get_dlg_int(
    h_dlg: &HWND,
    dlg_id: ResourceId,
    num_lo: u32,
    num_hi: u32,
) -> Result<u32, Box<dyn core::error::Error + Send + Sync>> {
    h_dlg
        // Get a handle to the dialog item
        .GetDlgItem(dlg_id as u16)
        // Retrieve the integer value from the dialog item
        .and_then(|dlg| dlg.GetWindowText())
        // If there is an error, convert it into a form that can be propagated down the chain
        .map_err(Into::into)
        // Parse the retrieved text into a u32
        .and_then(|text| text.parse::<u32>().map_err(Into::into))
        // Clamp the parsed value within the specified bounds
        .map(|value| value.clamp(num_lo, num_hi))
}

/// Scale a 96-DPI measurement to the current UI DPI
/// # Arguments
/// * `val` - The measurement in pixels at 96 DPI.
/// * `dpi` - The UI DPI to scale the measurement to.
/// # Returns
/// The measurement scaled to the given DPI.
/// # Notes
/// This function replicates the functionality of the `MulDiv` Win32 API function, with a few differences:
/// - It takes an unsigned integer value and returns an unsigned integer, while `MulDiv` operates on signed integers.
/// - It assumes that the denominator is always non-zero, which can be safely assumed in this context since `BASE_DPI` is a constant
///   and should never be zero.
pub const fn scale_dpi(val: u32, dpi: u32) -> u32 {
    // Perform multiplication in u64 to prevent overflow
    let product = val as u64 * dpi as u64;
    // Perform division with rounding
    ((product + (BASE_DPI as u64 / 2)) / BASE_DPI as u64) as u32
}
