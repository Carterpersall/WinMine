use core::cmp::{max, min};
use core::sync::atomic::{AtomicI32, Ordering};
use std::sync::atomic::AtomicBool;

use windows_sys::Win32::Data::HtmlHelp::{
    HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC, HH_TP_HELP_CONTEXTMENU, HH_TP_HELP_WM_HELP, HtmlHelpA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDlgItemTextW, SetDlgItemInt, SetDlgItemTextW,
};

use winsafe::co::{self, GWLP, HELPW, ICC, IDC, SM, STOCK_BRUSH, WS, WS_EX};
use winsafe::msg::WndMsg;
use winsafe::prelude::Handle;
use winsafe::{
    AdjustWindowRectEx, AtomStr, COLORREF, DLGPROC, DispatchMessage, GetMessage, GetSystemMetrics,
    HACCEL, HBRUSH, HCURSOR, HICON, HINSTANCE, HMENU, HPEN, HWND, INITCOMMONCONTROLSEX, IdIdcStr,
    IdIdiStr, IdMenu, IdStr, InitCommonControlsEx, MSG, POINT, PeekMessage, PostQuitMessage, PtsRc,
    RECT, RegisterClassEx, SIZE, TranslateMessage, WINDOWPOS, WNDCLASSEX, WString,
};

use crate::globals::{
    APP_PAUSED, BLK_BTN_INPUT, CXBORDER, CYCAPTION, CYMENU, GAME_STATUS, IGNORE_NEXT_CLICK,
    INIT_MINIMIZED, LEFT_CLK_DOWN, StatusFlag, WINDOW_HEIGHT, WINDOW_WIDTH, WND_Y_OFFSET,
    global_state,
};
use crate::grafix::{
    ButtonSprite, CleanUp, DX_BLK, DX_BUTTON, DX_GRID_OFF, DX_RIGHT_SPACE, DY_BLK, DY_BOTTOM_SPACE,
    DY_BUTTON, DY_GRID_OFF, DY_TOP_LED, DisplayButton, DisplayScreen, DrawScreen, FInitLocal,
    FLoadBitmaps, FreeBitmaps,
};
use crate::pref::{
    CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH, MenuMode, ReadPreferences, SoundState,
    WritePreferences,
};
use crate::rtns::{
    AdjustFlag, BOARD_HEIGHT, BOARD_INDEX_SHIFT, BOARD_WIDTH, BTN_FACE_STATE, BlockMask, C_BLK_MAX,
    CURSOR_X_POS, CURSOR_Y_POS, DoButton1Up, DoTimer, ID_TIMER, MakeGuess, PauseGame, ResumeGame,
    StartGame, TrackMouse, board_mutex, preferences_mutex,
};
use crate::sound::{EndTunes, FInitTunes};
use crate::util::{
    CCH_MSG_MAX, CheckEm, DoAbout, DoHelp, GetDlgInt, IconId, InitConst, LoadSz, ReportErr,
    SetMenuBar,
};

/// Indicates that preferences have changed and should be saved
static UPDATE_INI: AtomicBool = AtomicBool::new(false);

/// Menu and accelerator resource identifiers.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum MenuResourceId {
    /// Main menu resource.
    Menu = 500,
    /// Accelerator table resource.
    Accelerators = 501,
}
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
/// Resource identifier for the out-of-memory error.
const ID_ERR_MEM: u16 = 5;

/// Dialog templates.
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum DialogTemplateId {
    Pref = 80,
    Enter = 600,
    Best = 700,
}

/// Control identifiers shared across dialogs.
#[repr(i32)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum ControlId {
    EditHeight = 141,
    EditWidth = 142,
    EditMines = 143,
    BtnOk = 100,
    BtnCancel = 109,
    BtnReset = 707,
    TextBest = 601,
    EditName = 602,
    TimeBegin = 701,
    NameBegin = 702,
    TimeInter = 703,
    NameInter = 704,
    TimeExpert = 705,
    NameExpert = 706,
    SText1 = 708,
    SText2 = 709,
    SText3 = 710,
    TxtMines = 111,
    TxtHeight = 112,
    TxtWidth = 113,
}

/// Help context identifiers.
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum HelpContextId {
    PrefEditHeight = 1000,
    PrefEditWidth = 1001,
    PrefEditMines = 1002,
    BestBtnReset = 1003,
    SText = 1004,
}
const ID_MSG_BEGIN: u16 = 9;

/// Mines, height, and width tuples for the preset difficulty levels.
const LEVEL_DATA: [[i32; 3]; 3] = [[10, MINHEIGHT, MINWIDTH], [40, 16, 16], [99, 16, 30]];

fn preset_data(game: GameType) -> Option<[i32; 3]> {
    match game {
        GameType::Begin => Some(LEVEL_DATA[0]),
        GameType::Inter => Some(LEVEL_DATA[1]),
        GameType::Expert => Some(LEVEL_DATA[2]),
        GameType::Other => None,
    }
}

const HELP_FILE: &str = "winmine.chm\0";

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

// Structure for HELP_WM_HELP message.
#[repr(C)]
struct HelpInfo {
    cbSize: u32,
    iContextType: i32,
    iCtrlId: i32,
    hItemHandle: HWND,
    dwContextId: usize,
    mouse_pos: POINT,
}

fn show_dialog(template_id: u16, proc: DLGPROC) {
    let state = global_state();
    let hinst_wrap = {
        let guard = match state.h_inst.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe { HINSTANCE::from_ptr(guard.ptr()) }
    };
    let parent_hwnd = {
        let guard = match state.hwnd_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe { HWND::from_ptr(guard.ptr()) }
    };
    unsafe {
        let _ =
            hinst_wrap.DialogBoxParam(IdStr::Id(template_id), parent_hwnd.as_opt(), proc, Some(0));
    }
}

