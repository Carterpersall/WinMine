//! Main window and event handling for the Minesweeper game.

use core::cmp::{max, min};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Data::HtmlHelp::{
    HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC, HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HtmlHelpA,
};

use winsafe::co::{
    BN, DLGID, GWLP, HELPW, ICC, IDC, MK, PM, SC, SM, STOCK_BRUSH, SW, VK, WA, WM, WS, WS_EX,
};
use winsafe::msg::{WndMsg, em::SetLimitText, wm::Destroy};
use winsafe::{
    AdjustWindowRectExForDpi, AnyResult, GetSystemMetrics, HBRUSH, HELPINFO, HINSTANCE, HMENU,
    HWND, INITCOMMONCONTROLSEX, IdStr, InitCommonControlsEx, MSG, POINT, PeekMessage, PtsRc, RECT,
    SIZE, WINDOWPOS, gui, prelude::*,
};

use crate::globals::{
    BASE_DPI, BLK_BTN_INPUT, CXBORDER, CYCAPTION, CYMENU, DEFAULT_PLAYER_NAME, GAME_NAME,
    GAME_STATUS, IGNORE_NEXT_CLICK, INIT_MINIMIZED, LEFT_CLK_DOWN, MSG_FASTEST_BEGINNER,
    MSG_FASTEST_EXPERT, MSG_FASTEST_INTERMEDIATE, StatusFlag, TIME_FORMAT, UI_DPI, WINDOW_HEIGHT,
    WINDOW_WIDTH, WND_Y_OFFSET, update_ui_metrics_for_dpi,
};
use crate::grafix::{
    ButtonSprite, DX_BLK_96, DX_BUTTON_96, DX_LEFT_SPACE_96, DX_RIGHT_SPACE_96, DY_BLK_96,
    DY_BOTTOM_SPACE_96, DY_BUTTON_96, DY_GRID_OFF_96, DY_TOP_LED_96, display_button, draw_screen,
    init_game, load_bitmaps, scale_dpi,
};
use crate::pref::{
    CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH, MenuMode, SoundState, read_preferences,
    write_preferences,
};
use crate::rtns::{
    AdjustFlag, BOARD_HEIGHT, BOARD_WIDTH, BTN_FACE_STATE, CURSOR_X_POS, CURSOR_Y_POS, ID_TIMER,
    do_button_1_up, do_timer, make_guess, pause_game, preferences_mutex, resume_game, track_mouse,
};
use crate::sound::{init_sound, stop_all_sounds};
use crate::util::{IconId, do_about, do_help, get_dlg_int, init_const};

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
    /// Show "How to play" help.
    HowToPlay = 591,
    /// Open the help-about-help entry.
    HelpHelp = 592,
    /// Show the About dialog.
    HelpAbout = 593,
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
            591 => Ok(MenuCommand::HowToPlay),
            592 => Ok(MenuCommand::HelpHelp),
            593 => Ok(MenuCommand::HelpAbout),
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
}

impl WinMineMainWindow {
    /// Creates the main window and hooks its events.
    /// # Arguments
    /// * `wnd`: The main window to wrap.
    /// # Returns
    /// The wrapped main window with events hooked.
    fn new(wnd: gui::WindowMain) -> Self {
        let new_self = Self { wnd };
        new_self.events();
        new_self
    }

    /* Message Helper Functions */

    /// Begins a primary button drag operation.
    fn begin_primary_button_drag(&self) -> AnyResult<()> {
        LEFT_CLK_DOWN.store(true, Ordering::Relaxed);
        CURSOR_X_POS.store(-1, Ordering::Relaxed);
        CURSOR_Y_POS.store(-1, Ordering::Relaxed);
        display_button(self.wnd.hwnd(), ButtonSprite::Caution)
    }

    /// Finishes a primary button drag operation.
    /// # Returns
    /// A `Result` indicating success or failure.
    fn finish_primary_button_drag(&self) -> AnyResult<()> {
        LEFT_CLK_DOWN.store(false, Ordering::Relaxed);
        if status_play() {
            do_button_1_up(self.wnd.hwnd())?;
        } else {
            track_mouse(self.wnd.hwnd(), -2, -2);
        }
        Ok(())
    }

