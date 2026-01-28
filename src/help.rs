//! Handles displaying help topics and context-sensitive help.

use core::ffi::c_void;

use windows_sys::Win32::Data::HtmlHelp::{HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HtmlHelpA};

use winsafe::{HELPINFO, HWND, co::HELPW, prelude::Handle as _};

use crate::winmine::ControlId;

/// Maximum path buffer used when resolving help files.
const CCH_MAX_PATHNAME: usize = 250;

/// Help file name
const HELP_FILE: &str = "winmine.chm\0";

/// Help context identifiers.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum HelpContextId {
    /// Edit control for board height
    PrefEditHeight = 1000,
    /// Edit control for board width
    PrefEditWidth = 1001,
    /// Edit control for number of mines
    PrefEditMines = 1002,
    /// Reset best times button
    BestBtnReset = 1003,
    /// Static text for best times
    SText = 1004,
}

pub struct Help {}

impl Help {
    /// Help context ID mappings for dialogs
    ///
    /// Used by `WinHelp` to map control IDs to help context IDs.
    /// # Notes
    /// - The arrays are in pairs of (control ID, help context ID).
    /// - The arrays end with two zeros to signal the end of the mapping.
    pub const PREF_HELP_IDS: [u32; 14] = [
        ControlId::EditHeight as u32,
        HelpContextId::PrefEditHeight as u32,
        ControlId::EditWidth as u32,
        HelpContextId::PrefEditWidth as u32,
        ControlId::EditMines as u32,
        HelpContextId::PrefEditMines as u32,
        ControlId::TxtHeight as u32,
        HelpContextId::PrefEditHeight as u32,
        ControlId::TxtWidth as u32,
        HelpContextId::PrefEditWidth as u32,
        ControlId::TxtMines as u32,
        HelpContextId::PrefEditMines as u32,
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
        ControlId::BtnReset as u32,
        HelpContextId::BestBtnReset as u32,
        ControlId::SText1 as u32,
        HelpContextId::SText as u32,
        ControlId::SText2 as u32,
        HelpContextId::SText as u32,
        ControlId::SText3 as u32,
        HelpContextId::SText as u32,
        ControlId::TimeBegin as u32,
        HelpContextId::SText as u32,
        ControlId::TimeInter as u32,
        HelpContextId::SText as u32,
        ControlId::TimeExpert as u32,
        HelpContextId::SText as u32,
        ControlId::NameBegin as u32,
        HelpContextId::SText as u32,
        ControlId::NameInter as u32,
        HelpContextId::SText as u32,
        ControlId::NameExpert as u32,
        HelpContextId::SText as u32,
        0,
        0,
    ];

    /// Applies help context based on the HELPINFO structure pointed to by `l_param`.
    /// # Arguments
    /// * `l_param` - The LPARAM containing a pointer to the HELPINFO structure.
    /// * `ids` - The array of help context IDs.
    /// # Returns
    /// True if help was applied, false otherwise.
    pub fn apply_help_from_info(help: &HELPINFO, ids: &[u32]) {
        unsafe {
            HtmlHelpA(
                help.hItemHandle().as_isize() as *mut c_void,
                HELP_FILE.as_ptr(),
                HH_TP_HELP_WM_HELP as u32,
                ids.as_ptr() as usize,
            );
        }
    }

    /// Applies help context to a specific control.
    ///
    /// TODO: There is a DC leak somewhere around here.
    /// # Arguments
    /// * `hwnd` - The handle to the control.
    /// * `ids` - The array of help context IDs.
    pub fn apply_help_to_control(hwnd: &HWND, ids: &[u32]) {
        if let Some(control) = hwnd.as_opt() {
            unsafe {
                HtmlHelpA(
                    control.ptr(),
                    HELP_FILE.as_ptr(),
                    HH_TP_HELP_CONTEXTMENU as u32,
                    ids.as_ptr() as usize,
                );
            }
        }
    }

    /// Display the Help dialog for the given command.
    ///
    /// TODO: Refactor this function to only use the help dialog built into the resource file
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
}