fn initial_minimized_state(n_cmd_show: i32) -> bool {
    n_cmd_show == co::SW::SHOWMINNOACTIVE.raw() || n_cmd_show == co::SW::SHOWMINIMIZED.raw()
}

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

fn register_main_window_class() -> bool {
    let state = global_state();
    let (hinst, hicon) = {
        let inst_guard = match state.h_inst.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let icon_guard = match state.h_icon_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let hicon = icon_guard
            .as_ref()
            .map(|icon| unsafe { HICON::from_ptr(icon.ptr()) })
            .unwrap_or(HICON::NULL);
        (unsafe { HINSTANCE::from_ptr(inst_guard.ptr()) }, hicon)
    };
    let class_buf = {
        let guard = match state.sz_class.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    };

    unsafe {
        let mut wcx = WNDCLASSEX::default();
        wcx.lpfnWndProc = Some(MainWndProc);
        let hicon_sm = HICON::from_ptr(hicon.ptr());
        wcx.hInstance = hinst;
        wcx.hIcon = hicon;
        wcx.hIconSm = hicon_sm;
        wcx.hCursor = HINSTANCE::NULL
            .LoadCursor(IdIdcStr::Idc(IDC::ARROW))
            .map(|mut cursor| cursor.leak())
            .unwrap_or(HCURSOR::NULL);
        wcx.hbrBackground = HBRUSH::GetStockObject(STOCK_BRUSH::LTGRAY).unwrap_or(HBRUSH::NULL);

        let mut class_name = WString::from_wchars_slice(&class_buf);
        wcx.set_lpszClassName(Some(&mut class_name));
        RegisterClassEx(&wcx).is_ok()
    }
}

pub fn run_winmine(h_instance: HINSTANCE, n_cmd_show: i32) -> i32 {
    let state = global_state();
    {
        let mut inst_guard = match state.h_inst.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *inst_guard = h_instance;
    }
    InitConst();

    INIT_MINIMIZED.store(initial_minimized_state(n_cmd_show), Ordering::Relaxed);

    init_common_controls();
    let hinst_wrap = {
        let guard = match state.h_inst.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe { HINSTANCE::from_ptr(guard.ptr()) }
    };

    let icon = hinst_wrap.LoadIcon(IdIdiStr::Id(IconId::Main as u16)).ok();
    {
        let mut icon_guard = match state.h_icon_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *icon_guard = icon;
    }

    if !register_main_window_class() {
        return 0;
    }

    let menu = hinst_wrap
        .LoadMenu(IdStr::Id(MenuResourceId::Menu as u16))
        .ok();
    let menu_param_handle = menu
        .as_ref()
        .map(|m| unsafe { HMENU::from_ptr(m.ptr()) })
        .unwrap_or(HMENU::NULL);
    {
        let mut menu_guard = match state.h_menu.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *menu_guard = menu;
    }
    let h_accel = hinst_wrap
        .LoadAccelerators(IdStr::Id(MenuResourceId::Accelerators as u16))
        .ok();

    unsafe {
        ReadPreferences();
    }

    let dx_window = WINDOW_WIDTH.load(Ordering::Relaxed);
    let dy_window = WINDOW_HEIGHT.load(Ordering::Relaxed);
    let dxp_border = CXBORDER.load(Ordering::Relaxed);
    let dyp_adjust = WND_Y_OFFSET.load(Ordering::Relaxed);

    let class_name = {
        let guard = match state.sz_class.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let len = guard.iter().position(|&c| c == 0).unwrap_or(guard.len());
        String::from_utf16_lossy(&guard[..len])
    };

    let (x_window, y_window, f_menu) = {
        let prefs_guard = match preferences_mutex().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (prefs_guard.xWindow, prefs_guard.yWindow, prefs_guard.fMenu)
    };

    let menu_param = if menu_param_handle == HMENU::NULL {
        IdMenu::None
    } else {
        IdMenu::Menu(&menu_param_handle)
    };

    let hwnd_main = unsafe {
        HWND::CreateWindowEx(
            WS_EX::from_raw(0),
            AtomStr::from_str(&class_name),
            Some(&class_name),
            co::WS::OVERLAPPED | co::WS::MINIMIZEBOX | co::WS::CAPTION | co::WS::SYSMENU,
            POINT {
                x: x_window - dxp_border,
                y: y_window - dyp_adjust,
            },
            SIZE {
                cx: dx_window + dxp_border,
                cy: dy_window + dyp_adjust,
            },
            None,
            menu_param,
            &hinst_wrap,
            None,
        )
    }
    .unwrap_or(HWND::NULL);

    let hwnd_store = unsafe { HWND::from_ptr(hwnd_main.ptr()) };

    {
        let mut hwnd_guard = match state.hwnd_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *hwnd_guard = hwnd_store;
    }

    if hwnd_main.as_opt().is_none() {
        ReportErr(1000);
        return 0;
    }

    AdjustWindow(0);

    if let Err(e) = FInitLocal() {
        eprintln!("Failed to initialize local resources: {}", e);
        ReportErr(ID_ERR_MEM);
        return 0;
    }

    SetMenuBar(f_menu);
    StartGame();

    if let Some(hwnd_wrap) = hwnd_main.as_opt() {
        hwnd_wrap.ShowWindow(co::SW::SHOWNORMAL);
        let _ = hwnd_wrap.UpdateWindow();
    }

    INIT_MINIMIZED.store(false, Ordering::Relaxed);

    let mut msg = MSG::default();
    while let Ok(has_msg) = GetMessage(&mut msg, None, 0, 0) {
        if !has_msg {
            break;
        }

        let handled = h_accel
            .as_ref()
            .map(|accel| unsafe { HACCEL::from_ptr(accel.ptr()) })
            .unwrap_or(HACCEL::NULL)
            .as_opt()
            .and_then(|accel| {
                let hwnd_copy = {
                    match state.hwnd_main.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    }
                };
                hwnd_copy
                    .as_opt()
                    .and_then(|hwnd| hwnd.TranslateAccelerator(accel, &mut msg).ok())
            })
            .is_some();

        if !handled {
            TranslateMessage(&msg);
            unsafe {
                let _ = DispatchMessage(&msg);
            }
        }
    }

    CleanUp();

    if UPDATE_INI.load(Ordering::Relaxed) {
        unsafe {
            if let Err(e) = WritePreferences() {
                eprintln!("Failed to write preferences: {}", e);
            }
        }
    }

    msg.wParam as i32
}

