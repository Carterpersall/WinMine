//! Main window and event handling for the Minesweeper game.

use core::cmp::{max, min};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows_sys::Win32::Data::HtmlHelp::{
    HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC, HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HtmlHelpA,
};

use winsafe::co::{BN, DLGID, HELPW, ICC, IDC, MK, PM, SC, SM, STOCK_BRUSH, SW, VK, WA, WM, WS};
use winsafe::msg::{WndMsg, em::SetLimitText, wm::Destroy};
use winsafe::{
    AdjustWindowRectExForDpi, AnyResult, GetSystemMetrics, HBRUSH, HELPINFO, HINSTANCE, HWND,
    INITCOMMONCONTROLSEX, IdIdiStr, IdStr, InitCommonControlsEx, MSG, POINT, PeekMessage, PtsRc,
    RECT, SIZE, WINDOWPOS, gui, prelude::*,
};

use crate::globals::{
    BASE_DPI, DEFAULT_PLAYER_NAME, DRAG_ACTIVE, GAME_NAME, IGNORE_NEXT_CLICK, MSG_CREDIT,
    MSG_FASTEST_BEGINNER, MSG_FASTEST_EXPERT, MSG_FASTEST_INTERMEDIATE, MSG_VERSION_NAME, UI_DPI,
    WINDOW_HEIGHT, WINDOW_WIDTH,
};
use crate::grafix::{
    ButtonSprite, DX_BLK_96, DX_BUTTON_96, DX_LEFT_SPACE_96, DX_RIGHT_SPACE_96, DY_BLK_96,
    DY_BOTTOM_SPACE_96, DY_BUTTON_96, DY_GRID_OFF_96, DY_TOP_LED_96, display_button, draw_screen,
    load_bitmaps, scale_dpi,
};
use crate::pref::{
    CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH, MenuMode, read_preferences, write_preferences,
};
use crate::rtns::{AdjustFlag, GameState, ID_TIMER, StatusFlag, preferences_mutex};
use crate::sound::Sound;
use crate::util::{IconId, StateLock, do_help, get_dlg_int, init_const};

/// Indicates that preferences have changed and should be saved
static UPDATE_INI: AtomicBool = AtomicBool::new(false);

/// `WM_APP` request code posted to the main window when a new best time is
/// recorded.
///
/// The main UI thread handles this by showing the name-entry dialog, then the
/// best-times dialog.
pub const NEW_RECORD_DLG: usize = 1;

/// Menu and accelerator resource identifiers.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum MenuResourceId {
    /// Main menu resource.
    Menu = 500,
    /// Accelerator table resource.
    Accelerators = 501,
}
// TODO: Change these to be offsets of WM_APP, as they may conflict with system commands in their current form
/// Menu and accelerator command identifiers.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MenuCommand {
    /// Start a new game.
    New = 510,
    /// Exit the application.
    Exit = 512,
    /// Select the Beginner difficulty.
    Begin = 521,
    /// Select the Intermediate difficulty.
    Inter = 522,
    /// Select the Expert difficulty.
    Expert = 523,
    /// Open the Custom board dialog.
    Custom = 524,
    /// Toggle sound effects.
    Sound = 526,
    /// Toggle question-mark marks.
    Mark = 527,
    /// Show the best times dialog.
    Best = 528,
    /// Toggle color bitmaps.
    Color = 529,
    /// Open help.
    Help = 590,
    /// Show "search for help topics" entry.
    SearchHelp = 591,
    /// Open the help-about-help entry.
    UsingHelp = 592,
    /// Show the About dialog.
    AboutMinesweeper = 593,
}

impl TryFrom<usize> for MenuCommand {
    type Error = Box<dyn core::error::Error>;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match (value & 0xFFFF) as u16 {
            510 => Ok(MenuCommand::New),
            512 => Ok(MenuCommand::Exit),
            521 => Ok(MenuCommand::Begin),
            522 => Ok(MenuCommand::Inter),
            523 => Ok(MenuCommand::Expert),
            524 => Ok(MenuCommand::Custom),
            526 => Ok(MenuCommand::Sound),
            527 => Ok(MenuCommand::Mark),
            528 => Ok(MenuCommand::Best),
            529 => Ok(MenuCommand::Color),
            590 => Ok(MenuCommand::Help),
            591 => Ok(MenuCommand::SearchHelp),
            592 => Ok(MenuCommand::UsingHelp),
            593 => Ok(MenuCommand::AboutMinesweeper),
            val => Err(format!("Invalid MenuCommand value: {}", val).into()),
        }
    }
}

/// Dialog template identifiers.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum DialogTemplateId {
    /// Custom game preferences dialog
    Pref = 80,
    /// Best times display dialog
    Enter = 600,
    /// Confirm reset best times dialog
    Best = 700,
}

/// Control identifiers shared across dialogs.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum ControlId {
    /// Edit control for board height
    EditHeight = 141,
    /// Edit control for board width
    EditWidth = 142,
    /// Edit control for number of mines
    EditMines = 143,
    /// OK button
    BtnOk = 100,
    /// Cancel button
    _BtnCancel = 109,
    /// Reset best times button
    BtnReset = 707,
    /// Static text for best times
    TextBest = 601,
    /// Edit control for player name
    EditName = 602,
    /// Beginner best time
    TimeBegin = 701,
    /// Beginner player name
    NameBegin = 702,
    /// Intermediate best time
    TimeInter = 703,
    /// Intermediate player name
    NameInter = 704,
    /// Expert best time
    TimeExpert = 705,
    /// Expert player name
    NameExpert = 706,
    /// Static text 1 for best times
    SText1 = 708,
    /// Static text 2 for best times
    SText2 = 709,
    /// Static text 3 for best times
    SText3 = 710,
    /// Static text for number of mines
    TxtMines = 111,
    /// Static text for board height
    TxtHeight = 112,
    /// Static text for board width
    TxtWidth = 113,
}

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

