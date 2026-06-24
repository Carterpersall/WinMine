//! Handles displaying help topics and context-sensitive help.
//!
//! # Notes
//! - `HtmlHelp` leaks a `DC` whenever the popup help menu is displayed and leaks a `HBRUSH`
//!   when opening the help window. This is likely a bug in the Win32 API itself.

use std::io::Write as _;
use std::sync::LazyLock;

use winsafe::HwndHmenu::{Hmenu, Hwnd};
use winsafe::prelude::Handle as _;
use winsafe::{HELPINFO, HWND, HhCmd};

use crate::util::ResourceId;

/// Help handling for the Minesweeper game, including context-sensitive help and help file management.
pub(crate) struct Help;

impl Help {
    /// Help context ID mappings for dialogs
    ///
    /// Used by `WinHelp` to map control IDs to help context IDs.
    /// # Notes
    /// - The arrays are in pairs of (control ID, help context ID).
    pub(crate) const PREF_HELP_IDS: [(u16, u16); 6] = [
        (
            ResourceId::HeightEdit as u16,
            ResourceId::PrefEditHeight as u16,
        ),
        (
            ResourceId::WidthEdit as u16,
            ResourceId::PrefEditWidth as u16,
        ),
        (
            ResourceId::MinesEdit as u16,
            ResourceId::PrefEditMines as u16,
        ),
        (
            ResourceId::HeightText as u16,
            ResourceId::PrefEditHeight as u16,
        ),
        (
            ResourceId::WidthText as u16,
            ResourceId::PrefEditWidth as u16,
        ),
        (
            ResourceId::MinesText as u16,
            ResourceId::PrefEditMines as u16,
        ),
    ];

    /// Help context ID mappings for the best times dialog
    ///
    /// Used by `WinHelp` to map control IDs to help context IDs.
    /// # Notes
    /// - The arrays are in pairs of (control ID, help context ID).
    pub(crate) const BEST_HELP_IDS: [(u16, u16); 10] = [
        (ResourceId::ResetBtn as u16, ResourceId::BestBtnReset as u16),
        (ResourceId::SText1 as u16, ResourceId::SText as u16),
        (ResourceId::SText2 as u16, ResourceId::SText as u16),
        (ResourceId::SText3 as u16, ResourceId::SText as u16),
        (ResourceId::BeginTime as u16, ResourceId::SText as u16),
        (ResourceId::InterTime as u16, ResourceId::SText as u16),
        (ResourceId::ExpertTime as u16, ResourceId::SText as u16),
        (ResourceId::BeginName as u16, ResourceId::SText as u16),
        (ResourceId::InterName as u16, ResourceId::SText as u16),
        (ResourceId::ExpertName as u16, ResourceId::SText as u16),
    ];

    /// Gets the help file path as a wide string suitable for passing to the Win32 API.
    /// # Returns
    /// - The help file path.
    /// # Notes
    /// - The help file is integrated into the executable as a byte array and extracted to the temp directory at runtime.
    /// - The maximum path length for the help file is 245 characters. Any value exceeding this causes help to malfunction.
    /// - This function only computes the path once and caches it for future calls,
    ///   which means that changes to the help file's path during runtime will not be reflected.
    fn get_help_path() -> &'static str {
        static EMBEDDED_CHM: &[u8] = include_bytes!("../help/winmine.chm");
        static HELP_PATH: LazyLock<String> = LazyLock::new(|| {
            // Get the path to %TEMP%\winmine.chm and check if it already exists
            let mut path = std::env::temp_dir();
            path.push("winmine.chm");

            // Attempt to create the file if it doesn't currently exist
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    // If the file doesn't exist, write the embedded CHM data to the temp directory
                    // Note: Errors are logged but not propagated since help is a non-essential feature
                    if let Err(e) = file.write_all(EMBEDDED_CHM) {
                        eprintln!("Failed to write embedded help file to temp directory: {e}");
                    };
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // If the file already exists, check if it's the correct help file and replace it if it isn't
                    match std::fs::read(&path) {
                        Ok(existing_bytes) => {
                            if existing_bytes != EMBEDDED_CHM {
                                eprintln!(
                                    "Existing help file in temp directory does not match embedded help file. Overwriting."
                                );
                                if let Err(err) = std::fs::write(&path, EMBEDDED_CHM) {
                                    eprintln!(
                                        "Failed to overwrite help file in temp directory: {err}"
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("Failed to read existing help file in temp directory: {err}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create help file in temp directory: {e}");
                }
            }

            // Ensure that the path is less than 245 characters
            if path.as_os_str().len() > 245 {
                eprintln!(
                    "Help file path longer than 245 characters: {}",
                    path.as_os_str().len()
                );
            }

            // Convert the path to a string and return it
            path.into_os_string().into_string().unwrap_or_else(|_| {
                eprintln!("Failed to convert help file path to string");
                String::new()
            })
        });
        &HELP_PATH
    }

    /// Applies help context based on the HELPINFO structure pointed to by `help`.
    /// # Arguments
    /// - `help` - A HELPINFO structure containing help information.
    /// - `ids` - The array of help context IDs.
    pub(crate) fn apply_help_from_info(help: &HELPINFO, ids: &[(u16, u16)]) {
        // Get a pointer to the control that requested help, which may be a window handle or a menu handle
        let hwndcaller = match help.hItemHandle() {
            Hwnd(hwnd) => hwnd,
            Hmenu(_hmenu) => {
                eprintln!("Help requested from a menu handle, which is not supported. Ignoring.");
                HWND::NULL
            }
        };

        hwndcaller.HtmlHelp(Self::get_help_path(), HhCmd::TpHelpWmHelp(ids));
    }

    /// Displays the help dialog for the "help on Help" command.
    /// # Arguments
    /// - `hwnd` - The handle to the parent window for the help dialog.
    /// # Notes
    /// - The program used to use `NTHelp.chm` for this feature, but that file is now integrated into `winmine.chm`
    ///   due to `NTHelp.chm` not being included in modern versions of Windows.
    pub(crate) fn do_help_on_help(hwnd: &HWND) {
        // Buffer to hold the help file path
        let mut path = Self::get_help_path().to_owned();

        // Append the path to the "Using the Help Viewer" topic in winmine.chm
        path.push_str("::/topics/nthelp_overview.htm");

        hwnd.HtmlHelp(&path, HhCmd::DisplayToc);
    }

    /// Display the Help dialog for the given command.
    /// # Arguments
    /// - `hwnd` - The handle to the parent window for the help dialog.
    /// - `cmd` - The help command.
    /// # Notes
    /// - The help file is derived from the executable's path, replacing its extension with `.chm`.
    /// - The help file is expected to be located in the same directory as the executable.
    pub(crate) fn do_help(hwnd: &HWND, cmd: HhCmd) {
        hwnd.HtmlHelp(Self::get_help_path(), cmd);
    }
}