    /// Handles the `WM_KEYDOWN` message.
    /// # Arguments
    /// * `key`: The virtual key code of the key that was pressed.
    fn handle_keydown(&self, key: VK) {
        match key {
            code if code == VK::F4 => {
                let current_sound = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.sound_state
                };

                if matches!(current_sound, SoundState::On | SoundState::Off) {
                    let new_sound = match current_sound {
                        SoundState::On => {
                            stop_all_sounds();
                            SoundState::Off
                        }
                        SoundState::Off => init_sound(),
                    };

                    let f_menu = {
                        let mut prefs = match preferences_mutex().lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        prefs.sound_state = new_sound;
                        prefs.menu_mode
                    };

                    UPDATE_INI.store(true, Ordering::Relaxed);
                    self.set_menu_bar(f_menu);
                }
            }
            code if code == VK::F5 => {
                let menu_value = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.menu_mode
                };

                if !matches!(menu_value, MenuMode::AlwaysOn) {
                    self.set_menu_bar(MenuMode::Hidden);
                }
            }
            code if code == VK::F6 => {
                let menu_value = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.menu_mode
                };

                if !matches!(menu_value, MenuMode::AlwaysOn) {
                    self.set_menu_bar(MenuMode::On);
                }
            }
            code if code == VK::SHIFT => self.handle_xyzzys_shift(),
            _ => self.handle_xyzzys_default_key(key),
        }
    }

    /// Handles mouse move events.
    /// # Arguments
    /// * `key`: The mouse buttons currently pressed.
    /// * `point`: The coordinates of the mouse cursor.
    fn handle_mouse_move(&self, key: MK, point: POINT) -> AnyResult<()> {
        if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
            // If the left button is down, the user is dragging
            if status_play() {
                track_mouse(
                    self.wnd.hwnd(),
                    self.x_box_from_xpos(point.x),
                    self.y_box_from_ypos(point.y),
                );
            } else {
                self.finish_primary_button_drag()?;
            }
        } else {
            // Regular mouse move
            self.handle_xyzzys_mouse(key, point);
        }
        Ok(())
    }

    /// Handles right mouse button down events.
    /// # Arguments
    /// * `btn`: The mouse button that was pressed.
    /// * `point`: The coordinates of the mouse cursor.
    fn handle_rbutton_down(&self, btn: MK, point: POINT) -> AnyResult<()> {
        // Ignore right-clicks if the next click is set to be ignored
        if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) || !status_play() {
            return Ok(());
        }

        if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
            track_mouse(self.wnd.hwnd(), -3, -3);
            set_block_flag(true);
            unsafe {
                // TODO: Change this
                let _ = self.wnd.hwnd().PostMessage(WndMsg::new(
                    WM::MOUSEMOVE,
                    btn.raw() as usize,
                    point.x as isize | ((point.y as isize) << 16),
                ));
            }
            return Ok(());
        }

        if btn == MK::LBUTTON {
            self.begin_primary_button_drag()?;
            self.handle_mouse_move(btn, point)?;
            return Ok(());
        }

        // Regular right-click: make a guess
        make_guess(
            self.wnd.hwnd(),
            self.x_box_from_xpos(point.x),
            self.y_box_from_ypos(point.y),
        )?;
        Ok(())
    }

    /// Handles the `WM_SYSCOMMAND` message for minimize and restore events.
    /// # Arguments
    /// * `command` - The system command identifier.
    fn handle_syscommand(&self, command: SC) {
        // Isolate the system command identifier by masking out the lower 4 bits.
        //let command = (sys_cmd & 0xFFF0) as u32;
        if command == SC::MINIMIZE {
            pause_game();
            set_status_pause();
            set_status_icon();
        } else if command == SC::RESTORE {
            clr_status_pause();
            clr_status_icon();
            resume_game();
            IGNORE_NEXT_CLICK.store(false, Ordering::Relaxed);
        }
    }

    /// Handles the `WM_WINDOWPOSCHANGED` message to store the new window position in preferences.
    /// # Arguments
    /// * `pos` - A reference to the `WINDOWPOS` structure containing the new window position.
    fn handle_window_pos_changed(&self, pos: &WINDOWPOS) {
        if status_icon() {
            return;
        }

        let mut prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.wnd_x_pos = pos.x;
        prefs.wnd_y_pos = pos.y;
    }

    /// Handles command messages from the menu and accelerators.
    /// # Arguments
    /// * `w_param`: The wParam from the `WM_COMMAND` message.
    /// # Returns
    /// `Some(isize)` if the command was handled, `None` otherwise.
    /// # Notes
    /// This function returns an `Option` to indicate whether the command was handled.
    /// If the command was handled, it returns `Some(0)` to indicate success. If the command was not handled,
    /// it returns `None` to allow the default handler to process it. This means that there is no need to return
    /// a `Result` type with an error, as unhandled commands are simply passed to the default handler.
    fn handle_command(&self, w_param: usize) -> Option<isize> {
        match MenuCommand::try_from(w_param) {
            Ok(MenuCommand::New) => self.start_game(),
            Ok(MenuCommand::Exit) => {
                self.wnd.hwnd().ShowWindow(SW::HIDE);
                unsafe {
                    let _ = self.wnd.hwnd().SendMessage(WndMsg::new(
                        WM::SYSCOMMAND,
                        SC::CLOSE.raw() as usize,
                        0,
                    ));
                }
            }
            Ok(command @ (MenuCommand::Begin | MenuCommand::Inter | MenuCommand::Expert)) => {
                let game = match command {
                    MenuCommand::Begin => GameType::Begin,
                    MenuCommand::Inter => GameType::Inter,
                    MenuCommand::Expert => GameType::Expert,
                    _ => GameType::Other,
                };

                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if let Some(data) = game.preset_data() {
                        prefs.game_type = game;
                        prefs.mines = data.0;
                        prefs.height = data.1 as i32;
                        prefs.width = data.2 as i32;
                    }
                    prefs.menu_mode
                };
                self.start_game();
                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(f_menu);
            }
            Ok(MenuCommand::Custom) => {
                // TODO: The way that the preferences dialog is handled causes a custom game to always
                // be started when the dialog is closed, even if the user clicked "Cancel". Fix this.

                // Show the preferences dialog
                PrefDialog::new().show_modal(&self.wnd);

                let fmenu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.game_type = GameType::Other;
                    prefs.menu_mode
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(fmenu);
                self.start_game();
            }
            Ok(MenuCommand::Sound) => {
                let current_sound = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.sound_state
                };
                let new_sound = match current_sound {
                    SoundState::On => {
                        stop_all_sounds();
                        SoundState::Off
                    }
                    SoundState::Off => init_sound(),
                };
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.sound_state = new_sound;
                    prefs.menu_mode
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(f_menu);
            }
            Ok(MenuCommand::Color) => {
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.color = !prefs.color;
                    prefs.menu_mode
                };

                if let Err(e) = load_bitmaps(self.wnd.hwnd()) {
                    eprintln!("Failed to reload bitmaps: {e}");
                }

                // Repaint immediately so toggling color off updates without restarting.
                if let Ok(hdc) = self.wnd.hwnd().GetDC() {
                    // TODO: Handle properly after moving `handle_command` into separate closures
                    draw_screen(&hdc).unwrap();
                }
                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(f_menu);
            }
            Ok(MenuCommand::Mark) => {
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.mark_enabled = !prefs.mark_enabled;
                    prefs.menu_mode
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                self.set_menu_bar(f_menu);
            }
            Ok(MenuCommand::Best) => BestDialog::new().show_modal(&self.wnd),
            Ok(MenuCommand::Help) => {
                do_help(self.wnd.hwnd(), HELPW::INDEX, HH_DISPLAY_TOPIC as u32);
            }
            Ok(MenuCommand::HowToPlay) => {
                do_help(self.wnd.hwnd(), HELPW::CONTEXT, HH_DISPLAY_INDEX as u32);
            }
            Ok(MenuCommand::HelpHelp) => {
                do_help(self.wnd.hwnd(), HELPW::HELPONHELP, HH_DISPLAY_TOPIC as u32);
            }
            Ok(MenuCommand::HelpAbout) => {
                do_about(self.wnd.hwnd());
            }
            Err(_) => return None,
        }

        Some(0)
    }

    /// Handles clicks on the smiley face button.
    /// # Arguments
    /// * `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// True if the click was handled, false otherwise.
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
        let _ = self
            .wnd
            .hwnd()
            .MapWindowPoints(&HWND::NULL, PtsRc::Rc(&mut rc));

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
                            BTN_FACE_STATE.store(ButtonSprite::Happy as u8, Ordering::Relaxed);
                            display_button(self.wnd.hwnd(), ButtonSprite::Happy)?;
                            self.start_game();
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
                            display_button(
                                self.wnd.hwnd(),
                                match BTN_FACE_STATE.load(Ordering::Relaxed) {
                                    0 => ButtonSprite::Happy,
                                    1 => ButtonSprite::Caution,
                                    2 => ButtonSprite::Lose,
                                    3 => ButtonSprite::Win,
                                    _ => ButtonSprite::Down,
                                },
                            )?;
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
    ///
    /// TODO: Make `f_adjust` an enum
    pub fn adjust_window(&self, mut f_adjust: i32) {
        let menu_handle = self.wnd.hwnd().GetMenu().unwrap_or(HMENU::NULL);

        let x_boxes = BOARD_WIDTH.load(Ordering::Relaxed);
        let y_boxes = BOARD_HEIGHT.load(Ordering::Relaxed);
        let dx_window = scale_dpi(DX_BLK_96) * x_boxes
            + scale_dpi(DX_LEFT_SPACE_96)
            + scale_dpi(DX_RIGHT_SPACE_96);
        let dy_window = scale_dpi(DY_BLK_96) * y_boxes
            + scale_dpi(DY_GRID_OFF_96)
            + scale_dpi(DY_BOTTOM_SPACE_96);
        WINDOW_WIDTH.store(dx_window, Ordering::Relaxed);
        WINDOW_HEIGHT.store(dy_window, Ordering::Relaxed);

        let (mut x_window, mut y_window, f_menu) = {
            let prefs = match preferences_mutex().lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            (prefs.wnd_x_pos, prefs.wnd_y_pos, prefs.menu_mode)
        };

        let menu_visible = !matches!(f_menu, MenuMode::Hidden) && menu_handle.as_opt().is_some();
        let mut menu_extra = 0;
        let mut diff_level = false;
        if menu_visible
            && let Some(hwnd) = self.wnd.hwnd().as_opt()
            && let Some(menu) = menu_handle.as_opt()
            && let (Ok(game_rect), Ok(help_rect)) =
                (hwnd.GetMenuItemRect(menu, 0), hwnd.GetMenuItemRect(menu, 1))
            && game_rect.top != help_rect.top
        {
            diff_level = true;
            menu_extra = CYMENU.load(Ordering::Relaxed);
        }

        let desired = RECT {
            left: 0,
            top: 0,
            right: dx_window,
            bottom: dy_window,
        };
        let dw_style = self.wnd.hwnd().GetWindowLongPtr(GWLP::STYLE);
        let dw_ex_style = self.wnd.hwnd().GetWindowLongPtr(GWLP::EXSTYLE);
        let mut frame_extra = CXBORDER.load(Ordering::Relaxed);
        let mut dyp_adjust;
        if let Ok(adjusted) = AdjustWindowRectExForDpi(
            desired,
            unsafe { WS::from_raw(dw_style as _) },
            menu_visible,
            unsafe { WS_EX::from_raw(dw_ex_style as _) },
            UI_DPI.load(Ordering::Relaxed),
        ) {
            let cx_total = adjusted.right - adjusted.left;
            let cy_total = adjusted.bottom - adjusted.top;
            frame_extra = max(0, cx_total - dx_window);
            dyp_adjust = max(0, cy_total - dy_window);
        } else {
            dyp_adjust = CYCAPTION.load(Ordering::Relaxed);
            if menu_visible {
                dyp_adjust += CYMENU.load(Ordering::Relaxed);
            }
        }

        dyp_adjust += menu_extra;
        WND_Y_OFFSET.store(dyp_adjust, Ordering::Relaxed);

        let mut excess = x_window + dx_window + frame_extra - self.get_system_metrics(SM::CXSCREEN);
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize as i32;
            x_window -= excess;
        }
        excess = y_window + dy_window + dyp_adjust - self.get_system_metrics(SM::CYSCREEN);
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize as i32;
            y_window -= excess;
        }

        if !INIT_MINIMIZED.load(Ordering::Relaxed) {
            if (f_adjust & AdjustFlag::Resize as i32) != 0 {
                let _ = self.wnd.hwnd().MoveWindow(
                    POINT {
                        x: x_window,
                        y: y_window,
                    },
                    SIZE {
                        cx: dx_window + frame_extra,
                        cy: dy_window + dyp_adjust,
                    },
                    true,
                );
            }

            if diff_level
                && menu_visible
                && menu_handle.as_opt().is_some()
                && menu_handle
                    .as_opt()
                    .and_then(|menu| {
                        self.wnd
                            .hwnd()
                            .GetMenuItemRect(menu, 0)
                            .ok()
                            .zip(self.wnd.hwnd().GetMenuItemRect(menu, 1).ok())
                    })
                    .is_some_and(|(g, h)| g.top == h.top)
            {
                dyp_adjust -= CYMENU.load(Ordering::Relaxed);
                WND_Y_OFFSET.store(dyp_adjust, Ordering::Relaxed);
                let _ = self.wnd.hwnd().MoveWindow(
                    POINT {
                        x: x_window,
                        y: y_window,
                    },
                    SIZE {
                        cx: dx_window + frame_extra,
                        cy: dy_window + dyp_adjust,
                    },
                    true,
                );
            }

            if (f_adjust & AdjustFlag::Display as i32) != 0 {
                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: dx_window,
                    bottom: dy_window,
                };
                let _ = self.wnd.hwnd().InvalidateRect(Some(&rect), true);
            }
        }

        // TODO: Don't double lock here
        if let Ok(mut prefs) = preferences_mutex().lock() {
            prefs.wnd_x_pos = x_window;
            prefs.wnd_y_pos = y_window;
        } else if let Err(poisoned) = preferences_mutex().lock() {
            let mut guard = poisoned.into_inner();
            guard.wnd_x_pos = x_window;
            guard.wnd_y_pos = y_window;
        }
    }

    /// Retrieves system metrics, favoring virtual screen metrics for multi-monitor support.
    ///
    /// TODO: Is this function necessary? Could just call `GetSystemMetrics` directly where needed,
    /// which is only twice in `adjust_window`.
    /// # Arguments
    /// * `index` - The system metric index to retrieve.
    /// # Returns
    /// The requested system metric value.
    fn get_system_metrics(&self, index: SM) -> i32 {
        match index {
            SM::CXSCREEN => {
                let mut result = GetSystemMetrics(SM::CXVIRTUALSCREEN);
                if result == 0 {
                    result = GetSystemMetrics(SM::CXSCREEN);
                }
                result
            }
            SM::CYSCREEN => {
                let mut result = GetSystemMetrics(SM::CYVIRTUALSCREEN);
                if result == 0 {
                    result = GetSystemMetrics(SM::CYSCREEN);
                }
                result
            }
            _ => GetSystemMetrics(index),
        }
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
                update_ui_metrics_for_dpi(dpi);

                // Ensure the client area matches the board size for the active DPI.
                self2.adjust_window(AdjustFlag::Resize as i32 | AdjustFlag::Display as i32);

                // Initialize local resources.
                init_game(self2.wnd.hwnd())?;

                // Apply menu visibility and start the game.
                let f_menu = {
                    let prefs_guard = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs_guard.menu_mode
                };
                self2.set_menu_bar(f_menu);
                self2.start_game();

                unsafe { self2.wnd.hwnd().DefWindowProc(create) };
                Ok(0)
            }
        });

        // Mark the process as no longer "initially minimized" once shown.
        // TODO: Is this necessary?
        self.wnd.on().wm_show_window({
            move |show_window| {
                if show_window.being_shown {
                    INIT_MINIMIZED.store(false, Ordering::Relaxed);
                }
                Ok(())
            }
        });

        self.wnd.on().wm(WM::DPICHANGED, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                // wParam: new DPI in LOWORD/HIWORD (X/Y). lParam: suggested new window rect.
                let dpi = (msg.wparam) & 0xFFFF;
                if dpi > 0 {
                    UI_DPI.store(dpi as u32, Ordering::Relaxed);
                    update_ui_metrics_for_dpi(dpi as u32);
                }
                let suggested = unsafe { (msg.lparam as *const RECT).as_ref() };

                if let Some(rc) = suggested {
                    // Persist the suggested top-left so adjust_window keeps us on the same monitor
                    {
                        let mut prefs = match preferences_mutex().lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        prefs.wnd_x_pos = rc.left;
                        prefs.wnd_y_pos = rc.top;
                    }

                    let width = max(0, rc.right - rc.left);
                    let height = max(0, rc.bottom - rc.top);
                    let _ = self2.wnd.hwnd().MoveWindow(
                        POINT {
                            x: rc.left,
                            y: rc.top,
                        },
                        SIZE {
                            cx: width,
                            cy: height,
                        },
                        true,
                    );
                }

                // Our block + face-button bitmaps are cached pre-scaled, so they must be rebuilt after a DPI transition.
                if let Err(e) = load_bitmaps(self2.wnd.hwnd()) {
                    eprintln!("Failed to reload bitmaps after DPI change: {e}");
                }

                self2.adjust_window(AdjustFlag::Resize as i32 | AdjustFlag::Display as i32);
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

        // TODO: Move the handle_command logic into separate wm_command closures
        self.wnd.on().wm(WM::COMMAND, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                // If we have a handler for the command, execute it. Otherwise, run the default handler
                if let Some(result) = self2.handle_command(msg.wparam) {
                    return Ok(result);
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(0)
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
                self2.handle_keydown(key.vkey_code);
                unsafe { self2.wnd.hwnd().DefWindowProc(key) };
                Ok(())
            }
        });

        self.wnd.on().wm_destroy({
            let self2 = self.clone();
            move || {
                // Stop the timer if it is still running
                let _ = self2.wnd.hwnd().KillTimer(ID_TIMER);

                // Write preferences if they have changed
                if UPDATE_INI.load(Ordering::Relaxed)
                    && let Err(e) = write_preferences()
                {
                    eprintln!("Failed to write preferences: {e}");
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
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
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

                if status_play() {
                    set_block_flag(true);
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
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                    self2.finish_primary_button_drag()?;
                }
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
                if self2.btn_click_handler(l_btn.coords)? {
                    return Ok(());
                }
                if status_play() {
                    // Mask SHIFT and RBUTTON to indicate a "chord" operation.
                    set_block_flag(l_btn.vkey_code == MK::SHIFT || l_btn.vkey_code == MK::RBUTTON);
                    self2.begin_primary_button_drag()?;
                    self2.handle_mouse_move(l_btn.vkey_code, l_btn.coords)?;
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(l_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_l_button_up({
            let self2 = self.clone();
            move |l_btn| {
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                    self2.finish_primary_button_drag()?;
                }
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
                do_timer(self2.wnd.hwnd());
                Ok(())
            }
        });

        self.wnd.on().wm_paint({
            let self2 = self.clone();
            move || {
                if let Ok(paint_guard) = self2.wnd.hwnd().BeginPaint() {
                    draw_screen(&paint_guard)?;
                }
                Ok(())
            }
        });
    }
}

/// Runs the WinMine application.
/// # Arguments
/// * `h_instance`: The application instance handle.
/// * `n_cmd_show`: The initial window show command.
/// # Returns
/// Ok(()) on success, or an error on failure.
pub fn run_winmine(hinst: &HINSTANCE, n_cmd_show: i32) -> Result<(), Box<dyn core::error::Error>> {
    // Seed the RNG, initialize global values, and ensure the preferences registry key exists
    init_const();

    // Initialize DPI to 96 (default) before creating the window
    UI_DPI.store(96, Ordering::Relaxed);
    update_ui_metrics_for_dpi(UI_DPI.load(Ordering::Relaxed));

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
    read_preferences();

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

    // Determine whether to start minimized
    let cmd_show =
        if n_cmd_show == SW::SHOWMINNOACTIVE.raw() || n_cmd_show == SW::SHOWMINIMIZED.raw() {
            INIT_MINIMIZED.store(true, Ordering::Relaxed);
            Some(SW::SHOWMINIMIZED)
        } else {
            INIT_MINIMIZED.store(false, Ordering::Relaxed);
            Some(SW::SHOWNORMAL)
        };

    // Run the main application window, blocking until exit
    match app.wnd.run_main(cmd_show) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Unhandled error during main window execution: {e}").into()),
    }
}

/// Returns whether the game is currently in the 'icon' (minimized) status.
/// # Returns
/// True if the game is in icon status, false otherwise.
fn status_icon() -> bool {
    GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Icon as i32) != 0
}

/// Returns whether the game is currently in play status.
/// # Returns
/// True if the game is in play status, false otherwise.
fn status_play() -> bool {
    GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32) != 0
}