fn x_box_from_xpos(x: i32) -> i32 {
    (x - (DX_GRID_OFF - DX_BLK)) >> 4
}

fn y_box_from_ypos(y: i32) -> i32 {
    (y - (DY_GRID_OFF - DY_BLK)) >> 4
}

fn status_icon() -> bool {
    GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Icon as i32) != 0
}

fn status_play() -> bool {
    GAME_STATUS.load(Ordering::Relaxed) & (StatusFlag::Play as i32) != 0
}

fn set_status_pause() {
    GAME_STATUS.fetch_or(StatusFlag::Pause as i32, Ordering::Relaxed);
}

fn clr_status_pause() {
    GAME_STATUS.fetch_and(!(StatusFlag::Pause as i32), Ordering::Relaxed);
}

fn set_status_icon() {
    GAME_STATUS.fetch_or(StatusFlag::Icon as i32, Ordering::Relaxed);
}

fn clr_status_icon() {
    GAME_STATUS.fetch_and(!(StatusFlag::Icon as i32), Ordering::Relaxed);
}

fn set_block_flag(active: bool) {
    BLK_BTN_INPUT.store(active, Ordering::Relaxed);
}

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

fn begin_primary_button_drag(_h_wnd: HWND) {
    LEFT_CLK_DOWN.store(true, Ordering::Relaxed);
    CURSOR_X_POS.store(-1, Ordering::Relaxed);
    CURSOR_Y_POS.store(-1, Ordering::Relaxed);
    DisplayButton(ButtonSprite::Caution);
}

fn finish_primary_button_drag() {
    LEFT_CLK_DOWN.store(false, Ordering::Relaxed);
    if status_play() {
        DoButton1Up();
    } else {
        TrackMouse(-2, -2);
    }
}

fn handle_mouse_move(w_param: usize, l_param: isize) {
    if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
        if status_play() {
            TrackMouse(
                x_box_from_xpos(loword(l_param)),
                y_box_from_ypos(hiword(l_param)),
            );
        } else {
            finish_primary_button_drag();
        }
    } else {
        handle_xyzzys_mouse(w_param, l_param);
    }
}

fn handle_rbutton_down(h_wnd: HWND, w_param: usize, l_param: isize) -> Option<isize> {
    if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) {
        return Some(0);
    }

    if !status_play() {
        return None;
    }

    if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
        TrackMouse(-3, -3);
        set_block_flag(true);
        let hwnd_main = {
            let state = global_state();
            let guard = match state.hwnd_main.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            unsafe { HWND::from_ptr(guard.ptr()) }
        };
        unsafe {
            let _ = hwnd_main.PostMessage(WndMsg::new(co::WM::MOUSEMOVE, w_param, l_param));
        }
        return Some(0);
    }

    if (w_param & co::MK::LBUTTON.raw() as usize) != 0 {
        begin_primary_button_drag(h_wnd);
        handle_mouse_move(w_param, l_param);
        return None;
    }

    if !APP_PAUSED.load(Ordering::Relaxed) {
        MakeGuess(
            x_box_from_xpos(loword(l_param)),
            y_box_from_ypos(hiword(l_param)),
        );
    }

    Some(0)
}

fn menu_command(w_param: usize) -> Option<MenuCommand> {
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

fn handle_command(w_param: usize, _l_param: isize) -> Option<isize> {
    match menu_command(w_param) {
        Some(MenuCommand::New) => StartGame(),
        Some(MenuCommand::Exit) => {
            let state = global_state();
            let hwnd_main = {
                let guard = match state.hwnd_main.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                unsafe { HWND::from_ptr(guard.ptr()) }
            };
            hwnd_main.ShowWindow(co::SW::HIDE);
            if let Some(hwnd) = hwnd_main.as_opt() {
                unsafe {
                    let _ = hwnd.SendMessage(WndMsg::new(
                        co::WM::SYSCOMMAND,
                        co::SC::CLOSE.raw() as usize,
                        0,
                    ));
                }
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

            let (preset, f_color, f_mark, f_sound, f_menu) = {
                let mut prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(data) = preset_data(game) {
                    prefs.wGameType = game;
                    prefs.Mines = data[0];
                    prefs.Height = data[1];
                    prefs.Width = data[2];
                }
                (game, prefs.fColor, prefs.fMark, prefs.fSound, prefs.fMenu)
            };
            StartGame();
            UPDATE_INI.store(true, Ordering::Relaxed);
            FixMenus(preset, f_color, f_mark, f_sound);
            SetMenuBar(f_menu);
        }
        Some(MenuCommand::Custom) => DoPref(),
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
                    EndTunes();
                    SoundState::Off
                }
                SoundState::Off => FInitTunes(),
            };
            let (game, f_color, f_mark, f_menu) = {
                let mut prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fSound = new_sound;
                (prefs.wGameType, prefs.fColor, prefs.fMark, prefs.fMenu)
            };
            UPDATE_INI.store(true, Ordering::Relaxed);
            FixMenus(game, f_color, f_mark, new_sound);
            SetMenuBar(f_menu);
        }
        Some(MenuCommand::Color) => {
            let (color_enabled, game, f_mark, f_sound, f_menu) = {
                let mut prefs = match preferences_mutex().lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fColor = !prefs.fColor;
                (
                    prefs.fColor,
                    prefs.wGameType,
                    prefs.fMark,
                    prefs.fSound,
                    prefs.fMenu,
                )
            };
            let state = global_state();
            FreeBitmaps();
            if let Err(e) = FLoadBitmaps() {
                eprintln!("Failed to reload bitmaps: {}", e);
                ReportErr(ID_ERR_MEM);
                let hwnd_main = {
                    let guard = match state.hwnd_main.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    unsafe { HWND::from_ptr(guard.ptr()) }
                };
                if let Some(hwnd) = hwnd_main.as_opt() {
                    unsafe {
                        let _ = hwnd.SendMessage(WndMsg::new(
                            co::WM::SYSCOMMAND,
                            co::SC::CLOSE.raw() as usize,
                            0,
                        ));
                    }
                }
                return Some(0);
            }

            // Repaint immediately so toggling color off updates without restarting.
            DisplayScreen();
            UPDATE_INI.store(true, Ordering::Relaxed);
            FixMenus(game, color_enabled, f_mark, f_sound);
            SetMenuBar(f_menu);
        }
        Some(MenuCommand::Mark) => {
            let (game, color_enabled, mark_enabled, f_sound, f_menu) = {
                let mut prefs = match preferences_mutex().lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fMark = !prefs.fMark;
                (
                    prefs.wGameType,
                    prefs.fColor,
                    prefs.fMark,
                    prefs.fSound,
                    prefs.fMenu,
                )
            };
            UPDATE_INI.store(true, Ordering::Relaxed);
            FixMenus(game, color_enabled, mark_enabled, f_sound);
            SetMenuBar(f_menu);
        }
        Some(MenuCommand::Best) => DoDisplayBest(),
        Some(MenuCommand::Help) => DoHelp(HELPW::INDEX.raw() as u16, HH_DISPLAY_TOPIC as u32),
        Some(MenuCommand::HowToPlay) => {
            DoHelp(HELPW::CONTEXT.raw() as u16, HH_DISPLAY_INDEX as u32)
        }
        Some(MenuCommand::HelpHelp) => {
            DoHelp(HELPW::HELPONHELP.raw() as u16, HH_DISPLAY_TOPIC as u32)
        }
        Some(MenuCommand::HelpAbout) => {
            DoAbout();
            return Some(0);
        }
        None => {}
    }

    None
}

