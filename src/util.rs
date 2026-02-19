//! Utility functions and helpers used across the application.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use winsafe::{AnyResult, HWND, IdPos, prelude::*};

use crate::pref::GameType;
use crate::winmine::WinMineMainWindow;

/// Identifiers for resources used in the application, such as dialogs, menu items, and help contexts.
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
    /// Convert a `ResourceId` to a `u16` by using its underlying integer value.
    /// # Arguments
    /// - `res_id` - The `ResourceId` to convert.
    /// # Returns
    /// - The `u16` representation of the `ResourceId`.
    #[inline]
    fn from(res_id: ResourceId) -> Self {
        res_id as u16
    }
}

/// A wrapper around `RwLock` that handles poisoning by returning the inner data.
pub struct StateLock<T>(RwLock<T>);

impl<T> StateLock<T> {
    /// Create a new `StateLock` wrapping the given value.
    /// # Arguments
    /// - `value` - The value to wrap in the `RwLock`.
    pub const fn new(value: T) -> Self {
        Self(RwLock::new(value))
    }

    /// Get a read lock on the inner value, handling poisoning.
    /// # Returns
    /// - A `RwLockReadGuard` for the inner value
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        match self.0.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Get a write lock on the inner value, handling poisoning.
    /// # Returns
    /// - A `RwLockWriteGuard` for the inner value
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
/// - `seed` - The seed value to initialize the RNG with.
/// # Notes
/// This function replicates the functionality of the C standard library's `srand()` function.
pub fn seed_rng(seed: u16) {
    // Initialize the shared RNG state to the given seed value
    RNG_STATE.store(seed as u32, Ordering::Relaxed);
}

/// Generate the next pseudo-random number using a linear congruential generator.
/// # Returns
/// - The next pseudo-random number.
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
    // TODO: Loops are bad. Look into doing this a different way.
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
/// - `rnd_max` - Upper bound (exclusive) for the random number
/// # Returns
/// - A pseudo-random number in the [0, `rnd_max`) range
pub fn rnd(rnd_max: u32) -> u32 {
    rand() % rnd_max
}

impl WinMineMainWindow {
    /// Update the menu bar to reflect current preferences.
    /// # Returns
    /// - `Ok(())` - If the menu bar was successfully updated
    /// - `Err` - If there was an error retrieving the menu handle or updating the menu items
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
        let hmenu = self
            .wnd
            .hwnd()
            .GetMenu()
            .ok_or("Failed to get menu handle")?;
        hmenu.CheckMenuItem(
            IdPos::Id(ResourceId::Begin as u16),
            game_type == GameType::Begin,
        )?;
        hmenu.CheckMenuItem(
            IdPos::Id(ResourceId::Inter as u16),
            game_type == GameType::Inter,
        )?;
        hmenu.CheckMenuItem(
            IdPos::Id(ResourceId::Expert as u16),
            game_type == GameType::Expert,
        )?;
        hmenu.CheckMenuItem(
            IdPos::Id(ResourceId::Custom as u16),
            game_type == GameType::Other,
        )?;

        hmenu.CheckMenuItem(IdPos::Id(ResourceId::Color as u16), color)?;
        hmenu.CheckMenuItem(IdPos::Id(ResourceId::Mark as u16), mark)?;
        hmenu.CheckMenuItem(IdPos::Id(ResourceId::Sound as u16), sound)?;

        Ok(())
    }
}

/// Retrieve an integer value from a dialog item, clamping it within the specified bounds.
/// # Arguments
/// - `h_dlg` - Handle to the dialog window.
/// - `dlg_id` - Resource ID of the dialog item.
/// - `num_lo` - Minimum allowed value.
/// - `num_hi` - Maximum allowed value.
/// # Returns
/// - `Ok(u32)` - The clamped integer value from the dialog item.
/// - `Err` - If there was an error retrieving or parsing the value.
pub fn get_dlg_int(h_dlg: &HWND, dlg_id: ResourceId, num_lo: u32, num_hi: u32) -> AnyResult<u32> {
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
