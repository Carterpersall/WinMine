use core::cmp::{max, min};
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use windows_sys::Win32::Data::HtmlHelp::{
    HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC, HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HtmlHelpA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDlgItemTextW, SetDlgItemInt, SetDlgItemTextW,
};

use winsafe::co::{
    BN, DLGID, EM, GWLP, HELPW, ICC, IDC, MB, MK, PM, PS, SC, SM, STOCK_BRUSH, SW, VK, WA, WM, WS,
    WS_EX,
};
use winsafe::msg::WndMsg;
use winsafe::msg::wm::Destroy;
use winsafe::{
    AdjustWindowRectExForDpi, AnyResult, COLORREF, GetSystemMetrics, HBRUSH, HELPINFO, HINSTANCE,
    HMENU, HPEN, HWND, INITCOMMONCONTROLSEX, IdStr, InitCommonControlsEx, MSG, POINT, PeekMessage,
    PtsRc, RECT, SIZE, WINDOWPOS, WString, gui, prelude::*,
};

use crate::globals::{
    BASE_DPI, BLK_BTN_INPUT, CXBORDER, CYCAPTION, CYMENU, DEFAULT_PLAYER_NAME, ERR_OUT_OF_MEMORY,
    GAME_NAME, GAME_STATUS, IGNORE_NEXT_CLICK, INIT_MINIMIZED, LEFT_CLK_DOWN, MSG_FASTEST_BEGINNER,
    MSG_FASTEST_EXPERT, MSG_FASTEST_INTERMEDIATE, StatusFlag, TIME_FORMAT, UI_DPI, WINDOW_HEIGHT,
    WINDOW_WIDTH, WND_Y_OFFSET, update_ui_metrics_for_dpi,
};
use crate::grafix::{
    ButtonSprite, CleanUp, DX_BLK_96, DX_BUTTON_96, DX_LEFT_SPACE_96, DX_RIGHT_SPACE_96, DY_BLK_96,
    DY_BOTTOM_SPACE_96, DY_BUTTON_96, DY_GRID_OFF_96, DY_TOP_LED_96, DisplayScreen, DrawScreen,
    FInitLocal, FreeBitmaps, display_button, load_bitmaps, scale_dpi,
};
use crate::pref::{
    CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH, MenuMode, ReadPreferences, SoundState,
    WritePreferences,
};
use crate::rtns::{
    AdjustFlag, BOARD_HEIGHT, BOARD_INDEX_SHIFT, BOARD_WIDTH, BTN_FACE_STATE, BlockMask, C_BLK_MAX,
    CURSOR_X_POS, CURSOR_Y_POS, DoButton1Up, DoTimer, ID_TIMER, PauseGame, ResumeGame, StartGame,
    TrackMouse, board_mutex, make_guess, preferences_mutex,
};
use crate::sound::{FInitTunes, stop_all_sounds};
use crate::util::{DoAbout, DoHelp, GetDlgInt, IconId, InitConst, ReportErr, SetMenuBar};

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

/// Mines, height, and width tuples for the preset difficulty levels.
const LEVEL_DATA: [(i16, u32, u32); 3] = [(10, MINHEIGHT, MINWIDTH), (40, 16, 16), (99, 16, 30)];