fn handle_keydown(w_param: usize) {
    match w_param as u32 {
        code if code == co::VK::F4.raw() as u32 => {
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
                        EndTunes();
                        SoundState::Off
                    }
                    SoundState::Off => FInitTunes(),
                };

                let (game, color_enabled, mark_enabled, f_menu) = {
                    let mut prefs = match preferences_mutex().lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    prefs.fSound = new_sound;
                    (prefs.wGameType, prefs.fColor, prefs.fMark, prefs.fMenu)
                };

                UPDATE_INI.store(true, Ordering::Relaxed);
                FixMenus(game, color_enabled, mark_enabled, new_sound);
                SetMenuBar(f_menu);
            }
        }
        code if code == co::VK::F5.raw() as u32 => {
            let menu_value = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fMenu
            };

            if !matches!(menu_value, MenuMode::AlwaysOn) {
                SetMenuBar(MenuMode::Hidden);
            }
        }
        code if code == co::VK::F6.raw() as u32 => {
            let menu_value = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                prefs.fMenu
            };

            if !matches!(menu_value, MenuMode::AlwaysOn) {
                SetMenuBar(MenuMode::On);
            }
        }
        code if code == co::VK::SHIFT.raw() as u32 => handle_xyzzys_shift(),
        _ => handle_xyzzys_default_key(w_param),
    }
}

fn handle_window_pos_changed(l_param: isize) {
    if status_icon() || l_param == 0 {
        return;
    }

    let pos = unsafe { &*(l_param as *const WINDOWPOS) };
    if let Ok(mut prefs) = preferences_mutex().lock() {
        prefs.xWindow = pos.x;
        prefs.yWindow = pos.y;
    } else if let Err(poisoned) = preferences_mutex().lock() {
        let mut guard = poisoned.into_inner();
        guard.xWindow = pos.x;
        guard.yWindow = pos.y;
    }
}

fn handle_syscommand(w_param: usize) {
    // Isolate the system command identifier by masking out the lower 4 bits.
    let command = (w_param & 0xFFF0) as u32;
    if command == co::SC::MINIMIZE.raw() {
        PauseGame();
        set_status_pause();
        set_status_icon();
    } else if command == co::SC::RESTORE.raw() {
        clr_status_pause();
        clr_status_icon();
        ResumeGame();
        IGNORE_NEXT_CLICK.store(false, Ordering::Relaxed);
    }
}

