//! Handles displaying help topics and context-sensitive help.
//!
//! # Notes
//! - `HtmlHelp` leaks a `DC` whenever the popup help menu is displayed and leaks a `HBRUSH`
//!   when opening the help window. This is likely a bug in the Win32 API itself.

use core::ffi::c_void;
use std::os::windows::ffi::OsStrExt as _;
use std::sync::LazyLock;

use windows_sys::Win32::Data::HtmlHelp::{
    HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HTML_HELP_COMMAND, HtmlHelpW,
};

use winsafe::{HELPINFO, HWND, co::HELPW};

use crate::util::ResourceId;

pub struct Help {}

impl Help {
    /// Help context ID mappings for dialogs
    ///
    /// Used by `WinHelp` to map control IDs to help context IDs.
    /// # Notes
    /// - The arrays are in pairs of (control ID, help context ID).
    /// - The arrays end with two zeros to signal the end of the mapping.
    pub const PREF_HELP_IDS: [u32; 14] = [
        ResourceId::HeightEdit as u32,
        ResourceId::PrefEditHeight as u32,
        ResourceId::WidthEdit as u32,
        ResourceId::PrefEditWidth as u32,
        ResourceId::MinesEdit as u32,
        ResourceId::PrefEditMines as u32,
        ResourceId::HeightText as u32,
        ResourceId::PrefEditHeight as u32,
        ResourceId::WidthText as u32,
        ResourceId::PrefEditWidth as u32,
        ResourceId::MinesText as u32,
        ResourceId::PrefEditMines as u32,
        0,
        0,
    ];

    /// Help context ID mappings for the best times dialog
    ///
    /// Used by `WinHelp` to map control IDs to help context IDs.
    /// # Notes
    /// - The arrays are in pairs of (control ID, help context ID).
    /// - The arrays end with two zeros to signal the end of the mapping.
    pub const BEST_HELP_IDS: [u32; 22] = [
        ResourceId::ResetBtn as u32,
        ResourceId::BestBtnReset as u32,
        ResourceId::SText1 as u32,
        ResourceId::SText as u32,
        ResourceId::SText2 as u32,
        ResourceId::SText as u32,
        ResourceId::SText3 as u32,
        ResourceId::SText as u32,
        ResourceId::BeginTime as u32,
        ResourceId::SText as u32,
        ResourceId::InterTime as u32,
        ResourceId::SText as u32,
        ResourceId::ExpertTime as u32,
        ResourceId::SText as u32,
        ResourceId::BeginName as u32,
        ResourceId::SText as u32,
        ResourceId::InterName as u32,
        ResourceId::SText as u32,
        ResourceId::ExpertName as u32,
        ResourceId::SText as u32,
        0,
        0,
    ];

    /// Gets the help file path based on the current executable's path.
    /// # Returns
    /// The help file path as a `Vec<u16>`.
    /// # Notes
    /// - The help file is expected to be in the same directory as the executable, and be named `(executable name).chm`.
    /// - The maximum path length for the executable is 245 characters. Any value exceeding this causes help to malfunction.
    /// - This function only computes the path once and caches it for future calls,
    ///   which means that changes to the executable's path during runtime will not be reflected.
    fn get_help_path() -> &'static Vec<u16> {
        static HELP_PATH: LazyLock<Vec<u16>> = LazyLock::new(|| {
            // Get the current executable path
            let Ok(mut path) = std::env::current_exe() else {
                eprintln!("Failed to get current executable path");
                return "winmine.chm\0".encode_utf16().collect();
            };
            // Change the extension to .chm
            path.set_extension("chm");
            // Ensure that the path is less than 245 characters
            if path.as_os_str().len() > 245 {
                eprintln!(
                    "Executable path longer than 245 characters: {}",
                    path.as_os_str().len()
                );
            }
            path.into_os_string()
                .encode_wide()
                .chain(core::iter::once(0))
                .collect()
        });
        &HELP_PATH
    }

    /// Applies help context based on the HELPINFO structure pointed to by `l_param`.
    /// # Arguments
    /// - `l_param` - The LPARAM containing a pointer to the HELPINFO structure.
    /// - `ids` - The array of help context IDs.
    /// # Returns
    /// True if help was applied, false otherwise.
    pub fn apply_help_from_info(help: &HELPINFO, ids: &[u32]) {
        unsafe {
            HtmlHelpW(
                help.hItemHandle().as_isize() as *mut c_void,
                Self::get_help_path().as_ptr(),
                HH_TP_HELP_WM_HELP as u32,
                ids.as_ptr() as usize,
            );
        }
    }

    /// Applies help context to a specific control.
    /// # Arguments
    /// - `hwnd` - The handle to the control.
    /// - `ids` - The array of help context IDs.
    pub fn apply_help_to_control(hwnd: &HWND, ids: &[u32]) {
        unsafe {
            HtmlHelpW(
                hwnd.ptr(),
                Self::get_help_path().as_ptr(),
                HH_TP_HELP_CONTEXTMENU as u32,
                ids.as_ptr() as usize,
            );
        }
    }

    /// Display the Help dialog for the given command.
    ///
    /// TODO: Integrate help files directly into the executable
    /// # Arguments
    /// - `w_command` - The help command (e.g., HELPONHELP).
    /// - `l_param` - Additional parameter for the help command.
    /// # Notes
    /// - If `w_command` is `HELPONHELP`, the standard Windows help file `NTHelp.chm` is used.
    /// - For other commands, the help file is derived from the executable's path, replacing its extension with `.chm`.
    /// - The help file is expected to be located in the same directory as the executable.
    /// - The "help on help" feature currently relies on the presence of `NTHelp.chm` in the current working directory.
    pub fn do_help(hwnd: &HWND, w_command: HELPW, l_param: HTML_HELP_COMMAND) {
        // Buffer to hold the help file path
        let mut path = Self::get_help_path().clone();

        // If the user has requested help on help, adjust the path to point to the appropriate topic
        if w_command == HELPW::HELPONHELP {
            // Note: This used to use `NTHelp.chm`, but that file is now integrated into `winmine.chm`
            // Remove the null terminator
            path.pop();
            // Append the path to "Using the Help Viewer" topic in winmine.chm
            path.extend_from_slice(
                "::/topics/nthelp_overview.htm"
                    .encode_utf16()
                    .chain(core::iter::once(0))
                    .collect::<Vec<u16>>()
                    .as_slice(),
            );
        }

        unsafe {
            HtmlHelpW(hwnd.ptr(), path.as_ptr(), l_param as u32, 0);
        }
    }
}