/// Returns the preset data for a given game type, or None for custom games.
/// # Arguments
/// * `game`: The game type to get preset data for.
/// # Returns
/// The preset data as (mines, height, width), or None for a custom game.
const fn preset_data(game: GameType) -> Option<(i16, u32, u32)> {
    match game {
        GameType::Begin => Some(LEVEL_DATA[0]),
        GameType::Inter => Some(LEVEL_DATA[1]),
        GameType::Expert => Some(LEVEL_DATA[2]),
        GameType::Other => None,
    }
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

/// Determines whether the initial window state is minimized.
/// # Arguments
/// * `n_cmd_show`: The nCmdShow parameter from `WinMain`.
/// # Returns
/// True if the initial window state is minimized, false otherwise.
const fn initial_minimized_state(n_cmd_show: i32) -> bool {
    n_cmd_show == SW::SHOWMINNOACTIVE.raw() || n_cmd_show == SW::SHOWMINIMIZED.raw()
}

/// Initializes common controls used by the application using `InitCommonControlsEx`.
fn init_common_controls() {
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
    let _ = InitCommonControlsEx(&icc);
}

/// Struct containing the main window with its event handlers and the shared state.
#[derive(Clone)]
struct WinMineMainWindow {
    /// The main window, containing the HWND and event callbacks
    wnd: gui::WindowMain,
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
    fn begin_primary_button_drag(&self) {
        LEFT_CLK_DOWN.store(true, Ordering::Relaxed);
        CURSOR_X_POS.store(-1, Ordering::Relaxed);
        CURSOR_Y_POS.store(-1, Ordering::Relaxed);
        display_button(self.wnd.hwnd(), ButtonSprite::Caution);
    }

    /// Finishes a primary button drag operation.
    fn finish_primary_button_drag(&self) {
        LEFT_CLK_DOWN.store(false, Ordering::Relaxed);
        if status_play() {
            DoButton1Up(self.wnd.hwnd());
        } else {
            TrackMouse(self.wnd.hwnd(), -2, -2);
        }
    }

    /// Handles mouse move events.
    /// # Arguments
    /// * `key`: The mouse buttons currently pressed.
    /// * `point`: The coordinates of the mouse cursor.
    fn handle_mouse_move(&self, key: MK, point: POINT) {
        if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
            // If the left button is down, the user is dragging
            if status_play() {
                TrackMouse(
                    self.wnd.hwnd(),
                    x_box_from_xpos(point.x),
                    y_box_from_ypos(point.y),
                );
            } else {
                self.finish_primary_button_drag();
            }
        } else {
            // Regular mouse move
            handle_xyzzys_mouse(key, point);
        }
    }

    /// Handles right mouse button down events.
    /// # Arguments
    /// * `btn`: The mouse button that was pressed.
    /// * `point`: The coordinates of the mouse cursor.
    fn handle_rbutton_down(&self, btn: MK, point: POINT) {
        // Ignore right-clicks if the next click is set to be ignored
        if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) || !status_play() {
            return;
        }

        // TODO: Is this necessary?
        if !status_play() {
            return;
        }

        if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
            TrackMouse(self.wnd.hwnd(), -3, -3);
            set_block_flag(true);
            unsafe {
                // TODO: Change this
                let _ = self.wnd.hwnd().PostMessage(WndMsg::new(
                    WM::MOUSEMOVE,
                    btn.raw() as usize,
                    point.x as isize | ((point.y as isize) << 16),
                ));
            }
            return;
        }

        if btn == MK::LBUTTON {
            self.begin_primary_button_drag();
            self.handle_mouse_move(btn, point);
            return;
        }

        // Regular right-click: make a guess
        make_guess(
            self.wnd.hwnd(),
            x_box_from_xpos(point.x),
            y_box_from_ypos(point.y),
        );
    }

    /// Handles the "Custom" menu command by displaying the preferences dialog,
    /// updating the game settings, and starting a new game.
    ///
    /// TODO: The way that the preferences dialog is handled causes a custom game to always
    /// be started when the dialog is closed, even if the user clicked "Cancel". Fix this.
    fn DoPref(&self) {
        // Show the preferences dialog
        PrefDialog::new().show_modal(&self.wnd);

        let fmenu = {
            let mut prefs = match preferences_mutex().lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            prefs.wGameType = GameType::Other;
            prefs.fMenu
        };
        UPDATE_INI.store(true, Ordering::Relaxed);
        SetMenuBar(self.wnd.hwnd(), fmenu);
        StartGame(self.wnd.hwnd());
    }

    /// Handles command messages from the menu and accelerators.
    /// # Arguments
    /// * `w_param`: The wParam from the `WM_COMMAND` message.
    /// # Returns
    /// Some exit code if the command resulted in application exit, None otherwise.
    fn handle_command(&self, w_param: usize) -> Option<isize> {
        match menu_command(w_param) {
            Some(MenuCommand::New) => StartGame(self.wnd.hwnd()),
            Some(MenuCommand::Exit) => {
                self.wnd.hwnd().ShowWindow(SW::HIDE);
                unsafe {
                    let _ = self.wnd.hwnd().SendMessage(WndMsg::new(
                        WM::SYSCOMMAND,
                        SC::CLOSE.raw() as usize,
                        0,
                    ));
                }
                return Some(0);
            }
            Some(command @ (MenuCommand::Begin | MenuCommand::Inter | MenuCommand::Expert)) => {
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
                    if let Some(data) = preset_data(game) {
                        prefs.wGameType = game;
                        prefs.Mines = data.0;
                        prefs.Height = data.1 as i32;
                        prefs.Width = data.2 as i32;
                    }
                    prefs.fMenu
                };
                StartGame(self.wnd.hwnd());
                UPDATE_INI.store(true, Ordering::Relaxed);
                SetMenuBar(self.wnd.hwnd(), f_menu);
            }
            Some(MenuCommand::Custom) => self.DoPref(),
            Some(MenuCommand::Sound) => {
                let current_sound = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fSound
                };
                let new_sound = match current_sound {
                    SoundState::On => {
                        stop_all_sounds();
                        SoundState::Off
                    }
                    SoundState::Off => FInitTunes(),
                };
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fSound = new_sound;
                    prefs.fMenu
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                SetMenuBar(self.wnd.hwnd(), f_menu);
            }
            Some(MenuCommand::Color) => {
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fColor = !prefs.fColor;
                    prefs.fMenu
                };
                FreeBitmaps();
                if let Err(e) = load_bitmaps(self.wnd.hwnd()) {
                    eprintln!("Failed to reload bitmaps: {e}");
                    ReportErr(ERR_OUT_OF_MEMORY);
                    unsafe {
                        let _ = self.wnd.hwnd().SendMessage(WndMsg::new(
                            WM::SYSCOMMAND,
                            SC::CLOSE.raw() as usize,
                            0,
                        ));
                    }
                    return Some(0);
                }

                // Repaint immediately so toggling color off updates without restarting.
                DisplayScreen(self.wnd.hwnd());
                UPDATE_INI.store(true, Ordering::Relaxed);
                SetMenuBar(self.wnd.hwnd(), f_menu);
            }
            Some(MenuCommand::Mark) => {
                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fMark = !prefs.fMark;
                    prefs.fMenu
                };
                UPDATE_INI.store(true, Ordering::Relaxed);
                SetMenuBar(self.wnd.hwnd(), f_menu);
            }
            Some(MenuCommand::Best) => BestDialog::new().show_modal(&self.wnd),
            Some(MenuCommand::Help) => {
                DoHelp(self.wnd.hwnd(), HELPW::INDEX, HH_DISPLAY_TOPIC as u32);
            }
            Some(MenuCommand::HowToPlay) => {
                DoHelp(self.wnd.hwnd(), HELPW::CONTEXT, HH_DISPLAY_INDEX as u32);
            }
            Some(MenuCommand::HelpHelp) => {
                DoHelp(self.wnd.hwnd(), HELPW::HELPONHELP, HH_DISPLAY_TOPIC as u32);
            }
            Some(MenuCommand::HelpAbout) => {
                DoAbout(self.wnd.hwnd());
                return Some(0);
            }
            None => {}
        }

        None
    }

    /// Handles clicks on the smiley face button.
    /// # Arguments
    /// * `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// True if the click was handled, false otherwise.
    fn btn_click_handler(&self, point: POINT) -> bool {
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
            return false;
        }

        display_button(self.wnd.hwnd(), ButtonSprite::Down);
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
                            display_button(self.wnd.hwnd(), ButtonSprite::Happy);
                            StartGame(self.wnd.hwnd());
                        }
                        return true;
                    }
                    WM::MOUSEMOVE => {
                        if winsafe::PtInRect(rc, msg.pt) {
                            if !pressed {
                                pressed = true;
                                display_button(self.wnd.hwnd(), ButtonSprite::Down);
                            }
                        } else if pressed {
                            pressed = false;
                            display_button(self.wnd.hwnd(), current_face_sprite());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

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
                AdjustWindow(
                    self2.wnd.hwnd(),
                    AdjustFlag::Resize as i32 | AdjustFlag::Display as i32,
                );

                // Initialize local resources.
                if let Err(e) = FInitLocal(self2.wnd.hwnd()) {
                    eprintln!("Failed to initialize local resources: {e}");
                    ReportErr(ERR_OUT_OF_MEMORY);
                    return Err(std::io::Error::other(e.to_string()).into());
                }

                // Apply menu visibility and start the game.
                let f_menu = {
                    let prefs_guard = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs_guard.fMenu
                };
                SetMenuBar(self2.wnd.hwnd(), f_menu);
                StartGame(self2.wnd.hwnd());

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
                    // Persist the suggested top-left so AdjustWindow keeps us on the same monitor.
                    // TODO: Don't double lock here
                    if let Ok(mut prefs) = preferences_mutex().lock() {
                        prefs.xWindow = rc.left;
                        prefs.yWindow = rc.top;
                    } else if let Err(poisoned) = preferences_mutex().lock() {
                        let mut prefs = poisoned.into_inner();
                        prefs.xWindow = rc.left;
                        prefs.yWindow = rc.top;
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
                FreeBitmaps();
                if let Err(e) = load_bitmaps(self2.wnd.hwnd()) {
                    eprintln!("Failed to reload bitmaps after DPI change: {e}");
                }

                AdjustWindow(
                    self2.wnd.hwnd(),
                    AdjustFlag::Resize as i32 | AdjustFlag::Display as i32,
                );
                Ok(0)
            }
        });

        self.wnd.on().wm_window_pos_changed({
            let self2 = self.clone();
            move |wnd_pos| {
                handle_window_pos_changed(wnd_pos.windowpos);
                unsafe { self2.wnd.hwnd().DefWindowProc(wnd_pos) };
                Ok(())
            }
        });

        self.wnd.on().wm_sys_command({
            let self2 = self.clone();
            move |msg| {
                handle_syscommand(msg.request);
                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(())
            }
        });

        // TODO: Move the handle_command logic into separate wm_command closures
        self.wnd.on().wm(WM::COMMAND, {
            let self2 = self.clone();
            move |msg: WndMsg| {
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
                    DoEnterName(&self2.wnd);
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
                handle_keydown(self2.wnd.hwnd(), key.vkey_code);
                unsafe { self2.wnd.hwnd().DefWindowProc(key) };
                Ok(())
            }
        });

        self.wnd.on().wm_destroy({
            let self2 = self.clone();
            move || {
                // Stop the timer if it is still running
                let _ = self2.wnd.hwnd().KillTimer(ID_TIMER);

                // Clean up resources
                CleanUp();

                // Write preferences if they have changed
                if UPDATE_INI.load(Ordering::Relaxed)
                    && let Err(e) = WritePreferences()
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
                self2.handle_mouse_move(msg.vkey_code, msg.coords);
                unsafe { self2.wnd.hwnd().DefWindowProc(msg) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_down({
            let self2 = self.clone();
            move |r_btn| {
                self2.handle_rbutton_down(r_btn.vkey_code, r_btn.coords);
                unsafe { self2.wnd.hwnd().DefWindowProc(r_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_dbl_clk({
            let self2 = self.clone();
            move |r_btn| {
                self2.handle_rbutton_down(r_btn.vkey_code, r_btn.coords);
                unsafe { self2.wnd.hwnd().DefWindowProc(r_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_r_button_up({
            let self2 = self.clone();
            move |r_btn| {
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                    self2.finish_primary_button_drag();
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
                    self2.begin_primary_button_drag();
                    self2.handle_mouse_move(m_btn.vkey_code, m_btn.coords);
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(m_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_m_button_up({
            let self2 = self.clone();
            move |m_btn| {
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                    self2.finish_primary_button_drag();
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
                if self2.btn_click_handler(l_btn.coords) {
                    return Ok(());
                }
                if status_play() {
                    // Mask SHIFT and RBUTTON to indicate a "chord" operation.
                    set_block_flag(l_btn.vkey_code == MK::SHIFT || l_btn.vkey_code == MK::RBUTTON);
                    self2.begin_primary_button_drag();
                    self2.handle_mouse_move(l_btn.vkey_code, l_btn.coords);
                }
                unsafe { self2.wnd.hwnd().DefWindowProc(l_btn) };
                Ok(())
            }
        });

        self.wnd.on().wm_l_button_up({
            let self2 = self.clone();
            move |l_btn| {
                if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                    self2.finish_primary_button_drag();
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
                DoTimer(self2.wnd.hwnd());
                Ok(())
            }
        });

        self.wnd.on().wm_paint({
            let self2 = self.clone();
            move || {
                if let Ok(paint_guard) = self2.wnd.hwnd().BeginPaint() {
                    DrawScreen(&paint_guard);
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
/// The application exit code.
///
/// TODO: Return a result value
pub fn run_winmine(hinst: &HINSTANCE, n_cmd_show: i32) -> i32 {
    InitConst();

    // Initialize DPI to 96 (default) before creating the window
    UI_DPI.store(96, Ordering::Relaxed);
    update_ui_metrics_for_dpi(UI_DPI.load(Ordering::Relaxed));

    INIT_MINIMIZED.store(initial_minimized_state(n_cmd_show), Ordering::Relaxed);

    init_common_controls();

    let Ok(mut menu) = hinst.LoadMenu(IdStr::Id(MenuResourceId::Menu as u16)) else {
        eprintln!("Failed to load menu resource.");
        return 0;
    };

    let h_accel = hinst
        .LoadAccelerators(IdStr::Id(MenuResourceId::Accelerators as u16))
        .ok();

    ReadPreferences();

    let dx_window = WINDOW_WIDTH.load(Ordering::Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Ordering::Relaxed);

    // WinSafe `gui::WindowMain` owns the application message loop.
    let wnd = gui::WindowMain::new(gui::WindowMainOpts {
        class_name: GAME_NAME,
        title: GAME_NAME,
        class_icon: gui::Icon::Id(IconId::Main as u16),
        class_cursor: gui::Cursor::Idc(IDC::ARROW),
        class_bg_brush: gui::Brush::Handle(
            HBRUSH::GetStockObject(STOCK_BRUSH::LTGRAY).unwrap_or(HBRUSH::NULL),
        ),
        size: (dx_window, dy_window),
        style: WS::OVERLAPPED | WS::MINIMIZEBOX | WS::CAPTION | WS::SYSMENU,
        menu: menu.leak(),
        accel_table: h_accel,
        ..Default::default()
    });

    let app = WinMineMainWindow::new(wnd);

    let cmd_show = if initial_minimized_state(n_cmd_show) {
        Some(SW::SHOWMINIMIZED)
    } else {
        Some(SW::SHOWNORMAL)
    };

    match app.wnd.run_main(cmd_show) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Unhandled error running main window: {e}");
            let _ = HWND::NULL
                .MessageBox(&e.to_string(), "Unhandled error", MB::OK | MB::ICONERROR)
                .ok();
            0
        }
    }
}

/// Converts an x-coordinate in pixels to a box index.
/// # Arguments
/// * `x`: The x-coordinate in pixels.
/// # Returns
/// The corresponding box index.
fn x_box_from_xpos(x: i32) -> i32 {
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
fn y_box_from_ypos(y: i32) -> i32 {
    let cell = scale_dpi(DY_BLK_96);
    if cell <= 0 {
        return 0;
    }
    (y - (scale_dpi(DY_GRID_OFF_96) - cell)) / cell
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

/// Returns the current face button sprite based on the button state.
/// # Returns
/// The current face button sprite state.
fn current_face_sprite() -> ButtonSprite {
    match BTN_FACE_STATE.load(Ordering::Relaxed) {
        0 => ButtonSprite::Happy,
        1 => ButtonSprite::Caution,
        2 => ButtonSprite::Lose,
        3 => ButtonSprite::Win,
        _ => ButtonSprite::Down,
    }
}

/* Window Message Handlers */

/// Maps a command ID to a `MenuCommand` enum variant.
/// # Arguments
/// * `w_param`: The WPARAM from the `WM_COMMAND` message.
/// # Returns
/// An Option containing the corresponding `MenuCommand`, or None if not found.
const fn menu_command(w_param: usize) -> Option<MenuCommand> {
    match command_id(w_param) {
        510 => Some(MenuCommand::New),
        512 => Some(MenuCommand::Exit),
        521 => Some(MenuCommand::Begin),
        522 => Some(MenuCommand::Inter),
        523 => Some(MenuCommand::Expert),
        524 => Some(MenuCommand::Custom),
        526 => Some(MenuCommand::Sound),
        527 => Some(MenuCommand::Mark),
        528 => Some(MenuCommand::Best),
        529 => Some(MenuCommand::Color),
        590 => Some(MenuCommand::Help),
        591 => Some(MenuCommand::HowToPlay),
        592 => Some(MenuCommand::HelpHelp),
        593 => Some(MenuCommand::HelpAbout),
        _ => None,
    }
}

/// Handles the `WM_KEYDOWN` message.
/// # Arguments
/// * `hwnd`: A reference to the window handle.
/// * `key`: The virtual key code of the key that was pressed.
fn handle_keydown(hwnd: &HWND, key: VK) {
    match key {
        code if code == VK::F4 => {
            let current_sound = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fSound
            };

            if matches!(current_sound, SoundState::On | SoundState::Off) {
                let new_sound = match current_sound {
                    SoundState::On => {
                        stop_all_sounds();
                        SoundState::Off
                    }
                    SoundState::Off => FInitTunes(),
                };

                let f_menu = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fSound = new_sound;
                    prefs.fMenu
                };

                UPDATE_INI.store(true, Ordering::Relaxed);
                SetMenuBar(hwnd, f_menu);
            }
        }
        code if code == VK::F5 => {
            let menu_value = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fMenu
            };

            if !matches!(menu_value, MenuMode::AlwaysOn) {
                SetMenuBar(hwnd, MenuMode::Hidden);
            }
        }
        code if code == VK::F6 => {
            let menu_value = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fMenu
            };

            if !matches!(menu_value, MenuMode::AlwaysOn) {
                SetMenuBar(hwnd, MenuMode::On);
            }
        }
        code if code == VK::SHIFT => handle_xyzzys_shift(),
        _ => handle_xyzzys_default_key(key),
    }
}

/// Handles the `WM_WINDOWPOSCHANGED` message to store the new window position in preferences.
/// # Arguments
/// * `pos` - A reference to the `WINDOWPOS` structure containing the new window position.
fn handle_window_pos_changed(pos: &WINDOWPOS) {
    if status_icon() {
        return;
    }

    // TODO: Don't double lock here
    if let Ok(mut prefs) = preferences_mutex().lock() {
        prefs.xWindow = pos.x;
        prefs.yWindow = pos.y;
    } else if let Err(poisoned) = preferences_mutex().lock() {
        let mut guard = poisoned.into_inner();
        guard.xWindow = pos.x;
        guard.yWindow = pos.y;
    }
}

/// Handles the `WM_SYSCOMMAND` message for minimize and restore events.
/// # Arguments
/// * `command` - The system command identifier.
fn handle_syscommand(command: SC) {
    // Isolate the system command identifier by masking out the lower 4 bits.
    //let command = (sys_cmd & 0xFFF0) as u32;
    if command == SC::MINIMIZE {
        PauseGame();
        set_status_pause();
        set_status_icon();
    } else if command == SC::RESTORE {
        clr_status_pause();
        clr_status_icon();
        ResumeGame();
        IGNORE_NEXT_CLICK.store(false, Ordering::Relaxed);
    }
}

/// Checks if the given (x, y) coordinates are within the valid board range.
/// # Arguments
/// * `x`: The x-coordinate to check.
/// * `y`: The y-coordinate to check.
/// # Returns
/// True if the coordinates are within range, false otherwise.
fn in_range(x: i32, y: i32) -> bool {
    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

/// Calculates the board index for the given (x, y) coordinates.
/// # Arguments
/// * `x`: The x-coordinate.
/// * `y`: The y-coordinate.
/// # Returns
/// The calculated board index.
fn board_index(x: i32, y: i32) -> usize {
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    offset.max(0) as usize
}

/// Checks if the cell at the given (x, y) coordinates is a bomb.
/// # Arguments
/// * `x`: The x-coordinate of the cell.
/// * `y`: The y-coordinate of the cell.
/// # Returns
/// True if the cell is a bomb, false otherwise.
fn cell_is_bomb(x: i32, y: i32) -> bool {
    if !in_range(x, y) {
        return false;
    }
    let idx = board_index(x, y);
    if idx >= C_BLK_MAX {
        return false;
    }
    let guard = board_mutex();
    (guard[idx] as u8 & BlockMask::Bomb as u8) != 0
}

/* XYZZY Cheat Code Handling */

/// Length of the XYZZY cheat code sequence.
const CCH_XYZZY: i32 = 5;
/// Atomic counter tracking the progress of the XYZZY cheat code entry.
static I_XYZZY: AtomicI32 = AtomicI32::new(0);
/// The expected sequence of virtual key codes for the XYZZY cheat code.
const XYZZY_SEQUENCE: [VK; 5] = [VK::CHAR_X, VK::CHAR_Y, VK::CHAR_Z, VK::CHAR_Z, VK::CHAR_Y];

/// Handles the SHIFT key press for the XYZZY cheat code.
/// If the cheat code has been fully entered, this function toggles
/// the cheat code state by XORing the counter with 20 (0b10100).
fn handle_xyzzys_shift() {
    if I_XYZZY.load(Ordering::Relaxed) >= CCH_XYZZY {
        I_XYZZY.fetch_xor(20, Ordering::Relaxed);
    }
}

/// Handles default key presses for the XYZZY cheat code.
/// It checks if the pressed key matches the expected character in the
/// XYZZY sequence and updates the counter accordingly.
/// If the sequence is broken, the counter is reset.
/// # Arguments
/// * `w_param` - The WPARAM from the keydown message, containing the virtual key code
fn handle_xyzzys_default_key(key: VK) {
    let current = I_XYZZY.load(Ordering::Relaxed);
    if current < CCH_XYZZY {
        let expected = XYZZY_SEQUENCE[current as usize];
        if expected == key {
            I_XYZZY.store(current + 1, Ordering::Relaxed);
        } else {
            I_XYZZY.store(0, Ordering::Relaxed);
        }
    }
}

/// Handles mouse movement for the XYZZY cheat code.
/// If the cheat code is active and the Control key is held down,
/// or if the cheat code has been fully entered,
/// it reveals whether the cell under the cursor is a bomb or not by
/// setting the pixel at (0,0) of the device context to black (bomb) or white (no bomb).
/// # Arguments
/// * `key` - The WPARAM from the mouse move message, containing key states.
/// * `point` - The LPARAM from the mouse move message, containing cursor position.
fn handle_xyzzys_mouse(key: MK, point: POINT) {
    // Check if the XYZZY cheat code is active.
    let state = I_XYZZY.load(Ordering::Relaxed);
    if state == 0 {
        return;
    }

    // Check if the Control key is held down.
    let control_down = key == MK::CONTROL;
    if (state == CCH_XYZZY && control_down) || state > CCH_XYZZY {
        let x_pos = x_box_from_xpos(point.x);
        let y_pos = y_box_from_ypos(point.y);
        CURSOR_X_POS.store(x_pos, Ordering::Relaxed);
        CURSOR_Y_POS.store(y_pos, Ordering::Relaxed);
        if in_range(x_pos, y_pos)
            && let Ok(hdc) = HWND::DESKTOP.GetDC()
        {
            let color = if cell_is_bomb(x_pos, y_pos) {
                COLORREF::from_rgb(0, 0, 0)
            } else {
                COLORREF::from_rgb(0xFF, 0xFF, 0xFF)
            };

            // Set the pixel at (0,0) to indicate bomb status.
            HPEN::CreatePen(PS::SOLID, 0, color)
                .and_then(|mut pen| {
                    let mut old_pen = hdc.SelectObject(&pen.leak())?;
                    hdc.MoveToEx(0, 0, None)?;
                    // LineTo excludes the endpoint, so drawing to (1,0) sets pixel (0,0)
                    hdc.LineTo(1, 0)?;
                    hdc.SelectObject(&old_pen.leak())?;
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    eprintln!("Failed to draw pixel at (0,0): {e}");
                });
        }
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
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    (prefs.Height, prefs.Width, prefs.Mines)
                };

                // Populate the dialog controls with the current settings
                unsafe {
                    let hdlg_raw = dlg.hwnd().ptr();
                    SetDlgItemInt(hdlg_raw, ControlId::EditHeight as i32, height as u32, 0);
                    SetDlgItemInt(hdlg_raw, ControlId::EditWidth as i32, width as u32, 0);
                    SetDlgItemInt(hdlg_raw, ControlId::EditMines as i32, mines as u32, 0);
                }

                Ok(true)
            }
        });

        self.dlg.on().wm_command(DLGID::OK.raw(), BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Retrieve and validate user input from the dialog controls
                let height = GetDlgInt(dlg.hwnd(), ControlId::EditHeight as i32, MINHEIGHT, 24);
                let width = GetDlgInt(dlg.hwnd(), ControlId::EditWidth as i32, MINWIDTH, 30);
                let max_mines = min(999, (height - 1) * (width - 1));
                let mines = GetDlgInt(dlg.hwnd(), ControlId::EditMines as i32, 10, max_mines);

                // Update preferences with the new settings
                let mut prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.Height = height as i32;
                prefs.Width = width as i32;
                prefs.Mines = mines as i16;

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
    dlg: gui::WindowModal,
}

impl BestDialog {
    fn new() -> Self {
        let dlg = gui::WindowModal::new_dlg(DialogTemplateId::Best as u16);
        let new_self = Self { dlg };
        new_self.events();
        new_self
    }

    fn show_modal(&self, parent: &impl GuiParent) {
        if let Err(e) = self.dlg.show_modal(parent) {
            eprintln!("Failed to show best-times dialog: {e}");
        }
    }

    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let dlg = self.dlg.clone();
            move |_| -> AnyResult<bool> {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                reset_best_dialog(
                    dlg.hwnd(),
                    prefs.rgTime[GameType::Begin as usize],
                    prefs.rgTime[GameType::Inter as usize],
                    prefs.rgTime[GameType::Expert as usize],
                    &prefs.szBegin,
                    &prefs.szInter,
                    &prefs.szExpert,
                );

                Ok(true)
            }
        });

        self.dlg
            .on()
            .wm_command(ControlId::BtnReset as u16, BN::CLICKED, {
                let dlg = self.dlg.clone();
                move || -> AnyResult<()> {
                    // Set best times and names to defaults
                    {
                        let mut prefs = match preferences_mutex().lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };

                        // Set all best times to 999 seconds
                        prefs.rgTime[GameType::Begin as usize] = 999;
                        prefs.rgTime[GameType::Inter as usize] = 999;
                        prefs.rgTime[GameType::Expert as usize] = 999;

                        // Set the three best names to the default values
                        prefs.szBegin = DEFAULT_PLAYER_NAME.to_string();
                        prefs.szInter = DEFAULT_PLAYER_NAME.to_string();
                        prefs.szExpert = DEFAULT_PLAYER_NAME.to_string();
                    };

                    UPDATE_INI.store(true, Ordering::Relaxed);
                    reset_best_dialog(
                        dlg.hwnd(),
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
    dlg: gui::WindowModal,
}

impl EnterDialog {
    fn new() -> Self {
        let dlg = gui::WindowModal::new_dlg(DialogTemplateId::Enter as u16);
        let new_self = Self { dlg };
        new_self.events();
        new_self
    }

    fn show_modal(&self, parent: &impl GuiParent) {
        if let Err(e) = self.dlg.show_modal(parent) {
            eprintln!("Failed to show name-entry dialog: {e}");
        }
    }

    fn save_high_score_name(&self) {
        let mut buffer = [0u16; CCH_NAME_MAX];
        unsafe {
            GetDlgItemTextW(
                self.dlg.hwnd().ptr(),
                ControlId::EditName as i32,
                buffer.as_mut_ptr(),
                CCH_NAME_MAX as i32,
            );
        }

        let mut prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let new_name = String::from_utf16_lossy(&buffer);
        match prefs.wGameType {
            GameType::Begin => prefs.szBegin = new_name,
            GameType::Inter => prefs.szInter = new_name,
            _ => prefs.szExpert = new_name,
        }
    }

    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let dlg = self.dlg.clone();
            move |_| -> AnyResult<bool> {
                let (game_type, current_name) = {
                    let prefs = match preferences_mutex().lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let name = match prefs.wGameType {
                        GameType::Begin => prefs.szBegin.clone(),
                        GameType::Inter => prefs.szInter.clone(),
                        _ => prefs.szExpert.clone(),
                    };
                    (prefs.wGameType, name)
                };

                // TODO: Do this better
                unsafe {
                    let hdlg_raw = dlg.hwnd().ptr();

                    let string = match game_type {
                        GameType::Begin => MSG_FASTEST_BEGINNER,
                        GameType::Inter => MSG_FASTEST_INTERMEDIATE,
                        GameType::Expert => MSG_FASTEST_EXPERT,
                        // Unreachable
                        GameType::Other => "",
                    };

                    SetDlgItemTextW(
                        hdlg_raw,
                        ControlId::TextBest as i32,
                        WString::from_str(string).as_ptr(),
                    );

                    if let Ok(edit_hwnd) = dlg.hwnd().GetDlgItem(ControlId::EditName as u16) {
                        let _ = edit_hwnd.SendMessage(WndMsg::new(
                            WM::from_raw(EM::SETLIMITTEXT.raw()),
                            CCH_NAME_MAX,
                            0,
                        ));
                    }

                    SetDlgItemTextW(
                        hdlg_raw,
                        ControlId::EditName as i32,
                        WString::from_str(current_name).as_ptr(),
                    );
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

/// Handles the high-score name entry dialog.
/// # Arguments
/// * `parent`: The parent GUI element for the modal dialog.
fn DoEnterName(parent: &impl GuiParent) {
    EnterDialog::new().show_modal(parent);
    UPDATE_INI.store(true, Ordering::Relaxed);
}

/// Adjusts the main window size and position based on the current board and menu state.
///
/// This function is called whenever the board or menu state changes to ensure
/// that the main window is appropriately sized and positioned on the screen.
/// # Arguments
/// * `hwnd` - A reference to the main window handle.
/// * `f_adjust` - Flags indicating how to adjust the window (e.g., resize).
pub fn AdjustWindow(hwnd: &HWND, mut f_adjust: i32) {
    let menu_handle = hwnd.GetMenu().unwrap_or(HMENU::NULL);

    let x_boxes = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_boxes = BOARD_HEIGHT.load(Ordering::Relaxed);
    let dx_window =
        scale_dpi(DX_BLK_96) * x_boxes + scale_dpi(DX_LEFT_SPACE_96) + scale_dpi(DX_RIGHT_SPACE_96);
    let dy_window =
        scale_dpi(DY_BLK_96) * y_boxes + scale_dpi(DY_GRID_OFF_96) + scale_dpi(DY_BOTTOM_SPACE_96);
    WINDOW_WIDTH.store(dx_window, Ordering::Relaxed);
    WINDOW_HEIGHT.store(dy_window, Ordering::Relaxed);

    let (mut x_window, mut y_window, f_menu) = {
        let prefs = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (prefs.xWindow, prefs.yWindow, prefs.fMenu)
    };

    let menu_visible = !matches!(f_menu, MenuMode::Hidden) && menu_handle.as_opt().is_some();
    let mut menu_extra = 0;
    let mut diff_level = false;
    if menu_visible
        && let Some(hwnd) = hwnd.as_opt()
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
    let dw_style = hwnd.GetWindowLongPtr(GWLP::STYLE);
    let dw_ex_style = hwnd.GetWindowLongPtr(GWLP::EXSTYLE);
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

    let mut excess = x_window + dx_window + frame_extra - our_get_system_metrics(SM::CXSCREEN);
    if excess > 0 {
        f_adjust |= AdjustFlag::Resize as i32;
        x_window -= excess;
    }
    excess = y_window + dy_window + dyp_adjust - our_get_system_metrics(SM::CYSCREEN);
    if excess > 0 {
        f_adjust |= AdjustFlag::Resize as i32;
        y_window -= excess;
    }

    if !INIT_MINIMIZED.load(Ordering::Relaxed) {
        if (f_adjust & AdjustFlag::Resize as i32) != 0 {
            let _ = hwnd.MoveWindow(
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
                    hwnd.GetMenuItemRect(menu, 0)
                        .ok()
                        .zip(hwnd.GetMenuItemRect(menu, 1).ok())
                })
                .is_some_and(|(g, h)| g.top == h.top)
        {
            dyp_adjust -= CYMENU.load(Ordering::Relaxed);
            WND_Y_OFFSET.store(dyp_adjust, Ordering::Relaxed);
            let _ = hwnd.MoveWindow(
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
            let _ = hwnd.InvalidateRect(Some(&rect), true);
        }
    }

    // TODO: Don't double lock here
    if let Ok(mut prefs) = preferences_mutex().lock() {
        prefs.xWindow = x_window;
        prefs.yWindow = y_window;
    } else if let Err(poisoned) = preferences_mutex().lock() {
        let mut guard = poisoned.into_inner();
        guard.xWindow = x_window;
        guard.yWindow = y_window;
    }
}

/// Retrieves system metrics, favoring virtual screen metrics for multi-monitor support.
/// # Arguments
/// * `index` - The system metric index to retrieve.
/// # Returns
/// The requested system metric value.
fn our_get_system_metrics(index: SM) -> i32 {
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

/// Extracts the command identifier from a WPARAM value.
/// # Arguments
/// * `w_param` - The WPARAM value to extract from.
/// # Returns
/// The command identifier as a u16.
const fn command_id(w_param: usize) -> u16 {
    (w_param & 0xFFFF) as u16
}

/// Sets the dialog text for a given time and name in the best scores dialog.
///
/// TODO: Remove this function
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `id` - The control ID for the time text.
/// * `time` - The time value to display.
/// * `name` - The name associated with the time.
fn set_dtext(h_dlg: &HWND, id: i32, time: u16, name: &str) {
    // TODO: Make this better
    let time_fmt = TIME_FORMAT.replace("%d", &time.to_string());

    unsafe {
        SetDlgItemTextW(h_dlg.ptr(), id, WString::from_str(&time_fmt).as_ptr());
        SetDlgItemTextW(h_dlg.ptr(), id + 1, WString::from_str(name).as_ptr());
    }
}

/// Resets the best scores dialog with the provided times and names.
/// # Arguments
/// * `h_dlg` - Handle to the dialog window.
/// * `time_begin` - The best time for the beginner level.
/// * `time_inter` - The best time for the intermediate level.
/// * `time_expert` - The best time for the expert level.
/// * `name_begin` - The name associated with the beginner level best time.
/// * `name_inter` - The name associated with the intermediate level best time.
/// * `name_expert` - The name associated with the expert level best time.
fn reset_best_dialog(
    h_dlg: &HWND,
    time_begin: u16,
    time_inter: u16,
    time_expert: u16,
    name_begin: &str,
    name_inter: &str,
    name_expert: &str,
) {
    set_dtext(h_dlg, ControlId::TimeBegin as i32, time_begin, name_begin);
    set_dtext(h_dlg, ControlId::TimeInter as i32, time_inter, name_inter);
    set_dtext(
        h_dlg,
        ControlId::TimeExpert as i32,
        time_expert,
        name_expert,
    );
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