fn in_range(x: i32, y: i32) -> bool {
    let x_max = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_max = BOARD_HEIGHT.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

fn board_index(x: i32, y: i32) -> usize {
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    offset.max(0) as usize
}

fn cell_is_bomb(x: i32, y: i32) -> bool {
    if !in_range(x, y) {
        return false;
    }
    let idx = board_index(x, y);
    if idx >= C_BLK_MAX {
        return false;
    }
    let guard = match board_mutex().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    (guard[idx] as u8 & BlockMask::Bomb as u8) != 0
}

/* XYZZY Cheat Code Handling */

/// Length of the XYZZY cheat code sequence.
const CCH_XYZZY: i32 = 5;
/// Atomic counter tracking the progress of the XYZZY cheat code entry.
static I_XYZZY: AtomicI32 = AtomicI32::new(0);
const XYZZY_SEQUENCE: [u16; 5] = [
    b'X' as u16,
    b'Y' as u16,
    b'Z' as u16,
    b'Z' as u16,
    b'Y' as u16,
];

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
fn handle_xyzzys_default_key(w_param: usize) {
    let current = I_XYZZY.load(Ordering::Relaxed);
    if current < CCH_XYZZY {
        let expected = XYZZY_SEQUENCE[current as usize];
        if expected == (w_param & 0xFFFF) as u16 {
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
///
/// # Arguments
///
/// * `w_param` - The WPARAM from the mouse move message, containing key states.
/// * `l_param` - The LPARAM from the mouse move message, containing cursor position.
fn handle_xyzzys_mouse(w_param: usize, l_param: isize) {
    // Check if the XYZZY cheat code is active.
    let state = I_XYZZY.load(Ordering::Relaxed);
    if state == 0 {
        return;
    }

    // Check if the Control key is held down.
    let control_down = (w_param & co::MK::CONTROL.raw() as usize) != 0;
    if (state == CCH_XYZZY && control_down) || state > CCH_XYZZY {
        let x_pos = x_box_from_xpos(loword(l_param));
        let y_pos = y_box_from_ypos(hiword(l_param));
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
            HPEN::CreatePen(co::PS::SOLID, 0, color)
                .and_then(|mut pen| {
                    let mut old_pen = hdc.SelectObject(&pen.leak())?;
                    hdc.MoveToEx(0, 0, None)?;
                    // LineTo excludes the endpoint, so drawing to (1,0) sets pixel (0,0)
                    hdc.LineTo(1, 0)?;
                    hdc.SelectObject(&old_pen.leak())?;
                    Ok(())
                })
                .unwrap_or_else(|e| {
                    eprintln!("Failed to draw pixel at (0,0): {}", e);
                });
        }
    }
}

pub extern "system" fn MainWndProc(
    h_wnd: HWND,
    message: co::WM,
    w_param: usize,
    l_param: isize,
) -> isize {
    match message {
        co::WM::WINDOWPOSCHANGED => handle_window_pos_changed(l_param),
        co::WM::SYSCOMMAND => handle_syscommand(w_param),
        co::WM::COMMAND => {
            if let Some(result) = handle_command(w_param, l_param) {
                return result;
            }
        }
        co::WM::KEYDOWN => handle_keydown(w_param),
        co::WM::DESTROY => {
            let _ = h_wnd.KillTimer(ID_TIMER);
            PostQuitMessage(0);
        }
        co::WM::MBUTTONDOWN => {
            if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) {
                return 0;
            }
            if status_play() {
                set_block_flag(true);
                let hwnd_copy = unsafe { HWND::from_ptr(h_wnd.ptr()) };
                begin_primary_button_drag(hwnd_copy);
                handle_mouse_move(w_param, l_param);
            }
        }
        co::WM::LBUTTONDOWN => {
            if IGNORE_NEXT_CLICK.swap(false, Ordering::Relaxed) {
                return 0;
            }
            if FLocalButton(l_param) {
                return 0;
            }
            if status_play() {
                // Mask SHIFT and RBUTTON to indicate a "chord" operation.
                set_block_flag(
                    (w_param & (co::MK::SHIFT.raw() | co::MK::RBUTTON.raw()) as usize) != 0,
                );
                let hwnd_copy = unsafe { HWND::from_ptr(h_wnd.ptr()) };
                begin_primary_button_drag(hwnd_copy);
                handle_mouse_move(w_param, l_param);
            }
        }
        co::WM::MOUSEMOVE => handle_mouse_move(w_param, l_param),
        co::WM::RBUTTONUP | co::WM::MBUTTONUP | co::WM::LBUTTONUP => {
            if LEFT_CLK_DOWN.load(Ordering::Relaxed) {
                finish_primary_button_drag();
            }
        }
        co::WM::RBUTTONDOWN => {
            let hwnd_copy = unsafe { HWND::from_ptr(h_wnd.ptr()) };
            if let Some(result) = handle_rbutton_down(hwnd_copy, w_param, l_param) {
                return result;
            }
        }
        co::WM::ACTIVATE => {
            if (w_param & 0xFFFF) as u16 == co::WA::CLICKACTIVE.raw() {
                IGNORE_NEXT_CLICK.store(true, Ordering::Relaxed);
            }
        }
        co::WM::TIMER => {
            DoTimer();
            return 0;
        }
        co::WM::ENTERMENULOOP => APP_PAUSED.store(true, Ordering::Relaxed),
        co::WM::EXITMENULOOP => APP_PAUSED.store(false, Ordering::Relaxed),
        co::WM::PAINT => {
            if let Ok(paint_guard) = h_wnd.BeginPaint() {
                DrawScreen(&paint_guard);
            }
            return 0;
        }
        _ => {}
    }
    unsafe { h_wnd.DefWindowProc(WndMsg::new(message, w_param, l_param)) }
}

pub fn FixMenus(game: GameType, f_color: bool, f_mark: bool, f_sound: SoundState) {
    // Keep the menu checkmarks synchronized with the current difficulty/option flags.
    CheckEm(MenuCommand::Begin, game == GameType::Begin);
    CheckEm(MenuCommand::Inter, game == GameType::Inter);
    CheckEm(MenuCommand::Expert, game == GameType::Expert);
    CheckEm(MenuCommand::Custom, game == GameType::Other);

    CheckEm(MenuCommand::Color, f_color);
    CheckEm(MenuCommand::Mark, f_mark);
    CheckEm(MenuCommand::Sound, f_sound == SoundState::On);
}

pub fn DoPref() {
    // Launch the custom game dialog, then treat the result as a "Custom" board.
    show_dialog(DialogTemplateId::Pref as u16, PrefDlgProc);

    let (game, f_color, f_mark, f_sound) = {
        let mut prefs = match preferences_mutex().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        prefs.wGameType = GameType::Other;
        (prefs.wGameType, prefs.fColor, prefs.fMark, prefs.fSound)
    };
    FixMenus(game, f_color, f_mark, f_sound);
    UPDATE_INI.store(true, Ordering::Relaxed);
    StartGame();
}

pub fn DoEnterName() {
    // Show the high-score entry dialog and mark preferences dirty.
    show_dialog(DialogTemplateId::Enter as u16, EnterDlgProc);
    UPDATE_INI.store(true, Ordering::Relaxed);
}

pub fn DoDisplayBest() {
    // Present the high-score list dialog as-is; no post-processing required here.
    show_dialog(DialogTemplateId::Best as u16, BestDlgProc);
}

/// Handles clicks on the smiley face button, providing the pressed animation
/// and starting a new game if clicked.
/// # Arguments
/// * `l_param` - The LPARAM from the mouse click message, containing cursor position.
/// # Returns
/// * `bool` - Returns true if the button was clicked and handled, false otherwise.
pub fn FLocalButton(l_param: isize) -> bool {
    let state = global_state();
    let hwnd_main = {
        let guard = match state.hwnd_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe { HWND::from_ptr(guard.ptr()) }
    };

    // Handle clicks on the smiley face button while providing the pressed animation.
    let mut msg = MSG::default();

    msg.pt.x = loword(l_param);
    msg.pt.y = hiword(l_param);

    let dx_window = WINDOW_WIDTH.load(Ordering::Relaxed);
    let mut rc = RECT {
        left: (dx_window - DX_BUTTON) >> 1,
        top: DY_TOP_LED,
        right: 0,
        bottom: 0,
    };
    rc.right = rc.left + DX_BUTTON;
    rc.bottom = rc.top + DY_BUTTON;

    if !winsafe::PtInRect(rc, msg.pt) {
        return false;
    }

    let mut capture_guard = hwnd_main.as_opt().map(|hwnd| hwnd.SetCapture());
    DisplayButton(ButtonSprite::Down);
    if let Some(hwnd) = hwnd_main.as_opt() {
        let _ = hwnd.MapWindowPoints(&HWND::NULL, PtsRc::Rc(&mut rc));
    }

    let mut pressed = true;
    let hwnd_opt = hwnd_main.as_opt();
    loop {
        if PeekMessage(
            &mut msg,
            hwnd_opt,
            co::WM::MOUSEFIRST.raw(),
            co::WM::MOUSELAST.raw(),
            co::PM::REMOVE,
        ) {
            match msg.message {
                co::WM::LBUTTONUP => {
                    if pressed && winsafe::PtInRect(rc, msg.pt) {
                        BTN_FACE_STATE.store(ButtonSprite::Happy as u8, Ordering::Relaxed);
                        DisplayButton(ButtonSprite::Happy);
                        StartGame();
                    }
                    capture_guard.take();
                    return true;
                }
                co::WM::MOUSEMOVE => {
                    if winsafe::PtInRect(rc, msg.pt) {
                        if !pressed {
                            pressed = true;
                            DisplayButton(ButtonSprite::Down);
                        }
                    } else if pressed {
                        pressed = false;
                        DisplayButton(current_face_sprite());
                    }
                }
                _ => {}
            }
        }
    }
}

pub extern "system" fn PrefDlgProc(
    h_dlg: HWND,
    message: co::WM,
    w_param: usize,
    l_param: isize,
) -> isize {
    // Custom game dialog mirroring the legacy behavior and help wiring.
    let h_dlg_raw = h_dlg.ptr();
    match message {
        co::WM::INITDIALOG => {
            let (height, width, mines) = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                (prefs.Height, prefs.Width, prefs.Mines)
            };
            unsafe {
                SetDlgItemInt(
                    h_dlg_raw as _,
                    ControlId::EditHeight as i32,
                    height as u32,
                    0,
                );
                SetDlgItemInt(h_dlg_raw as _, ControlId::EditWidth as i32, width as u32, 0);
                SetDlgItemInt(h_dlg_raw as _, ControlId::EditMines as i32, mines as u32, 0);
            }
            return 1;
        }
        co::WM::COMMAND => {
            match command_id(w_param) {
                id if id == ControlId::BtnOk as u16 || id == co::DLGID::OK.raw() => {
                    let height = GetDlgInt(&h_dlg, ControlId::EditHeight as i32, MINHEIGHT, 24);
                    let width = GetDlgInt(&h_dlg, ControlId::EditWidth as i32, MINWIDTH, 30);
                    let max_mines = min(999, (height - 1) * (width - 1));
                    let mines = GetDlgInt(&h_dlg, ControlId::EditMines as i32, 10, max_mines);

                    let lock = preferences_mutex().lock();
                    if let Ok(mut prefs) = lock {
                        prefs.Height = height;
                        prefs.Width = width;
                        prefs.Mines = mines;
                    } else if let Err(poisoned) = preferences_mutex().lock() {
                        let mut prefs = poisoned.into_inner();
                        prefs.Height = height;
                        prefs.Width = width;
                        prefs.Mines = mines;
                    }
                }
                id if id == ControlId::BtnCancel as u16 || id == co::DLGID::CANCEL.raw() => {}
                _ => return 0,
            }
            let _ = h_dlg.EndDialog(1);
            return 1;
        }
        co::WM::HELP => {
            if apply_help_from_info(l_param, &PREF_HELP_IDS) {
                return 1;
            }
        }
        co::WM::CONTEXTMENU => {
            let target = unsafe { HWND::from_ptr(w_param as _) };
            apply_help_to_hwnd(target, &PREF_HELP_IDS);
            return 1;
        }
        _ => {}
    }
    0
}

pub extern "system" fn BestDlgProc(
    h_dlg: HWND,
    message: co::WM,
    w_param: usize,
    l_param: isize,
) -> isize {
    // High-score dialog with reset + context help support.
    match message {
        co::WM::INITDIALOG => {
            let snapshot = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                (
                    prefs.rgTime[GameType::Begin as usize],
                    prefs.rgTime[GameType::Inter as usize],
                    prefs.rgTime[GameType::Expert as usize],
                    prefs.szBegin,
                    prefs.szInter,
                    prefs.szExpert,
                )
            };
            let (time_begin, time_inter, time_expert, name_begin, name_inter, name_expert) =
                snapshot;
            reset_best_dialog(
                &h_dlg,
                time_begin,
                time_inter,
                time_expert,
                name_begin,
                name_inter,
                name_expert,
            );
            return 1;
        }
        co::WM::COMMAND => match command_id(w_param) {
            id if id == ControlId::BtnReset as u16 => {
                let snapshot = if let Ok(mut prefs) = preferences_mutex().lock() {
                    prefs.rgTime[GameType::Begin as usize] = 999;
                    prefs.rgTime[GameType::Inter as usize] = 999;
                    prefs.rgTime[GameType::Expert as usize] = 999;
                    copy_from_default(&mut prefs.szBegin);
                    copy_from_default(&mut prefs.szInter);
                    copy_from_default(&mut prefs.szExpert);
                    (
                        prefs.rgTime[GameType::Begin as usize],
                        prefs.rgTime[GameType::Inter as usize],
                        prefs.rgTime[GameType::Expert as usize],
                        prefs.szBegin,
                        prefs.szInter,
                        prefs.szExpert,
                    )
                } else if let Err(poisoned) = preferences_mutex().lock() {
                    let mut prefs = poisoned.into_inner();
                    prefs.rgTime[GameType::Begin as usize] = 999;
                    prefs.rgTime[GameType::Inter as usize] = 999;
                    prefs.rgTime[GameType::Expert as usize] = 999;
                    copy_from_default(&mut prefs.szBegin);
                    copy_from_default(&mut prefs.szInter);
                    copy_from_default(&mut prefs.szExpert);
                    (
                        prefs.rgTime[GameType::Begin as usize],
                        prefs.rgTime[GameType::Inter as usize],
                        prefs.rgTime[GameType::Expert as usize],
                        prefs.szBegin,
                        prefs.szInter,
                        prefs.szExpert,
                    )
                } else {
                    (
                        999,
                        999,
                        999,
                        [0; CCH_NAME_MAX],
                        [0; CCH_NAME_MAX],
                        [0; CCH_NAME_MAX],
                    )
                };

                let (time_begin, time_inter, time_expert, name_begin, name_inter, name_expert) =
                    snapshot;

                UPDATE_INI.store(true, Ordering::Relaxed);
                reset_best_dialog(
                    &h_dlg,
                    time_begin,
                    time_inter,
                    time_expert,
                    name_begin,
                    name_inter,
                    name_expert,
                );
                return 1;
            }
            id if id == ControlId::BtnOk as u16
                || id == co::DLGID::OK.raw()
                || id == ControlId::BtnCancel as u16
                || id == co::DLGID::CANCEL.raw() =>
            {
                let _ = h_dlg.EndDialog(1);
                return 1;
            }
            _ => {}
        },
        co::WM::HELP => {
            if apply_help_from_info(l_param, &BEST_HELP_IDS) {
                return 1;
            }
        }
        co::WM::CONTEXTMENU => {
            let target = unsafe { HWND::from_ptr(w_param as _) };
            apply_help_to_hwnd(target, &BEST_HELP_IDS);
            return 1;
        }
        _ => {}
    }
    0
}