/// Help file name
const HELP_FILE: &str = "winmine.chm\0";

/// Help context ID mappings for dialogs
///
/// Used by `WinHelp` to map control IDs to help context IDs.
/// # Notes
/// - The arrays are in pairs of (control ID, help context ID).
/// - The arrays end with two zeros to signal the end of the mapping.
const PREF_HELP_IDS: [u32; 14] = [
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
const BEST_HELP_IDS: [u32; 22] = [
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

/// Struct containing the main window with its event handlers and the shared state.
#[derive(Clone)]
pub struct WinMineMainWindow {
    /// The main window, containing the HWND and event callbacks
    pub wnd: gui::WindowMain,
    /// Shared state for the game
    pub state: Arc<StateLock<GameState>>,
}

impl WinMineMainWindow {
    /// Creates the main window and hooks its events.
    /// # Arguments
    /// * `wnd`: The main window to wrap.
    /// # Returns
    /// The wrapped main window with events hooked.
    fn new(wnd: gui::WindowMain) -> Self {
        let new_self = Self {
            wnd,
            state: Arc::new(StateLock::new(GameState::new())),
        };
        new_self.events();
        new_self
    }

    /* Message Helper Functions */

    /// Begins a primary button drag operation.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn begin_primary_button_drag(&self) -> AnyResult<()> {
        DRAG_ACTIVE.store(true, Ordering::Relaxed);
        self.state.write().cursor_pos = POINT { x: -1, y: -1 };
        display_button(self.wnd.hwnd(), ButtonSprite::Caution)
    }

    /// Finishes a primary button drag operation.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn finish_primary_button_drag(&self) -> AnyResult<()> {
        DRAG_ACTIVE.store(false, Ordering::Relaxed);
        if self.state.read().game_status.contains(StatusFlag::Play) {
            self.state.write().do_button_1_up(self.wnd.hwnd())?;
        } else {
            self.state.write().track_mouse(self.wnd.hwnd(), -2, -2)?;
        }
        // If a chord operation was active, end it now
        self.state.write().chord_active = false;
        Ok(())
    }

    /// Handles the `WM_KEYDOWN` message.
    ///
    /// TODO: Move this function into the closure.
    /// # Arguments
    /// * `key`: The virtual key code of the key that was pressed.
    /// # Returns
    /// An `Ok(())` if successful, or an error if handling the key failed.
    fn handle_keydown(&self, key: VK) -> AnyResult<()> {
        match key {
            code if code == VK::F4 => {
                let new_sound = match preferences_mutex().sound_enabled {
                    true => {
                        Sound::stop_all();
                        false
                    }
                    false => Sound::init(),
                };

                let f_menu = {
                    let mut prefs = preferences_mutex();
                    prefs.sound_enabled = new_sound;
                    prefs.menu_mode
                };

                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(f_menu)?;
            }
            code if code == VK::F5 => {
                let menu_value = { preferences_mutex().menu_mode };

                if !matches!(menu_value, MenuMode::AlwaysOn) {
                    self.set_menu_bar(MenuMode::Hidden)?;
                }
            }
            code if code == VK::F6 => {
                let menu_value = { preferences_mutex().menu_mode };

                if !matches!(menu_value, MenuMode::AlwaysOn) {
                    self.set_menu_bar(MenuMode::On)?;
                }
            }
            code if code == VK::SHIFT => self.handle_xyzzys_shift(),
            _ => self.handle_xyzzys_default_key(key),
        }

        Ok(())
    }

    /// Handles mouse move events.
    /// # Arguments
    /// * `key`: The mouse buttons currently pressed.
    /// * `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// An `Ok(())` if successful, or an error if handling the mouse move failed.
    fn handle_mouse_move(&self, key: MK, point: POINT) -> AnyResult<()> {
        if DRAG_ACTIVE.load(Ordering::Relaxed) {
            // If the user is dragging, track the mouse position
            if self.state.read().game_status.contains(StatusFlag::Play) {
                self.state.write().track_mouse(
                    self.wnd.hwnd(),
                    self.x_box_from_xpos(point.x),
                    self.y_box_from_ypos(point.y),
                )?;
            } else {
                self.finish_primary_button_drag()?;
            }
        } else {
            // Regular mouse move
            self.handle_xyzzys_mouse(key, point)?;
        }
        Ok(())
    }

    /// Handles right mouse button down events.
    /// # Arguments
    /// * `btn`: The mouse button that was pressed.
    /// * `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// An `Ok(())` if successful, or an error if handling the right button down failed.
    fn handle_rbutton_down(&self, btn: MK, point: POINT) -> AnyResult<()> {
        // Ignore right-clicks if the next click is set to be ignored
        if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed)
            || !self.state.read().game_status.contains(StatusFlag::Play)
        {
            return Ok(());
        }

        // If the left and right buttons are both down, and the middle button is not down, start a chord operation
        if btn & (MK::LBUTTON | MK::RBUTTON | MK::MBUTTON) == MK::LBUTTON | MK::RBUTTON {
            let state = &mut self.state.write();
            state.chord_active = true;
            state.track_mouse(self.wnd.hwnd(), -3, -3)?;
            self.begin_primary_button_drag()?;
            self.handle_mouse_move(btn, point)?;
            return Ok(());
        }

        // Regular right-click: make a guess
        self.state.write().make_guess(
            self.wnd.hwnd(),
            self.x_box_from_xpos(point.x),
            self.y_box_from_ypos(point.y),
        )?;
        Ok(())
    }

    /// Handles the `WM_SYSCOMMAND` message for minimize and restore events.
    ///
    /// TODO: Use the normal WM commands rather than the basic WM_SYSCOMMAND message
    /// # Arguments
    /// * `command` - The system command identifier.
    fn handle_syscommand(&self, command: SC) {
        let state = &mut self.state.write();
        if command == SC::MINIMIZE {
            state.pause_game();
            state.game_status.insert(StatusFlag::Pause);
            state.game_status.insert(StatusFlag::Minimized);
        } else if command == SC::RESTORE {
            state.game_status.remove(StatusFlag::Pause);
            state.game_status.remove(StatusFlag::Minimized);
            state.resume_game();
            IGNORE_NEXT_CLICK.store(false, Ordering::Relaxed);
        }
    }

    /// Handles the `WM_WINDOWPOSCHANGED` message to store the new window position in preferences.
    /// # Arguments
    /// * `pos` - A reference to the `WINDOWPOS` structure containing the new window position.
    fn handle_window_pos_changed(&self, pos: &WINDOWPOS) {
        if self
            .state
            .read()
            .game_status
            .contains(StatusFlag::Minimized)
        {
            return;
        }

        let mut prefs = preferences_mutex();
        prefs.wnd_x_pos = pos.x;
        prefs.wnd_y_pos = pos.y;
    }

    /// Handles clicks on the smiley face button.
    /// # Arguments
    /// * `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// An `AnyResult<bool>` indicating whether the click was handled.
    ///
    /// TODO: Does it need to return a bool and a result?
    fn btn_click_handler(&self, point: POINT) -> AnyResult<bool> {
        // Handle clicks on the smiley face button while providing the pressed animation.
        let mut msg = MSG::default();

        msg.pt.x = point.x;
        msg.pt.y = point.y;

        let dx_window = WINDOW_WIDTH.load(Ordering::Relaxed);
        let dx_button = scale_dpi(DX_BUTTON_96);
        let dy_button = scale_dpi(DY_BUTTON_96);
        let dy_top_led = scale_dpi(DY_TOP_LED_96);
        let mut rc = RECT {
            left: (dx_window - dx_button) / 2,
            top: dy_top_led,
            right: 0,
            bottom: 0,
        };
        rc.right = rc.left + dx_button;
        rc.bottom = rc.top + dy_button;

        if !winsafe::PtInRect(rc, msg.pt) {
            return Ok(false);
        }

        display_button(self.wnd.hwnd(), ButtonSprite::Down)?;
        self.wnd
            .hwnd()
            .MapWindowPoints(&HWND::NULL, PtsRc::Rc(&mut rc))?;

        let mut pressed = true;
        loop {
            if PeekMessage(
                &mut msg,
                self.wnd.hwnd().as_opt(),
                WM::MOUSEFIRST.raw(),
                WM::MOUSELAST.raw(),
                PM::REMOVE,
            ) {
                match msg.message {
                    WM::LBUTTONUP => {
                        if pressed && winsafe::PtInRect(rc, msg.pt) {
                            self.state.write().btn_face_state = ButtonSprite::Happy;
                            display_button(self.wnd.hwnd(), ButtonSprite::Happy)?;
                            self.start_game()?;
                        }
                        return Ok(true);
                    }
                    WM::MOUSEMOVE => {
                        if winsafe::PtInRect(rc, msg.pt) {
                            if !pressed {
                                pressed = true;
                                display_button(self.wnd.hwnd(), ButtonSprite::Down)?;
                            }
                        } else if pressed {
                            pressed = false;
                            display_button(self.wnd.hwnd(), self.state.read().btn_face_state)?;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /* Helper Functions */

    /// Adjusts the main window size and position based on the current board and menu state.
    ///
    /// This function is called whenever the board or menu state changes to ensure
    /// that the main window is appropriately sized and positioned on the screen.
    /// # Arguments
    /// * `f_adjust` - Flags indicating how to adjust the window (e.g., resize).
    /// # Returns
    /// An `Ok(())` if successful, or an error if adjustment failed.
    pub fn adjust_window(&self, mut f_adjust: AdjustFlag) -> AnyResult<()> {
        // Get the current menu handle
        let menu_handle = self.wnd.hwnd().GetMenu();

        // Calculate desired window size based on board dimensions and DPI scaling
        let dx_window = scale_dpi(DX_BLK_96) * self.state.read().board_width
            + scale_dpi(DX_LEFT_SPACE_96)
            + scale_dpi(DX_RIGHT_SPACE_96);
        let dy_window = scale_dpi(DY_BLK_96) * self.state.read().board_height
            + scale_dpi(DY_GRID_OFF_96)
            + scale_dpi(DY_BOTTOM_SPACE_96);
        WINDOW_WIDTH.store(dx_window, Ordering::Relaxed);
        WINDOW_HEIGHT.store(dy_window, Ordering::Relaxed);

        // Get the current window position and menu mode from preferences
        let (mut x_window, mut y_window, f_menu) = {
            let prefs = preferences_mutex();
            (prefs.wnd_x_pos, prefs.wnd_y_pos, prefs.menu_mode)
        };

        // Determine if the menu is visible based on preferences and menu handle availability
        let menu_visible = !matches!(f_menu, MenuMode::Hidden) && menu_handle.is_some();

        let desired = RECT {
            left: 0,
            top: 0,
            right: dx_window,
            bottom: dy_window,
        };
        // Adjust the window rect for the current DPI
        let adjusted = AdjustWindowRectExForDpi(
            desired,
            self.wnd.hwnd().style(),
            menu_visible,
            self.wnd.hwnd().style_ex(),
            UI_DPI.load(Ordering::Relaxed),
        )?;

        // Calculate total window size including non-client areas
        let cx_total = adjusted.right - adjusted.left;
        let cy_total = adjusted.bottom - adjusted.top;
        // Calculate frame adjustments needed to fit the desired client area
        let frame_extra = max(0, cx_total - dx_window);
        let dyp_adjust = max(0, cy_total - dy_window);

        // Get the screen width
        let cx_screen = {
            let mut result = GetSystemMetrics(SM::CXVIRTUALSCREEN);
            if result == 0 {
                result = GetSystemMetrics(SM::CXSCREEN);
            }
            result
        };
        // If the window exceeds the screen width, adjust its x position to be within bounds
        let mut excess = x_window + dx_window + frame_extra - cx_screen;
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize;
            x_window -= excess;
        }
        // Get the screen height
        let cy_screen = {
            let mut result = GetSystemMetrics(SM::CYVIRTUALSCREEN);
            if result == 0 {
                result = GetSystemMetrics(SM::CYSCREEN);
            }
            result
        };
        // If the window exceeds the screen height, adjust its y position to be within bounds
        excess = y_window + dy_window + dyp_adjust - cy_screen;
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize;
            y_window -= excess;
        }

        // If a window resize has been requested, move and resize the window accordingly
        if f_adjust.contains(AdjustFlag::Resize) {
            self.wnd.hwnd().MoveWindow(
                POINT {
                    x: x_window,
                    y: y_window,
                },
                SIZE {
                    cx: dx_window + frame_extra,
                    cy: dy_window + dyp_adjust,
                },
                true,
            )?;
        }

        // If a display refresh has been requested, invalidate the window's client area
        if f_adjust.contains(AdjustFlag::Redraw) {
            let rect = RECT {
                left: 0,
                top: 0,
                right: dx_window,
                bottom: dy_window,
            };
            self.wnd.hwnd().InvalidateRect(Some(&rect), true)?;
        }

        // Update preferences with the new window position
        let mut prefs = preferences_mutex();
        prefs.wnd_x_pos = x_window;
        prefs.wnd_y_pos = y_window;

        Ok(())
    }

    /// Converts an x-coordinate in pixels to a box index.
    /// # Arguments
    /// * `x`: The x-coordinate in pixels.
    /// # Returns
    /// The corresponding box index.
    pub fn x_box_from_xpos(&self, x: i32) -> i32 {
        let cell = scale_dpi(DX_BLK_96);
        if cell <= 0 {
            return 0;
        }
        (x - (scale_dpi(DX_LEFT_SPACE_96) - cell)) / cell
    }

    /// Converts a y-coordinate in pixels to a box index.
    /// # Arguments
    /// * `y`: The y-coordinate in pixels.
    /// # Returns
    /// The corresponding box index.
    pub fn y_box_from_ypos(&self, y: i32) -> i32 {
        let cell = scale_dpi(DY_BLK_96);
        if cell <= 0 {
            return 0;
        }
        (y - (scale_dpi(DY_GRID_OFF_96) - cell)) / cell
    }

    /* Event Handlers */

    /// Hooks the window messages to their respective handlers.
    fn events(&self) {
        self.wnd.on().wm_create({
            let self2 = self.clone();
            move |create| -> winsafe::AnyResult<i32> {
                // Sync global DPI state to the actual monitor DPI where the window was created.
                let dpi = self2.wnd.hwnd().GetDpiForWindow();
                UI_DPI.store(if dpi == 0 { BASE_DPI } else { dpi }, Ordering::Relaxed);

                // Ensure the client area matches the board size for the active DPI.
                self2.adjust_window(AdjustFlag::ResizeAndRedraw)?;

                // Initialize local resources.
                self2.state.write().init_game(self2.wnd.hwnd())?;

                // Apply menu visibility and start the game.
                let f_menu = { preferences_mutex().menu_mode };
                self2.set_menu_bar(f_menu)?;
                self2.start_game()?;

                unsafe { self2.wnd.hwnd().DefWindowProc(create) };
                Ok(0)
            }
        });

        self.wnd.on().wm(WM::DPICHANGED, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                // wParam: new DPI in LOWORD/HIWORD (X/Y). lParam: suggested new window rect.
                let dpi = (msg.wparam) & 0xFFFF;
                if dpi > 0 {
                    UI_DPI.store(dpi as u32, Ordering::Relaxed);
                }
                let suggested = unsafe { (msg.lparam as *const RECT).as_ref() };

                if let Some(rc) = suggested {
                    // Persist the suggested top-left so adjust_window keeps us on the same monitor
                    {
                        let mut prefs = preferences_mutex();
                        prefs.wnd_x_pos = rc.left;
                        prefs.wnd_y_pos = rc.top;
                    }

                    let width = max(0, rc.right - rc.left);
                    let height = max(0, rc.bottom - rc.top);
                    self2.wnd.hwnd().MoveWindow(
                        POINT {
                            x: rc.left,
                            y: rc.top,
                        },
                        SIZE {
                            cx: width,
                            cy: height,
                        },
                        true,
                    )?;
                }

                // Our block + face-button bitmaps are cached pre-scaled, so they must be rebuilt after a DPI transition.
                if let Err(e) = load_bitmaps(self2.wnd.hwnd()) {
                    eprintln!("Failed to reload bitmaps after DPI change: {e}");
                }

                self2.adjust_window(AdjustFlag::ResizeAndRedraw)?;
                Ok(0)
            }
        });

        self.wnd.on().wm_window_pos_changed({
            let self2 = self.clone();
            move |wnd_pos| {
                self2.handle_window_pos_changed(wnd_pos.windowpos);
                unsafe { self2.wnd.hwnd().DefWindowProc(wnd_pos) };
                Ok(())
            }
        });

        self.wnd.on().wm_sys_command({
            let self2 = self.clone();
            move |msg| {
                self2.handle_syscommand(msg.request);
                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(())
            }
        });

        // Handle `WM_APP` requests posted from non-UI modules.
        self.wnd.on().wm(WM::APP, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                if msg.wparam == NEW_RECORD_DLG {
                    EnterDialog::new().show_modal(&self2.wnd);
                    UPDATE_INI.store(true, Ordering::Relaxed);
                    BestDialog::new().show_modal(&self2.wnd);
                    return Ok(0);
                }

                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(0)
            }
        });

        self.wnd.on().wm_key_down({
            let self2 = self.clone();
            move |key| {
                self2.handle_keydown(key.vkey_code)?;
                unsafe { self2.wnd.hwnd().DefWindowProc(key) };
                Ok(())
            }
        });

        self.wnd.on().wm_destroy({
            let self2 = self.clone();
            move || {
                // Stop the timer if it is still running
                self2.wnd.hwnd().KillTimer(ID_TIMER)?;

                // Write preferences if they have changed
                // TODO: The original code has a bug where the current window position is not saved on exit
                // unless another preference has changed. Fix this.
                if UPDATE_INI.load(Ordering::Relaxed) {
                    write_preferences()?;
                }

                unsafe { self2.wnd.hwnd().DefWindowProc(Destroy {}) };
                Ok(())
            }
        });

        self.wnd.on().wm_mouse_move({
            let self2 = self.clone();
            move |msg| {
                self2.handle_mouse_move(msg.vkey_code, msg.coords)?;
                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_down({
            let self2 = self.clone();
            move |r_btn| {
                self2.handle_rbutton_down(r_btn.vkey_code, r_btn.coords)?;
                unsafe { self2.wnd.hwnd().DefWindowProc(r_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_dbl_clk({
            let self2 = self.clone();
            move |r_btn| {
                self2.handle_rbutton_down(r_btn.vkey_code, r_btn.coords)?;
                unsafe { self2.wnd.hwnd().DefWindowProc(r_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_up({
            let self2 = self.clone();
            move |r_btn| {
                // If the right button is released while the left button is down, finish the drag operation
                // This replicates the original behavior, though it does add some complexity.
                if r_btn.vkey_code & MK::LBUTTON == MK::LBUTTON {
                    self2.finish_primary_button_drag()?;
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(r_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_m_button_down({
            let self2 = self.clone();
            move |m_btn| {
                // Ignore middle-clicks if the next click is to be ignored
                if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) {
                    return Ok(());
                }

                if m_btn.vkey_code.has(MK::MBUTTON) {
                    // If the middle button is pressed, start a chord operation
                    // However, if a chord is already active, end the chord instead
                    self2.state.write().chord_active = !self2.state.read().chord_active;
                }
                if self2.state.read().game_status.contains(StatusFlag::Play) {
                    self2.begin_primary_button_drag()?;
                    self2.handle_mouse_move(m_btn.vkey_code, m_btn.coords)?;
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(m_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_m_button_up({
            let self2 = self.clone();
            move |m_btn| {
                self2.finish_primary_button_drag()?;
                unsafe { self2.wnd.hwnd().DefWindowProc(m_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_l_button_down({
            let self2 = self.clone();
            move |l_btn| {
                if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) {
                    return Ok(());
                }
                // TODO: This logic can be simplified
                if self2.btn_click_handler(l_btn.coords)? {
                    return Ok(());
                }
                // If the right button or the shift key is also down, start a chord operation
                if l_btn.vkey_code.has(MK::RBUTTON) || l_btn.vkey_code.has(MK::SHIFT) {
                    self2.state.write().chord_active = true;
                }
                if self2.state.read().game_status.contains(StatusFlag::Play) {
                    self2.begin_primary_button_drag()?;
                    self2.handle_mouse_move(l_btn.vkey_code, l_btn.coords)?;
                }
                Ok(())
            }
        });

        self.wnd.on().wm_l_button_up({
            let self2 = self.clone();
            move |l_btn| {
                self2.finish_primary_button_drag()?;
                unsafe { self2.wnd.hwnd().DefWindowProc(l_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_activate({
            let self2 = self.clone();
            move |activate| {
                if activate.event == WA::CLICKACTIVE {
                    IGNORE_NEXT_CLICK.store(true, Ordering::Relaxed);
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(activate) };
                Ok(())
            }
        });

        self.wnd.on().wm_timer(ID_TIMER, {
            let self2 = self.clone();
            move || {
                self2.state.write().do_timer(self2.wnd.hwnd())?;
                Ok(())
            }
        });

        self.wnd.on().wm_paint({
            let self2 = self.clone();
            move || {
                let _paint_guard = self2.wnd.hwnd().BeginPaint()?;
                draw_screen(&self2.wnd.hwnd().GetDC()?, &self2.state.read())?;
                Ok(())
            }
        });

        /* Menu Commands */

        self.wnd.on().wm_command_acc_menu(MenuCommand::New as u16, {
            let self2 = self.clone();
            move || {
                self2.start_game()?;
                Ok(())
            }
        });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Exit as u16, {
                let self2 = self.clone();
                move || {
                    self2.wnd.hwnd().ShowWindow(SW::HIDE);
                    self2.wnd.close();
                    Ok(())
                }
            });

        // Function to be shared between difficulty menu commands
        let difficulty_command = {
            let self2 = self.clone();
            move |command: MenuCommand| {
                let game = match command {
                    MenuCommand::Begin => GameType::Begin,
                    MenuCommand::Inter => GameType::Inter,
                    MenuCommand::Expert => GameType::Expert,
                    _ => GameType::Other,
                };

                let f_menu = {
                    let mut prefs = preferences_mutex();
                    if let Some(data) = game.preset_data() {
                        prefs.game_type = game;
                        prefs.mines = data.0;
                        prefs.height = data.1 as i32;
                        prefs.width = data.2 as i32;
                    }
                    prefs.menu_mode
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                self2.set_menu_bar(f_menu)?;
                self2.start_game()?;
                Ok(())
            }
        };

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Begin as u16, {
                let difficulty_command = difficulty_command.clone();
                move || difficulty_command.clone()(MenuCommand::Begin)
            });
        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Inter as u16, {
                let difficulty_command = difficulty_command.clone();
                move || difficulty_command.clone()(MenuCommand::Inter)
            });
        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Expert as u16, {
                let difficulty_command = difficulty_command.clone();
                move || difficulty_command.clone()(MenuCommand::Expert)
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Custom as u16, {
                let self2 = self.clone();
                move || {
                    // TODO: The way that the preferences dialog is handled causes a custom game to always
                    // be started when the dialog is closed, even if the user clicked "Cancel". Fix

                    // Show the preferences dialog
                    PrefDialog::new().show_modal(&self2.wnd);

                    let fmenu = {
                        let mut prefs = preferences_mutex();
                        prefs.game_type = GameType::Other;
                        prefs.menu_mode
                    };
                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self2.set_menu_bar(fmenu)?;
                    self2.start_game()?;
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Sound as u16, {
                let self2 = self.clone();
                move || {
                    let new_sound = match preferences_mutex().sound_enabled {
                        true => {
                            Sound::stop_all();
                            false
                        }
                        false => Sound::init(),
                    };
                    let f_menu = {
                        let mut prefs = preferences_mutex();
                        prefs.sound_enabled = new_sound;
                        prefs.menu_mode
                    };
                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self2.set_menu_bar(f_menu)?;
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Color as u16, {
                let self2 = self.clone();
                move || {
                    let f_menu = {
                        let mut prefs = preferences_mutex();
                        prefs.color = !prefs.color;
                        prefs.menu_mode
                    };

                    if let Err(e) = load_bitmaps(self2.wnd.hwnd()) {
                        eprintln!("Failed to reload bitmaps: {e}");
                    }

                    // Repaint immediately so toggling color off updates without restarting.
                    draw_screen(&self2.wnd.hwnd().GetDC()?, &self2.state.read())?;
                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self2.set_menu_bar(f_menu)?;
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Mark as u16, {
                let self2 = self.clone();
                move || {
                    let f_menu = {
                        let mut prefs = preferences_mutex();
                        prefs.mark_enabled = !prefs.mark_enabled;
                        prefs.menu_mode
                    };
                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self2.set_menu_bar(f_menu)?;
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Best as u16, {
                let self2 = self.clone();
                move || {
                    BestDialog::new().show_modal(&self2.wnd);
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::Help as u16, {
                let self2 = self.clone();
                move || {
                    do_help(self2.wnd.hwnd(), HELPW::INDEX, HH_DISPLAY_TOPIC as u32);
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::SearchHelp as u16, {
                let self2 = self.clone();
                move || {
                    do_help(self2.wnd.hwnd(), HELPW::CONTEXT, HH_DISPLAY_INDEX as u32);
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::UsingHelp as u16, {
                let self2 = self.clone();
                move || {
                    do_help(self2.wnd.hwnd(), HELPW::HELPONHELP, HH_DISPLAY_TOPIC as u32);
                    Ok(())
                }
            });

        self.wnd
            .on()
            .wm_command_acc_menu(MenuCommand::AboutMinesweeper as u16, {
                let self2 = self.clone();
                move || {
                    let icon = self2
                        .wnd
                        .hwnd()
                        .hinstance()
                        .LoadIcon(IdIdiStr::Id(IconId::Main as u16))?;

                    self2.wnd.hwnd().ShellAbout(
                        MSG_VERSION_NAME,
                        None,
                        Some(MSG_CREDIT),
                        icon.as_opt(),
                    )?;
                    Ok(())
                }
            });
    }
}

/// Runs the WinMine application.
/// # Arguments
/// * `h_instance`: The application instance handle.
/// # Returns
/// Ok(()) on success, or an error on failure.
pub fn run_winmine(hinst: &HINSTANCE) -> Result<(), Box<dyn core::error::Error>> {
    // Seed the RNG, initialize global values, and ensure the preferences registry key exists
    init_const();

    // Initialize DPI to 96 (default) before creating the window
    UI_DPI.store(96, Ordering::Relaxed);

    // Initialize common controls
    let mut icc = INITCOMMONCONTROLSEX::default();
    icc.icc = ICC::ANIMATE_CLASS
        | ICC::BAR_CLASSES
        | ICC::COOL_CLASSES
        | ICC::HOTKEY_CLASS
        | ICC::LISTVIEW_CLASSES
        | ICC::PAGESCROLLER_CLASS
        | ICC::PROGRESS_CLASS
        | ICC::TAB_CLASSES
        | ICC::UPDOWN_CLASS
        | ICC::USEREX_CLASSES;
    InitCommonControlsEx(&icc)?;

    // Get a handle to the menu resource
    let mut menu = hinst.LoadMenu(IdStr::Id(MenuResourceId::Menu as u16))?;

    // Get a handle to the accelerators resource
    let h_accel = hinst.LoadAccelerators(IdStr::Id(MenuResourceId::Accelerators as u16))?;

    // Read user preferences into the global state
    read_preferences()?;

    let dx_window = WINDOW_WIDTH.load(Ordering::Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Ordering::Relaxed);

    // Create the main application window
    let wnd = gui::WindowMain::new(gui::WindowMainOpts {
        class_name: GAME_NAME,
        title: GAME_NAME,
        class_icon: gui::Icon::Id(IconId::Main as u16),
        class_cursor: gui::Cursor::Idc(IDC::ARROW),
        class_bg_brush: gui::Brush::Handle(HBRUSH::GetStockObject(STOCK_BRUSH::LTGRAY)?),
        size: (dx_window, dy_window),
        style: WS::OVERLAPPED | WS::MINIMIZEBOX | WS::CAPTION | WS::SYSMENU,
        menu: menu.leak(),
        accel_table: Some(h_accel),
        ..Default::default()
    });

    // Create the main application state
    let app = WinMineMainWindow::new(wnd);

    // Run the main application window, blocking until exit
    match app.wnd.run_main(None) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Unhandled error during main window execution: {e}").into()),
    }
}

/// Struct containing the state shared by the Preferences dialog
#[derive(Clone)]
struct PrefDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
}

impl PrefDialog {
    /// Creates a new Preferences dialog instance and sets up event handlers.
    fn new() -> Self {
        let dlg = gui::WindowModal::new_dlg(DialogTemplateId::Pref as u16);
        let new_self = Self { dlg };
        new_self.events();
        new_self
    }

    /// Displays the Preferences dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) {
        if let Err(e) = self.dlg.show_modal(parent) {
            eprintln!("Failed to show preferences dialog: {e}");
        }
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let dlg = self.dlg.clone();
            move |_| -> AnyResult<bool> {
                // Get current board settings from preferences
                let (height, width, mines) = {
                    let prefs = preferences_mutex();
                    (prefs.height, prefs.width, prefs.mines)
                };

                // Populate the dialog controls with the current settings
                dlg.hwnd()
                    .GetDlgItem(ControlId::EditHeight as u16)
                    .and_then(|edit| edit.SetWindowText(&height.to_string()))?;
                dlg.hwnd()
                    .GetDlgItem(ControlId::EditWidth as u16)
                    .and_then(|edit| edit.SetWindowText(&width.to_string()))?;
                dlg.hwnd()
                    .GetDlgItem(ControlId::EditMines as u16)
                    .and_then(|edit| edit.SetWindowText(&mines.to_string()))?;

                Ok(true)
            }
        });

        self.dlg.on().wm_command(DLGID::OK.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Retrieve and validate user input from the dialog controls
                let height = get_dlg_int(dlg.hwnd(), ControlId::EditHeight as i32, MINHEIGHT, 24)?;
                let width = get_dlg_int(dlg.hwnd(), ControlId::EditWidth as i32, MINWIDTH, 30)?;
                let max_mines = min(999, (height - 1) * (width - 1));
                let mines = get_dlg_int(dlg.hwnd(), ControlId::EditMines as i32, 10, max_mines)?;

                // Update preferences with the new settings
                let mut prefs = preferences_mutex();
                prefs.height = height as i32;
                prefs.width = width as i32;
                prefs.mines = mines as i16;

                // Close the dialog
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Close the dialog without saving changes
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_help({
            move |help| {
                apply_help_from_info(help.helpinfo, &PREF_HELP_IDS);
                Ok(())
            }
        });

        // TODO: WinSafe's wm_context_menu doesn't have any arguments, that might be a bug
        self.dlg.on().wm(WM::CONTEXTMENU, {
            move |msg: WndMsg| -> AnyResult<isize> {
                let target = unsafe { HWND::from_ptr(msg.wparam as *mut c_void) };
                apply_help_to_control(&target, &PREF_HELP_IDS);
                Ok(1)
            }
        });
    }
}

/// Best times dialog
#[derive(Clone)]
struct BestDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
}

impl BestDialog {
    /// Creates a new BestDialog instance and sets up event handlers.
    /// # Returns
    /// A new BestDialog instance.
    fn new() -> Self {
        let dlg = gui::WindowModal::new_dlg(DialogTemplateId::Best as u16);
        let new_self = Self { dlg };
        new_self.events();
        new_self
    }

    /// Displays the best-times dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) {
        if let Err(e) = self.dlg.show_modal(parent) {
            eprintln!("Failed to show best-times dialog: {e}");
        }
    }

    /* Helper Functions */

    /// Resets the best scores dialog with the provided times and names.
    /// # Arguments
    /// * `time_begin` - The best time for the beginner level.
    /// * `time_inter` - The best time for the intermediate level.
    /// * `time_expert` - The best time for the expert level.
    /// * `name_begin` - The name associated with the beginner level best time.
    /// * `name_inter` - The name associated with the intermediate level best time.
    /// * `name_expert` - The name associated with the expert level best time.
    /// # Returns
    /// A `Result` indicating success or failure.
    fn reset_best_dialog(
        &self,
        time_begin: u16,
        time_inter: u16,
        time_expert: u16,
        name_begin: &str,
        name_inter: &str,
        name_expert: &str,
    ) -> AnyResult<()> {
        // Set the beginner time and name
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeBegin as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_begin} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeBegin as u16 + 1)
            .and_then(|hwnd| hwnd.SetWindowText(name_begin))?;

        // Set the intermediate time and name
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeInter as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_inter} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeInter as u16 + 1)
            .and_then(|hwnd| hwnd.SetWindowText(name_inter))?;

        // Set the expert time and name
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeExpert as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_expert} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ControlId::TimeExpert as u16 + 1)
            .and_then(|hwnd| hwnd.SetWindowText(name_expert))?;

        Ok(())
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let self2 = self.clone();
            move |_| -> AnyResult<bool> {
                let prefs = preferences_mutex();
                self2.reset_best_dialog(
                    prefs.best_times[GameType::Begin as usize],
                    prefs.best_times[GameType::Inter as usize],
                    prefs.best_times[GameType::Expert as usize],
                    &prefs.beginner_name,
                    &prefs.inter_name,
                    &prefs.expert_name,
                )?;

                Ok(true)
            }
        });

        self.dlg
            .on()
            .wm_command(ControlId::BtnReset as u16, BN::CLICKED, {
                let self2 = self.clone();
                move || -> AnyResult<()> {
                    // Set best times and names to defaults
                    {
                        let mut prefs = preferences_mutex();

                        // Set all best times to 999 seconds
                        prefs.best_times[GameType::Begin as usize] = 999;
                        prefs.best_times[GameType::Inter as usize] = 999;
                        prefs.best_times[GameType::Expert as usize] = 999;

                        // Set the three best names to the default values
                        prefs.beginner_name = DEFAULT_PLAYER_NAME.to_string();
                        prefs.inter_name = DEFAULT_PLAYER_NAME.to_string();
                        prefs.expert_name = DEFAULT_PLAYER_NAME.to_string();
                    };

                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self2.reset_best_dialog(
                        999,
                        999,
                        999,
                        DEFAULT_PLAYER_NAME,
                        DEFAULT_PLAYER_NAME,
                        DEFAULT_PLAYER_NAME,
                    )?;
                    Ok(())
                }
            });

        self.dlg.on().wm_command(DLGID::OK.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_help({
            move |help| {
                apply_help_from_info(help.helpinfo, &BEST_HELP_IDS);
                Ok(())
            }
        });

        self.dlg.on().wm(WM::CONTEXTMENU, {
            move |msg: WndMsg| -> AnyResult<isize> {
                let target = unsafe { HWND::from_ptr(msg.wparam as *mut c_void) };
                apply_help_to_control(&target, &BEST_HELP_IDS);
                Ok(1)
            }
        });
    }
}

/// New record name entry dialog
#[derive(Clone)]
struct EnterDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
}

impl EnterDialog {
    /// Creates a new EnterDialog instance and sets up event handlers.
    /// # Returns
    /// A new EnterDialog instance.
    fn new() -> Self {
        let dlg = gui::WindowModal::new_dlg(DialogTemplateId::Enter as u16);
        let new_self = Self { dlg };
        new_self.events();
        new_self
    }

    /// Displays the name entry dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) {
        if let Err(e) = self.dlg.show_modal(parent) {
            eprintln!("Failed to show name-entry dialog: {e}");
        }
    }

    /// Saves the entered high-score name to preferences.
    /// # Returns
    /// A `Result` indicating success or failure.
    fn save_high_score_name(&self) -> AnyResult<()> {
        // Retrieve the entered name from the dialog's edit control
        let new_name = self
            .dlg
            .hwnd()
            .GetDlgItem(ControlId::EditName as u16)
            .and_then(|edit_hwnd| edit_hwnd.GetWindowText())?;

        let mut prefs = preferences_mutex();
        match prefs.game_type {
            GameType::Begin => prefs.beginner_name = new_name,
            GameType::Inter => prefs.inter_name = new_name,
            GameType::Expert => prefs.expert_name = new_name,
            // Unreachable
            GameType::Other => {}
        }
        Ok(())
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let dlg = self.dlg.clone();
            move |_| -> AnyResult<bool> {
                let (game_type, current_name) = {
                    let prefs = preferences_mutex();
                    let name = match prefs.game_type {
                        GameType::Begin => prefs.beginner_name.clone(),
                        GameType::Inter => prefs.inter_name.clone(),
                        GameType::Expert => prefs.expert_name.clone(),
                        // Unreachable
                        GameType::Other => "".to_string(),
                    };
                    (prefs.game_type, name)
                };

                if let Ok(best_hwnd) = dlg.hwnd().GetDlgItem(ControlId::TextBest as u16) {
                    let string = match game_type {
                        GameType::Begin => MSG_FASTEST_BEGINNER,
                        GameType::Inter => MSG_FASTEST_INTERMEDIATE,
                        GameType::Expert => MSG_FASTEST_EXPERT,
                        // Unreachable
                        GameType::Other => "",
                    };

                    best_hwnd.SetWindowText(string)?;
                }

                if let Ok(edit_hwnd) = dlg.hwnd().GetDlgItem(ControlId::EditName as u16) {
                    unsafe {
                        edit_hwnd.SendMessage(SetLimitText {
                            max_chars: Some(CCH_NAME_MAX as u32),
                        });
                    };

                    edit_hwnd.SetWindowText(&current_name)?;
                }

                Ok(true)
            }
        });

        self.dlg
            .on()
            .wm_command(ControlId::BtnOk as u16, BN::CLICKED, {
                let self2 = self.clone();
                move || -> AnyResult<()> {
                    self2.save_high_score_name()?;
                    self2.dlg.hwnd().EndDialog(1)?;
                    Ok(())
                }
            });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let self2 = self.clone();
            move || -> AnyResult<()> {
                self2.save_high_score_name()?;
                self2.dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });
    }
}

/// Applies help context based on the HELPINFO structure pointed to by `l_param`.
/// # Arguments
/// * `l_param` - The LPARAM containing a pointer to the HELPINFO structure.
/// * `ids` - The array of help context IDs.
/// # Returns
/// True if help was applied, false otherwise.
fn apply_help_from_info(help: &HELPINFO, ids: &[u32]) {
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
/// TODO: Move help stuff into its own module.
/// # Arguments
/// * `hwnd` - The handle to the control.
/// * `ids` - The array of help context IDs.
fn apply_help_to_control(hwnd: &HWND, ids: &[u32]) {
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