/// Sets the play status flag.
fn set_status_pause() {
    GAME_STATUS.fetch_or(StatusFlag::Pause as i32, Ordering::Relaxed);
}

/// Clears the pause status flag.
fn clr_status_pause() {
    GAME_STATUS.fetch_and(!(StatusFlag::Pause as i32), Ordering::Relaxed);
}

/// Sets the status icon flag.
fn set_status_icon() {
    GAME_STATUS.fetch_or(StatusFlag::Icon as i32, Ordering::Relaxed);
}

/// Clears the status icon flag.
fn clr_status_icon() {
    GAME_STATUS.fetch_and(!(StatusFlag::Icon as i32), Ordering::Relaxed);
}

/// Sets the block flag indicating whether button input is blocked.
/// # Arguments
/// * `active`: True to block button input, false to allow it.
fn set_block_flag(active: bool) {
    BLK_BTN_INPUT.store(active, Ordering::Relaxed);
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
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    (prefs.height, prefs.width, prefs.mines)
                };

                // Populate the dialog controls with the current settings
                // TODO: Handle errors
                let _ = dlg
                    .hwnd()
                    .GetDlgItem(ControlId::EditHeight as u16)
                    .and_then(|edit| edit.SetWindowText(&height.to_string()));
                let _ = dlg
                    .hwnd()
                    .GetDlgItem(ControlId::EditWidth as u16)
                    .and_then(|edit| edit.SetWindowText(&width.to_string()));
                let _ = dlg
                    .hwnd()
                    .GetDlgItem(ControlId::EditMines as u16)
                    .and_then(|edit| edit.SetWindowText(&mines.to_string()));

                Ok(true)
            }
        });

        self.dlg.on().wm_command(DLGID::OK.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Retrieve and validate user input from the dialog controls
                // TODO: Handle errors properly
                let height = get_dlg_int(dlg.hwnd(), ControlId::EditHeight as i32, MINHEIGHT, 24)
                    .unwrap_or(MINHEIGHT);
                let width = get_dlg_int(dlg.hwnd(), ControlId::EditWidth as i32, MINWIDTH, 30)
                    .unwrap_or(MINWIDTH);
                let max_mines = min(999, (height - 1) * (width - 1));
                let mines = get_dlg_int(dlg.hwnd(), ControlId::EditMines as i32, 10, max_mines)
                    .unwrap_or(10);

                // Update preferences with the new settings
                let mut prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.height = height as i32;
                prefs.width = width as i32;
                prefs.mines = mines as i16;

                // Close the dialog
                let _ = dlg.hwnd().EndDialog(1);
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Close the dialog without saving changes
                let _ = dlg.hwnd().EndDialog(1);
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

    /// Sets the dialog text for a given time and name in the best scores dialog.
    ///
    /// TODO: Remove this function
    /// # Arguments
    /// * `id` - The control ID for the time text.
    /// * `time` - The time value to display.
    /// * `name` - The name associated with the time.
    fn set_dtext(&self, id: i32, time: u16, name: &str) {
        // TODO: Make this better
        let time_fmt = TIME_FORMAT.replace("%d", &time.to_string());

        // TODO: Handle errors
        let _ = self
            .dlg
            .hwnd()
            .GetDlgItem(id as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&time_fmt));
        let _ = self
            .dlg
            .hwnd()
            .GetDlgItem((id + 1) as u16)
            .and_then(|hwnd| hwnd.SetWindowText(name));
    }

    /// Resets the best scores dialog with the provided times and names.
    /// # Arguments
    /// * `time_begin` - The best time for the beginner level.
    /// * `time_inter` - The best time for the intermediate level.
    /// * `time_expert` - The best time for the expert level.
    /// * `name_begin` - The name associated with the beginner level best time.
    /// * `name_inter` - The name associated with the intermediate level best time.
    /// * `name_expert` - The name associated with the expert level best time.
    fn reset_best_dialog(
        &self,
        time_begin: u16,
        time_inter: u16,
        time_expert: u16,
        name_begin: &str,
        name_inter: &str,
        name_expert: &str,
    ) {
        self.set_dtext(ControlId::TimeBegin as i32, time_begin, name_begin);
        self.set_dtext(ControlId::TimeInter as i32, time_inter, name_inter);
        self.set_dtext(ControlId::TimeExpert as i32, time_expert, name_expert);
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let self2 = self.clone();
            move |_| -> AnyResult<bool> {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                self2.reset_best_dialog(
                    prefs.best_times[GameType::Begin as usize],
                    prefs.best_times[GameType::Inter as usize],
                    prefs.best_times[GameType::Expert as usize],
                    &prefs.beginner_name,
                    &prefs.inter_name,
                    &prefs.expert_name,
                );

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
                        let mut prefs = match preferences_mutex().lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };

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
                    );
                    Ok(())
                }
            });

        self.dlg.on().wm_command(DLGID::OK.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                let _ = dlg.hwnd().EndDialog(1);
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                let _ = dlg.hwnd().EndDialog(1);
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
    fn save_high_score_name(&self) {
        // Retrieve the entered name from the dialog's edit control
        // TODO: Handle errors properly
        let new_name = self
            .dlg
            .hwnd()
            .GetDlgItem(ControlId::EditName as u16)
            .and_then(|edit_hwnd| edit_hwnd.GetWindowText())
            .unwrap_or(DEFAULT_PLAYER_NAME.to_string());

        let mut prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match prefs.game_type {
            GameType::Begin => prefs.beginner_name = new_name,
            GameType::Inter => prefs.inter_name = new_name,
            GameType::Expert => prefs.expert_name = new_name,
            // Unreachable
            GameType::Other => {}
        }
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let dlg = self.dlg.clone();
            move |_| -> AnyResult<bool> {
                let (game_type, current_name) = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let name = match prefs.game_type {
                        GameType::Begin => prefs.beginner_name.clone(),
                        GameType::Inter => prefs.inter_name.clone(),
                        GameType::Expert => prefs.expert_name.clone(),
                        // Unreachable
                        GameType::Other => "".to_string(),
                    };
                    (prefs.game_type, name)
                };

                // TODO: Handle errors
                if let Ok(best_hwnd) = dlg.hwnd().GetDlgItem(ControlId::TextBest as u16) {
                    let string = match game_type {
                        GameType::Begin => MSG_FASTEST_BEGINNER,
                        GameType::Inter => MSG_FASTEST_INTERMEDIATE,
                        GameType::Expert => MSG_FASTEST_EXPERT,
                        // Unreachable
                        GameType::Other => "",
                    };

                    let _ = best_hwnd.SetWindowText(string);
                }

                if let Ok(edit_hwnd) = dlg.hwnd().GetDlgItem(ControlId::EditName as u16) {
                    unsafe {
                        edit_hwnd.SendMessage(SetLimitText {
                            max_chars: Some(CCH_NAME_MAX as u32),
                        });
                    };

                    let _ = edit_hwnd.SetWindowText(&current_name);
                }

                Ok(true)
            }
        });

        self.dlg
            .on()
            .wm_command(ControlId::BtnOk as u16, BN::CLICKED, {
                let self2 = self.clone();
                move || -> AnyResult<()> {
                    self2.save_high_score_name();
                    let _ = self2.dlg.hwnd().EndDialog(1);
                    Ok(())
                }
            });

        self.dlg.on().wm_command(DLGID::CANCEL.raw(), BN::CLICKED, {
            let self2 = self.clone();
            move || -> AnyResult<()> {
                self2.save_high_score_name();
                let _ = self2.dlg.hwnd().EndDialog(1);
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