pub extern "system" fn EnterDlgProc(
    h_dlg: HWND,
    message: co::WM,
    w_param: usize,
    _l_param: isize,
) -> isize {
    // Name entry dialog shown when a player beats a high score.
    let h_dlg_raw = h_dlg.ptr();
    match message {
        co::WM::INITDIALOG => {
            let (game_type, current_name) = {
                let prefs = match preferences_mutex().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let name = match prefs.wGameType {
                    GameType::Begin => prefs.szBegin,
                    GameType::Inter => prefs.szInter,
                    _ => prefs.szExpert,
                };
                (prefs.wGameType, name)
            };

            unsafe {
                let mut buffer = [0u16; CCH_MSG_MAX];
                let string_id = ID_MSG_BEGIN + game_type as u16;
                if let Err(e) = LoadSz(string_id, buffer.as_mut_ptr(), buffer.len() as u32) {
                    eprintln!("Failed to load dialog string {}: {}", string_id, e);
                } else {
                    SetDlgItemTextW(h_dlg_raw as _, ControlId::TextBest as i32, buffer.as_ptr());
                }
                if let Ok(edit_hwnd) = h_dlg.GetDlgItem(ControlId::EditName as u16) {
                    let _ = edit_hwnd.SendMessage(WndMsg::new(
                        co::WM::from_raw(co::EM::SETLIMITTEXT.raw()),
                        CCH_NAME_MAX,
                        0,
                    ));
                }
                SetDlgItemTextW(
                    h_dlg_raw as _,
                    ControlId::EditName as i32,
                    current_name.as_ptr(),
                );
            }
            return 1;
        }
        co::WM::COMMAND => match command_id(w_param) {
            id if id == ControlId::BtnOk as u16
                || id == co::DLGID::OK.raw()
                || id == ControlId::BtnCancel as u16
                || id == co::DLGID::CANCEL.raw() =>
            {
                let mut buffer = [0u16; CCH_NAME_MAX];
                unsafe {
                    GetDlgItemTextW(
                        h_dlg_raw as _,
                        ControlId::EditName as i32,
                        buffer.as_mut_ptr(),
                        CCH_NAME_MAX as i32,
                    );
                }

                let lock = preferences_mutex().lock();
                if let Ok(mut prefs) = lock {
                    match prefs.wGameType {
                        GameType::Begin => prefs.szBegin = buffer,
                        GameType::Inter => prefs.szInter = buffer,
                        _ => prefs.szExpert = buffer,
                    }
                } else if let Err(poisoned) = preferences_mutex().lock() {
                    let mut prefs = poisoned.into_inner();
                    match prefs.wGameType {
                        GameType::Begin => prefs.szBegin = buffer,
                        GameType::Inter => prefs.szInter = buffer,
                        _ => prefs.szExpert = buffer,
                    }
                }

                let _ = h_dlg.EndDialog(1);
                return 1;
            }
            _ => {}
        },
        _ => {}
    }

    0
}

