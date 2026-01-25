//! Utility functions and helpers used across the application.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use windows_sys::Win32::Data::HtmlHelp::HtmlHelpA;

use winsafe::co::{HELPW, KEY, REG_OPTION, SM};
use winsafe::{AnyResult, GetSystemMetrics, GetTickCount64, HKEY, HMENU, HWND, IdPos, prelude::*};

use crate::globals::CXBORDER;
use crate::pref::{GameType, MenuMode, SZ_WINMINE_REG_STR, SoundState};
use crate::rtns::{AdjustFlag, preferences_mutex};
use crate::winmine::{MenuCommand, WinMineMainWindow};

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

/// Multiplier used by the linear congruential generator that produces the app's RNG values.
const RNG_MULTIPLIER: u32 = 1_103_515_245;
/// Increment used by the linear congruential generator.
const RNG_INCREMENT: u32 = 12_345;
/// Default seed applied when the RNG would otherwise start at zero.
const RNG_DEFAULT_SEED: u32 = 0xACE1_1234;
/// Shared state of the linear congruential generator.
static RNG_STATE: AtomicU32 = AtomicU32::new(RNG_DEFAULT_SEED);

/// Icon resources embedded in the executable.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum IconId {
    /// Main application icon.
    Main = 100,
}

/// Maximum path buffer used when resolving help files.
const CCH_MAX_PATHNAME: usize = 250;

/// Seed the RNG with the specified seed value.
/// # Arguments
/// * `seed` - The seed value to initialize the RNG with. If zero, a default seed is used.
fn seed_rng(seed: u32) {
    // Ensure the RNG seed is never zero
    let value = if seed == 0 { RNG_DEFAULT_SEED } else { seed };
    // Initialize the shared RNG state to the given seed value
    RNG_STATE.store(value, Ordering::Relaxed);
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
fn next_rand() -> u32 {
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
    next_rand() % rnd_max
}

/// Initialize UI globals and seed the RNG state.
///
/// TODO: Does this function need to exist? It is only called once during startup.
pub fn init_const() {
    // Seed the RNG using the low 16 bits of the current tick count
    let ticks = (GetTickCount64() as u32) & 0xFFFF;
    seed_rng(ticks as u32);

    // Get the system metrics for border width
    CXBORDER.store(GetSystemMetrics(SM::CXBORDER) + 1, Ordering::Relaxed);

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
pub fn menu_check(hmenu: &HMENU, idm: MenuCommand, f_check: bool) {
    if let Some(menu) = hmenu.as_opt() {
        let _ = menu.CheckMenuItem(IdPos::Id(idm as u16), f_check);
    }
}

impl WinMineMainWindow {
    /// Show or hide the menu bar based on the specified mode.
    /// # Arguments
    /// * `f_active` - The desired menu mode.
    pub fn set_menu_bar(&self, f_active: MenuMode) -> AnyResult<()> {
        // Persist the menu visibility preference, refresh accelerator state, and resize the window.
        let (menu_on, game_type, color, mark, sound) = {
            let mut prefs = match preferences_mutex().lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            prefs.menu_mode = f_active;
            (
                !matches!(prefs.menu_mode, MenuMode::Hidden),
                prefs.game_type,
                prefs.color,
                prefs.mark_enabled,
                prefs.sound_state,
            )
        };

        // Update the menu checkmarks to reflect the current preferences
        let hmenu = self.wnd.hwnd().GetMenu().unwrap_or(HMENU::NULL);
        menu_check(&hmenu, MenuCommand::Begin, game_type == GameType::Begin);
        menu_check(&hmenu, MenuCommand::Inter, game_type == GameType::Inter);
        menu_check(&hmenu, MenuCommand::Expert, game_type == GameType::Expert);
        menu_check(&hmenu, MenuCommand::Custom, game_type == GameType::Other);

        menu_check(&hmenu, MenuCommand::Color, color);
        menu_check(&hmenu, MenuCommand::Mark, mark);
        menu_check(&hmenu, MenuCommand::Sound, sound == SoundState::On);

        // Show or hide the menu bar as set in preferences
        let menu = self.wnd.hwnd().GetMenu().unwrap_or(HMENU::NULL);
        let menu_arg = if menu_on { &menu } else { &HMENU::NULL };
        self.wnd.hwnd().SetMenu(menu_arg)?;
        self.adjust_window(AdjustFlag::Resize as i32)?;

        Ok(())
    }
}

/// Display the Help dialog for the given command.
///
/// TODO: Refactor this function to only use the help dialog built into the resource file
/// TODO: Move this function into a help module
/// # Arguments
/// * `w_command` - The help command (e.g., HELPONHELP).
/// * `l_param` - Additional parameter for the help command.
pub fn do_help(hwnd: &HWND, w_command: HELPW, l_param: u32) {
    // htmlhelp.dll expects either the localized .chm next to the EXE or the fallback NTHelp file.
    let mut buffer = [0u8; CCH_MAX_PATHNAME];

    if w_command == HELPW::HELPONHELP {
        const HELP_FILE: &[u8] = b"NTHelp.chm\0";
        buffer[..HELP_FILE.len()].copy_from_slice(HELP_FILE);
    } else {
        let exe_path = hwnd.hinstance().GetModuleFileName().unwrap_or_default();
        let mut bytes = exe_path.into_bytes();
        if bytes.len() + 1 > CCH_MAX_PATHNAME {
            bytes.truncate(CCH_MAX_PATHNAME - 1);
        }
        bytes.push(0);
        let len = bytes.len() - 1;
        buffer[..bytes.len()].copy_from_slice(&bytes);

        let mut dot = None;
        for i in (0..len).rev() {
            if buffer[i] == b'.' {
                dot = Some(i);
                break;
            }
            if buffer[i] == b'\\' {
                break;
            }
        }
        let pos = dot.unwrap_or(len);
        const EXT: &[u8] = b".chm\0";
        let mut i = 0;
        while i < EXT.len() && pos + i < buffer.len() {
            buffer[pos + i] = EXT[i];
            i += 1;
        }
    }

    unsafe {
        HtmlHelpA(hwnd.ptr(), buffer.as_ptr(), l_param, 0);
    }
}

/// Retrieve an integer value from a dialog item, clamping it within the specified bounds.
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `dlg_id` - The dialog item ID.
/// * `num_lo` - Minimum allowed value.
/// * `num_hi` - Maximum allowed value.
/// # Returns
/// The clamped integer value from the dialog item, or an error if retrieval or parsing fails.
pub fn get_dlg_int(
    h_dlg: &HWND,
    dlg_id: i32,
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