pub fn AdjustWindow(mut f_adjust: i32) {
    // Recompute the main window rectangle whenever the board or menu state changes.
    let state = global_state();
    let hwnd_main = {
        let guard = match state.hwnd_main.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        unsafe { HWND::from_ptr(guard.ptr()) }
    };
    if hwnd_main.as_opt().is_none() {
        return;
    }

    let menu_handle = {
        let guard = match state.h_menu.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .as_ref()
            .map(|menu| unsafe { HMENU::from_ptr(menu.ptr()) })
            .unwrap_or(HMENU::NULL)
    };

    let x_boxes = BOARD_WIDTH.load(Ordering::Relaxed);
    let y_boxes = BOARD_HEIGHT.load(Ordering::Relaxed);
    let dx_window = DX_BLK * x_boxes + DX_GRID_OFF + DX_RIGHT_SPACE;
    let dy_window = DY_BLK * y_boxes + DY_GRID_OFF + DY_BOTTOM_SPACE;
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
        && let Some(hwnd) = hwnd_main.as_opt()
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
    let dw_style = hwnd_main.GetWindowLongPtr(GWLP::STYLE) as u32;
    let dw_ex_style = hwnd_main.GetWindowLongPtr(GWLP::EXSTYLE) as u32;
    let mut frame_extra = CXBORDER.load(Ordering::Relaxed);
    let mut dyp_adjust;
    if let Ok(adjusted) = unsafe {
        AdjustWindowRectEx(
            desired,
            WS::from_raw(dw_style),
            menu_visible,
            WS_EX::from_raw(dw_ex_style),
        )
    } {
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
            let _ = hwnd_main.MoveWindow(
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
                    hwnd_main
                        .GetMenuItemRect(menu, 0)
                        .ok()
                        .zip(hwnd_main.GetMenuItemRect(menu, 1).ok())
                })
                .is_some_and(|(g, h)| g.top == h.top)
        {
            dyp_adjust -= CYMENU.load(Ordering::Relaxed);
            WND_Y_OFFSET.store(dyp_adjust, Ordering::Relaxed);
            let _ = hwnd_main.MoveWindow(
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
            let _ = hwnd_main.InvalidateRect(Some(&rect), true);
        }
    }

    if let Ok(mut prefs) = preferences_mutex().lock() {
        prefs.xWindow = x_window;
        prefs.yWindow = y_window;
    } else if let Err(poisoned) = preferences_mutex().lock() {
        let mut guard = poisoned.into_inner();
        guard.xWindow = x_window;
        guard.yWindow = y_window;
    }
}

fn our_get_system_metrics(index: SM) -> i32 {
    // Favor the virtual screen metrics when available to support multi-monitor setups.
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

fn loword(value: isize) -> i32 {
    ((value as u32) & 0xFFFF) as i16 as i32
}

fn hiword(value: isize) -> i32 {
    (((value as u32) >> 16) & 0xFFFF) as i16 as i32
}

fn command_id(w_param: usize) -> u16 {
    (w_param & 0xFFFF) as u16
}

fn set_dtext(h_dlg: &HWND, id: i32, time: i32, name: &[u16; CCH_NAME_MAX]) {
    let state = global_state();
    let time_fmt = {
        let guard = match state.sz_time.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let len = guard.iter().position(|&ch| ch == 0).unwrap_or(guard.len());
        String::from_utf16_lossy(&guard[..len])
    };

    let mut buffer = [0u16; CCH_NAME_MAX];
    let text = time_fmt.replace("%d", &time.to_string());
    for (i, code_unit) in text
        .encode_utf16()
        .chain(Some(0))
        .take(buffer.len())
        .enumerate()
    {
        buffer[i] = code_unit;
    }

    unsafe {
        SetDlgItemTextW(h_dlg.ptr() as _, id, buffer.as_ptr());
        SetDlgItemTextW(h_dlg.ptr() as _, id + 1, name.as_ptr());
    }
}

fn reset_best_dialog(
    h_dlg: &HWND,
    time_begin: i32,
    time_inter: i32,
    time_expert: i32,
    name_begin: [u16; CCH_NAME_MAX],
    name_inter: [u16; CCH_NAME_MAX],
    name_expert: [u16; CCH_NAME_MAX],
) {
    set_dtext(h_dlg, ControlId::TimeBegin as i32, time_begin, &name_begin);
    set_dtext(h_dlg, ControlId::TimeInter as i32, time_inter, &name_inter);
    set_dtext(
        h_dlg,
        ControlId::TimeExpert as i32,
        time_expert,
        &name_expert,
    );
}

fn copy_from_default(dst: &mut [u16; CCH_NAME_MAX]) {
    let state = global_state();
    let source = {
        let guard = match state.sz_default_name.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard
    };

    for (i, ch) in source.iter().copied().enumerate().take(CCH_NAME_MAX) {
        dst[i] = ch;
        if ch == 0 {
            return;
        }
    }
    dst[CCH_NAME_MAX - 1] = 0;
}

fn apply_help_from_info(l_param: isize, ids: &[u32]) -> bool {
    if l_param == 0 {
        return false;
    }

    let info = unsafe { &*(l_param as *const HelpInfo) };
    let Some(hwnd) = info.hItemHandle.as_opt() else {
        return false;
    };

    unsafe {
        HtmlHelpA(
            hwnd.ptr() as _,
            HELP_FILE.as_ptr() as _,
            HH_TP_HELP_WM_HELP as _,
            ids.as_ptr() as usize,
        );
    }

    true
}

fn apply_help_to_hwnd(hwnd: HWND, ids: &[u32]) {
    if let Some(control) = hwnd.as_opt() {
        unsafe {
            HtmlHelpA(
                control.ptr() as _,
                HELP_FILE.as_ptr() as _,
                HH_TP_HELP_CONTEXTMENU as _,
                ids.as_ptr() as usize,
            );
        }
    }
}
