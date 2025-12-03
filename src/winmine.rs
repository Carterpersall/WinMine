use core::cmp::{max, min};
use core::ffi::c_int;
use core::mem;
use core::ptr::{addr_of, addr_of_mut, null_mut};

use windows_sys::core::{w, BOOL, PCWSTR, PSTR};
use windows_sys::Win32::Data::HtmlHelp::{HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC};
#[cfg(not(debug_assertions))]
use windows_sys::Win32::Foundation::COLORREF;
use windows_sys::Win32::Foundation::{
    FALSE, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::LTGRAY_BRUSH;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetStockObject, InvalidateRect, MapWindowPoints, PtInRect, HBRUSH,
    PAINTSTRUCT,
};
#[cfg(not(debug_assertions))]
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, SetPixel};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_ANIMATE_CLASS, ICC_BAR_CLASSES, ICC_COOL_CLASSES, ICC_HOTKEY_CLASS,
    ICC_LISTVIEW_CLASSES, ICC_PAGESCROLLER_CLASS, ICC_PROGRESS_CLASS, ICC_TAB_CLASSES,
    ICC_UPDOWN_CLASS, ICC_USEREX_CLASSES, INITCOMMONCONTROLSEX,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_F4, VK_F5, VK_F6, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::WinHelpW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    wsprintfW, GWL_EXSTYLE, GWL_STYLE, HACCEL, HELP_CONTEXT, HELP_CONTEXTMENU, HELP_HELPONHELP,
    HELP_INDEX, HELP_WM_HELP, HMENU, IDCANCEL, IDC_ARROW, IDOK, MSG, PM_REMOVE, SC_CLOSE,
    SC_MINIMIZE, SC_RESTORE, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SW_HIDE, SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, WS_CAPTION, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DialogBoxParamW, DispatchMessageW,
    EndDialog, GetDlgItem, GetDlgItemTextW, GetMenuItemRect, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, KillTimer, LoadAcceleratorsW, LoadCursorW, LoadIconW, LoadMenuW, MoveWindow,
    PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW, SetDlgItemInt,
    SetDlgItemTextW, ShowWindow, TranslateAcceleratorW, TranslateMessage, WINDOWPOS, WM_ACTIVATE,
    WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_ENTERMENULOOP, WM_EXITMENULOOP, WM_HELP,
    WM_INITDIALOG, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEFIRST, WM_MOUSELAST, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSCOMMAND, WM_TIMER, WM_WINDOWPOSCHANGED, WNDCLASSW,
};

use crate::globals::{
    bInitMinimized, dxFrameExtra, dxWindow, dxpBorder, dyWindow, dypAdjust, dypCaption, dypMenu,
    fBlock, fButton1Down, fIgnoreClick, fLocalPause, fStatus, hIconMain, hInst, hMenu, hwndMain,
    szClass, szDefaultName, szTime,
};
use crate::grafix::{
    CleanUp, DisplayButton, DisplayScreen, DrawScreen, FInitLocal, FLoadBitmaps, FreeBitmaps,
};
use crate::pref::{
    fUpdateIni, ReadPreferences, WritePreferences, CCH_NAME_MAX, FMENU_ALWAYS_ON, FMENU_ON,
    FSOUND_OFF, FSOUND_ON, MINHEIGHT, MINWIDTH, WGAME_BEGIN, WGAME_EXPERT, WGAME_INTER,
};
#[cfg(not(debug_assertions))]
use crate::rtns::rgBlk;
use crate::rtns::{
    iButtonCur, xBoxMac, xCur, yBoxMac, yCur, DoButton1Up, DoTimer, MakeGuess, PauseGame,
    Preferences, ResumeGame, StartGame, TrackMouse,
};
use crate::sound::{EndTunes, FInitTunes};
use crate::util::{CheckEm, DoAbout, DoHelp, GetDlgInt, InitConst, LoadSz, ReportErr, SetMenuBar};

const ID_MENU: u16 = 500;
const ID_MENU_ACCEL: u16 = 501;
const ID_ICON_MAIN: u16 = 100;
const IDM_NEW: u16 = 510;
const IDM_EXIT: u16 = 512;
const IDM_BEGIN: u16 = 521;
const IDM_INTER: u16 = 522;
const IDM_EXPERT: u16 = 523;
const IDM_CUSTOM: u16 = 524;
const IDM_SOUND: u16 = 526;
const IDM_MARK: u16 = 527;
const IDM_BEST: u16 = 528;
const IDM_COLOR: u16 = 529;
const IDM_HELP: u16 = 590;
const IDM_HOW2PLAY: u16 = 591;
const IDM_HELP_HELP: u16 = 592;
const IDM_HELP_ABOUT: u16 = 593;
const ID_ERR_MEM: u16 = 5;
const ID_TIMER: usize = 1;

const ID_DLG_PREF: u16 = 80;
const ID_DLG_ENTER: u16 = 600;
const ID_DLG_BEST: u16 = 700;

const WGAME_OTHER: u16 = 3;
const ID_EDIT_HEIGHT: c_int = 141;
const ID_EDIT_WIDTH: c_int = 142;
const ID_EDIT_MINES: c_int = 143;
const ID_BTN_OK: u16 = 100;
const ID_BTN_CANCEL: u16 = 109;
const ID_BTN_RESET: u16 = 707;
const ID_TEXT_BEST: c_int = 601;
const ID_EDIT_NAME: c_int = 602;
const ID_TIME_BEGIN: c_int = 701;
const ID_NAME_BEGIN: c_int = 702;
const ID_TIME_INTER: c_int = 703;
const ID_NAME_INTER: c_int = 704;
const ID_TIME_EXPERT: c_int = 705;
const ID_NAME_EXPERT: c_int = 706;
const ID_STEXT1: c_int = 708;
const ID_STEXT2: c_int = 709;
const ID_STEXT3: c_int = 710;
const ID_TXT_MINES: c_int = 111;
const ID_TXT_HEIGHT: c_int = 112;
const ID_TXT_WIDTH: c_int = 113;
const IDH_PREF_EDIT_HEIGHT: u32 = 1000;
const IDH_PREF_EDIT_WIDTH: u32 = 1001;
const IDH_PREF_EDIT_MINES: u32 = 1002;
const IDH_BEST_BTN_RESET: u32 = 1003;
const IDH_STEXT: u32 = 1004;
const ID_MSG_BEGIN: u16 = 9;
const CCH_MSG_MAX: usize = 128;

const DX_BLK: c_int = 16;
const DY_BLK: c_int = 16;
const DX_LEFT_SPACE: c_int = 12;
const DX_RIGHT_SPACE: c_int = 12;
const DY_TOP_SPACE: c_int = 12;
const DY_BOTTOM_SPACE: c_int = 12;
const DX_GRID_OFF: c_int = DX_LEFT_SPACE;
const DY_LED: c_int = 23;
const DY_TOP_LED: c_int = DY_TOP_SPACE + 4;
const DY_GRID_OFF: c_int = DY_TOP_LED + DY_LED + 16;
const DX_BUTTON: c_int = 24;
const DY_BUTTON: c_int = 24;

const I_BUTTON_HAPPY: c_int = 0;
const I_BUTTON_CAUTION: c_int = 1;
const I_BUTTON_DOWN: c_int = 4;

const FMENU_FLAG_OFF: c_int = 0x01;
const F_CALC: c_int = 0x01;
const F_RESIZE: c_int = 0x02;
const F_DISPLAY: c_int = 0x04;

const WINDOW_STYLE: u32 = WS_OVERLAPPED | WS_MINIMIZEBOX | WS_CAPTION | WS_SYSMENU;

const LEVEL_DATA: [[c_int; 3]; 3] = [[10, MINHEIGHT, MINWIDTH], [40, 16, 16], [99, 16, 30]];

const F_PLAY: c_int = 0x01;
const F_PAUSE: c_int = 0x02;
const F_ICON: c_int = 0x08;

const FMENU_OFF: c_int = 1;
const SC_MASK: WPARAM = 0xFFF0;
const WA_CLICKACTIVE: u16 = 2;

const MK_LBUTTON: WPARAM = 0x0001;
const MK_RBUTTON: WPARAM = 0x0002;
const MK_SHIFT_FLAG: WPARAM = 0x0004;
const MK_CHORD_MASK: WPARAM = MK_SHIFT_FLAG | MK_RBUTTON;
#[cfg(not(debug_assertions))]
const MK_CONTROL_FLAG: WPARAM = 0x0008;

const VK_F4_CODE: u32 = VK_F4 as u32;
const VK_F5_CODE: u32 = VK_F5 as u32;
const VK_F6_CODE: u32 = VK_F6 as u32;
const VK_SHIFT_CODE: u32 = VK_SHIFT as u32;

#[cfg(not(debug_assertions))]
const C_BLK_MAX: usize = 27 * 32;
#[cfg(not(debug_assertions))]
const BOARD_INDEX_SHIFT: usize = 5;
#[cfg(not(debug_assertions))]
const MASK_BOMB: u8 = 0x80;
#[cfg(not(debug_assertions))]
const COLOR_BLACK: COLORREF = 0x0000_0000;
#[cfg(not(debug_assertions))]
const COLOR_WHITE: COLORREF = 0x00FF_FFFF;

const HELP_FILE: PCWSTR = w!("winmine.hlp");

const PREF_HELP_IDS: [u32; 14] = [
    ID_EDIT_HEIGHT as u32,
    IDH_PREF_EDIT_HEIGHT,
    ID_EDIT_WIDTH as u32,
    IDH_PREF_EDIT_WIDTH,
    ID_EDIT_MINES as u32,
    IDH_PREF_EDIT_MINES,
    ID_TXT_HEIGHT as u32,
    IDH_PREF_EDIT_HEIGHT,
    ID_TXT_WIDTH as u32,
    IDH_PREF_EDIT_WIDTH,
    ID_TXT_MINES as u32,
    IDH_PREF_EDIT_MINES,
    0,
    0,
];

const BEST_HELP_IDS: [u32; 22] = [
    ID_BTN_RESET as u32,
    IDH_BEST_BTN_RESET,
    ID_STEXT1 as u32,
    IDH_STEXT,
    ID_STEXT2 as u32,
    IDH_STEXT,
    ID_STEXT3 as u32,
    IDH_STEXT,
    ID_TIME_BEGIN as u32,
    IDH_STEXT,
    ID_TIME_INTER as u32,
    IDH_STEXT,
    ID_TIME_EXPERT as u32,
    IDH_STEXT,
    ID_NAME_BEGIN as u32,
    IDH_STEXT,
    ID_NAME_INTER as u32,
    IDH_STEXT,
    ID_NAME_EXPERT as u32,
    IDH_STEXT,
    0,
    0,
];

const EM_SETLIMITTEXT: u32 = 0x00C5;
const IDOK_U16: u16 = IDOK as u16;
const IDCANCEL_U16: u16 = IDCANCEL as u16;
const NULL_HWND: HWND = 0 as HWND;
const NULL_HMENU: HMENU = 0 as HMENU;

#[repr(C)]
struct HelpInfo {
    cbSize: u32,
    iContextType: c_int,
    iCtrlId: c_int,
    hItemHandle: HANDLE,
    dwContextId: usize,
    mouse_pos: POINT,
}

type DialogProc = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> isize>;

extern "system" {
    fn UpdateWindow(h_wnd: HWND) -> BOOL;
}

fn show_dialog(template_id: u16, proc: DialogProc) {
    unsafe {
        DialogBoxParamW(hInst, make_int_resource(template_id), hwndMain, proc, 0);
    }
}

fn bool_to_bool(flag: bool) -> BOOL {
    if flag {
        TRUE
    } else {
        FALSE
    }
}

fn int_to_bool(value: c_int) -> BOOL {
    bool_to_bool(value != 0)
}

fn make_int_resource(id: u16) -> PCWSTR {
    id as usize as *const u16
}

unsafe fn class_name_ptr() -> PCWSTR {
    addr_of!(szClass[0])
}

fn initial_minimized_state(n_cmd_show: c_int) -> bool {
    n_cmd_show == SW_SHOWMINNOACTIVE || n_cmd_show == SW_SHOWMINIMIZED
}

unsafe fn init_common_controls() {
    let icc = INITCOMMONCONTROLSEX {
        dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_ANIMATE_CLASS
            | ICC_BAR_CLASSES
            | ICC_COOL_CLASSES
            | ICC_HOTKEY_CLASS
            | ICC_LISTVIEW_CLASSES
            | ICC_PAGESCROLLER_CLASS
            | ICC_PROGRESS_CLASS
            | ICC_TAB_CLASSES
            | ICC_UPDOWN_CLASS
            | ICC_USEREX_CLASSES,
    };
    InitCommonControlsEx(&icc);
}

unsafe fn register_main_window_class() -> bool {
    let mut wc: WNDCLASSW = mem::zeroed();
    wc.lpfnWndProc = Some(MainWndProc);
    wc.hInstance = hInst;
    wc.hIcon = hIconMain;
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.hbrBackground = GetStockObject(LTGRAY_BRUSH) as HBRUSH;
    wc.lpszMenuName = 0 as PCWSTR;
    wc.lpszClassName = class_name_ptr();
    RegisterClassW(&wc) != 0
}

pub unsafe fn run_winmine(
    h_instance: HINSTANCE,
    _h_prev_instance: HINSTANCE,
    _lp_cmd_line: PSTR,
    n_cmd_show: c_int,
) -> c_int {
    hInst = h_instance;
    InitConst();

    bInitMinimized = bool_to_bool(initial_minimized_state(n_cmd_show));

    init_common_controls();
    hIconMain = LoadIconW(hInst, make_int_resource(ID_ICON_MAIN));

    if !register_main_window_class() {
        return FALSE as c_int;
    }

    hMenu = LoadMenuW(hInst, make_int_resource(ID_MENU));
    let h_accel: HACCEL = LoadAcceleratorsW(hInst, make_int_resource(ID_MENU_ACCEL));

    ReadPreferences();

    hwndMain = CreateWindowExW(
        0,
        class_name_ptr(),
        class_name_ptr(),
        WINDOW_STYLE,
        Preferences.xWindow - dxpBorder,
        Preferences.yWindow - dypAdjust,
        dxWindow + dxpBorder,
        dyWindow + dypAdjust,
        NULL_HWND,
        NULL_HMENU,
        hInst,
        null_mut(),
    );

    if hwndMain == NULL_HWND {
        ReportErr(1000);
        return FALSE as c_int;
    }

    AdjustWindow(F_CALC);

    if FInitLocal() == FALSE {
        ReportErr(ID_ERR_MEM);
        return FALSE as c_int;
    }

    SetMenuBar(Preferences.fMenu);
    StartGame();

    ShowWindow(hwndMain, SW_SHOWNORMAL);
    UpdateWindow(hwndMain);

    bInitMinimized = FALSE;

    let mut msg: MSG = mem::zeroed();
    loop {
        let result = GetMessageW(&mut msg, NULL_HWND, 0, 0);
        if result <= 0 {
            break;
        }

        if h_accel.is_null() || TranslateAcceleratorW(hwndMain, h_accel, &msg) == 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    CleanUp();

    if fUpdateIni != FALSE {
        WritePreferences();
    }

    msg.wParam as c_int
}

fn x_box_from_xpos(x: c_int) -> c_int {
    (x - (DX_GRID_OFF - DX_BLK)) >> 4
}

fn y_box_from_ypos(y: c_int) -> c_int {
    (y - (DY_GRID_OFF - DY_BLK)) >> 4
}

unsafe fn status_icon() -> bool {
    (fStatus & F_ICON) != 0
}

unsafe fn status_play() -> bool {
    (fStatus & F_PLAY) != 0
}

unsafe fn set_status_pause() {
    fStatus |= F_PAUSE;
}

unsafe fn clr_status_pause() {
    fStatus &= !F_PAUSE;
}

unsafe fn set_status_icon() {
    fStatus |= F_ICON;
}

unsafe fn clr_status_icon() {
    fStatus &= !F_ICON;
}

unsafe fn set_block_flag(active: bool) {
    fBlock = bool_to_bool(active);
}

unsafe fn begin_primary_button_drag(h_wnd: HWND) {
    SetCapture(h_wnd);
    fButton1Down = TRUE;
    xCur = -1;
    yCur = -1;
    DisplayButton(I_BUTTON_CAUTION);
}

unsafe fn finish_primary_button_drag() {
    fButton1Down = FALSE;
    ReleaseCapture();
    if status_play() {
        DoButton1Up();
    } else {
        TrackMouse(-2, -2);
    }
}

unsafe fn handle_mouse_move(w_param: WPARAM, l_param: LPARAM) {
    if fButton1Down != FALSE {
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

unsafe fn handle_rbutton_down(h_wnd: HWND, w_param: WPARAM, l_param: LPARAM) -> Option<LRESULT> {
    if handle_ignore_click() {
        return Some(0);
    }

    if !status_play() {
        return None;
    }

    if fButton1Down != FALSE {
        TrackMouse(-3, -3);
        set_block_flag(true);
        PostMessageW(hwndMain, WM_MOUSEMOVE, w_param, l_param);
        return Some(0);
    }

    if (w_param & MK_LBUTTON) != 0 {
        begin_primary_button_drag(h_wnd);
        handle_mouse_move(w_param, l_param);
        return None;
    }

    if !local_pause() {
        MakeGuess(
            x_box_from_xpos(loword(l_param)),
            y_box_from_ypos(hiword(l_param)),
        );
    }

    Some(0)
}

unsafe fn handle_command(w_param: WPARAM, _l_param: LPARAM) -> Option<LRESULT> {
    match command_id(w_param) {
        IDM_NEW => StartGame(),
        IDM_EXIT => {
            ShowWindow(hwndMain, SW_HIDE);
            SendMessageW(hwndMain, WM_SYSCOMMAND, SC_CLOSE as WPARAM, 0);
            return Some(0);
        }
        IDM_BEGIN | IDM_INTER | IDM_EXPERT => {
            let index = (command_id(w_param) - IDM_BEGIN) as usize;
            Preferences.wGameType = index as u16;
            Preferences.Mines = LEVEL_DATA[index][0];
            Preferences.Height = LEVEL_DATA[index][1];
            Preferences.Width = LEVEL_DATA[index][2];
            StartGame();
            update_menu_from_preferences();
        }
        IDM_CUSTOM => DoPref(),
        IDM_SOUND => {
            if Preferences.fSound != 0 {
                EndTunes();
                Preferences.fSound = FALSE;
            } else {
                Preferences.fSound = FInitTunes();
            }
            update_menu_from_preferences();
        }
        IDM_COLOR => {
            Preferences.fColor = toggle_bool(Preferences.fColor);
            FreeBitmaps();
            if FLoadBitmaps() == 0 {
                ReportErr(ID_ERR_MEM);
                SendMessageW(hwndMain, WM_SYSCOMMAND, SC_CLOSE as WPARAM, 0);
                return Some(0);
            }
            DisplayScreen();
            update_menu_from_preferences();
        }
        IDM_MARK => {
            Preferences.fMark = toggle_bool(Preferences.fMark);
            update_menu_from_preferences();
        }
        IDM_BEST => DoDisplayBest(),
        IDM_HELP => DoHelp(HELP_INDEX as u16, HH_DISPLAY_TOPIC as u32),
        IDM_HOW2PLAY => DoHelp(HELP_CONTEXT as u16, HH_DISPLAY_INDEX as u32),
        IDM_HELP_HELP => DoHelp(HELP_HELPONHELP as u16, HH_DISPLAY_TOPIC as u32),
        IDM_HELP_ABOUT => {
            DoAbout();
            return Some(0);
        }
        _ => {}
    }

    None
}

unsafe fn handle_keydown(w_param: WPARAM) {
    match w_param as u32 {
        VK_F4_CODE => {
            if sound_switchable() {
                if sound_on() {
                    EndTunes();
                    Preferences.fSound = FSOUND_OFF;
                } else {
                    Preferences.fSound = FInitTunes();
                }
            }
        }
        VK_F5_CODE => {
            if menu_switchable() {
                SetMenuBar(FMENU_OFF);
            }
        }
        VK_F6_CODE => {
            if menu_switchable() {
                SetMenuBar(FMENU_ON);
            }
        }
        VK_SHIFT_CODE => handle_xyzzys_shift(),
        _ => handle_xyzzys_default_key(w_param),
    }
}

unsafe fn handle_window_pos_changed(l_param: LPARAM) {
    if status_icon() || l_param == 0 {
        return;
    }

    let pos = &*(l_param as *const WINDOWPOS);
    Preferences.xWindow = pos.x;
    Preferences.yWindow = pos.y;
}

unsafe fn handle_syscommand(w_param: WPARAM) {
    match (w_param & SC_MASK) as u32 {
        SC_MINIMIZE => {
            PauseGame();
            set_status_pause();
            set_status_icon();
        }
        SC_RESTORE => {
            clr_status_pause();
            clr_status_icon();
            ResumeGame();
            fIgnoreClick = FALSE;
        }
        _ => {}
    }
}

unsafe fn handle_ignore_click() -> bool {
    if fIgnoreClick != FALSE {
        fIgnoreClick = FALSE;
        true
    } else {
        false
    }
}

unsafe fn local_pause() -> bool {
    fLocalPause != FALSE
}

fn menu_switchable() -> bool {
    unsafe { Preferences.fMenu != FMENU_ALWAYS_ON }
}

fn sound_switchable() -> bool {
    unsafe { Preferences.fSound > 1 }
}

fn sound_on() -> bool {
    unsafe { Preferences.fSound == FSOUND_ON }
}

unsafe fn update_menu_from_preferences() {
    fUpdateIni = TRUE;
    SetMenuBar(Preferences.fMenu);
}

fn toggle_bool(value: BOOL) -> BOOL {
    if value == FALSE {
        TRUE
    } else {
        FALSE
    }
}

fn get_activate_state(w_param: WPARAM) -> u16 {
    (w_param & 0xFFFF) as u16
}

#[cfg(not(debug_assertions))]
unsafe fn in_range(x: c_int, y: c_int) -> bool {
    x > 0 && y > 0 && x <= xBoxMac && y <= yBoxMac
}

#[cfg(not(debug_assertions))]
unsafe fn board_index(x: c_int, y: c_int) -> usize {
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    offset.max(0) as usize
}

#[cfg(not(debug_assertions))]
unsafe fn cell_is_bomb(x: c_int, y: c_int) -> bool {
    if !in_range(x, y) {
        return false;
    }
    let idx = board_index(x, y);
    if idx >= C_BLK_MAX {
        return false;
    }
    (rgBlk[idx] as u8 & MASK_BOMB) != 0
}

#[cfg(not(debug_assertions))]
const CCH_XYZZY: c_int = 5;
#[cfg(not(debug_assertions))]
static mut I_XYZZY: c_int = 0;
#[cfg(not(debug_assertions))]
const XYZZY_SEQUENCE: [u16; 5] = [
    b'X' as u16,
    b'Y' as u16,
    b'Z' as u16,
    b'Z' as u16,
    b'Y' as u16,
];

#[cfg(not(debug_assertions))]
unsafe fn handle_xyzzys_shift() {
    if I_XYZZY >= CCH_XYZZY {
        I_XYZZY ^= 20;
    }
}

#[cfg(debug_assertions)]
unsafe fn handle_xyzzys_shift() {}

#[cfg(not(debug_assertions))]
unsafe fn handle_xyzzys_default_key(w_param: WPARAM) {
    if I_XYZZY < CCH_XYZZY {
        if XYZZY_SEQUENCE[I_XYZZY as usize] == (w_param & 0xFFFF) as u16 {
            I_XYZZY += 1;
        } else {
            I_XYZZY = 0;
        }
    }
}

#[cfg(debug_assertions)]
unsafe fn handle_xyzzys_default_key(_w_param: WPARAM) {}

#[cfg(not(debug_assertions))]
unsafe fn handle_xyzzys_mouse(w_param: WPARAM, l_param: LPARAM) {
    if I_XYZZY == 0 {
        return;
    }

    let control_down = (w_param & MK_CONTROL_FLAG) != 0;
    if (I_XYZZY == CCH_XYZZY && control_down) || I_XYZZY > CCH_XYZZY {
        xCur = x_box_from_xpos(loword(l_param));
        yCur = y_box_from_ypos(hiword(l_param));
        if in_range(xCur, yCur) {
            let hdc = GetDC(NULL_HWND);
            if hdc != null_mut() {
                let color = if cell_is_bomb(xCur, yCur) {
                    COLOR_BLACK
                } else {
                    COLOR_WHITE
                };
                SetPixel(hdc, 0, 0, color);
                ReleaseDC(NULL_HWND, hdc);
            }
        }
    }
}

#[cfg(debug_assertions)]
unsafe fn handle_xyzzys_mouse(_w_param: WPARAM, _l_param: LPARAM) {}

pub unsafe extern "system" fn MainWndProc(
    h_wnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match message {
        WM_WINDOWPOSCHANGED => handle_window_pos_changed(l_param),
        WM_SYSCOMMAND => handle_syscommand(w_param),
        WM_COMMAND => {
            if let Some(result) = handle_command(w_param, l_param) {
                return result;
            }
        }
        WM_KEYDOWN => handle_keydown(w_param),
        WM_DESTROY => {
            KillTimer(hwndMain, ID_TIMER);
            PostQuitMessage(0);
        }
        WM_MBUTTONDOWN => {
            if handle_ignore_click() {
                return 0;
            }
            if status_play() {
                set_block_flag(true);
                begin_primary_button_drag(h_wnd);
                handle_mouse_move(w_param, l_param);
            }
        }
        WM_LBUTTONDOWN => {
            if handle_ignore_click() {
                return 0;
            }
            if FLocalButton(l_param) != 0 {
                return 0;
            }
            if status_play() {
                set_block_flag((w_param & MK_CHORD_MASK) != 0);
                begin_primary_button_drag(h_wnd);
                handle_mouse_move(w_param, l_param);
            }
        }
        WM_MOUSEMOVE => handle_mouse_move(w_param, l_param),
        WM_RBUTTONUP | WM_MBUTTONUP | WM_LBUTTONUP => {
            if fButton1Down != FALSE {
                finish_primary_button_drag();
            }
        }
        WM_RBUTTONDOWN => {
            if let Some(result) = handle_rbutton_down(h_wnd, w_param, l_param) {
                return result;
            }
        }
        WM_ACTIVATE => {
            if get_activate_state(w_param) == WA_CLICKACTIVE {
                fIgnoreClick = TRUE;
            }
        }
        WM_TIMER => {
            DoTimer();
            return 0;
        }
        WM_ENTERMENULOOP => fLocalPause = TRUE,
        WM_EXITMENULOOP => fLocalPause = FALSE,
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = mem::zeroed();
            let hdc = BeginPaint(h_wnd, &mut paint);
            DrawScreen(hdc);
            EndPaint(h_wnd, &paint);
            return 0;
        }
        _ => {}
    }

    DefWindowProcW(h_wnd, message, w_param, l_param)
}

pub unsafe fn FixMenus() {
    // Keep the menu checkmarks synchronized with the current difficulty/option flags.
    let game = Preferences.wGameType;
    CheckEm(IDM_BEGIN, bool_to_bool(game == WGAME_BEGIN as u16));
    CheckEm(IDM_INTER, bool_to_bool(game == WGAME_INTER as u16));
    CheckEm(IDM_EXPERT, bool_to_bool(game == WGAME_EXPERT as u16));
    CheckEm(IDM_CUSTOM, bool_to_bool(game == WGAME_OTHER));

    CheckEm(IDM_COLOR, Preferences.fColor);
    CheckEm(IDM_MARK, Preferences.fMark);
    CheckEm(IDM_SOUND, int_to_bool(Preferences.fSound));
}

pub unsafe fn DoPref() {
    // Launch the custom game dialog, then treat the result as a "Custom" board.
    show_dialog(ID_DLG_PREF, Some(PrefDlgProc));

    Preferences.wGameType = WGAME_OTHER;
    FixMenus();
    fUpdateIni = TRUE;
    StartGame();
}

pub unsafe fn DoEnterName() {
    // Show the high-score entry dialog and mark preferences dirty.
    show_dialog(ID_DLG_ENTER, Some(EnterDlgProc));
    fUpdateIni = TRUE;
}

pub unsafe fn DoDisplayBest() {
    // Present the high-score list dialog as-is; no post-processing required here.
    show_dialog(ID_DLG_BEST, Some(BestDlgProc));
}

pub unsafe fn FLocalButton(l_param: LPARAM) -> BOOL {
    // Handle clicks on the smiley face button while providing the pressed animation.
    let mut msg: MSG = core::mem::zeroed();

    msg.pt.x = loword(l_param);
    msg.pt.y = hiword(l_param);

    let mut rc = RECT {
        left: (dxWindow - DX_BUTTON) >> 1,
        top: DY_TOP_LED,
        right: 0,
        bottom: 0,
    };
    rc.right = rc.left + DX_BUTTON;
    rc.bottom = rc.top + DY_BUTTON;

    if PtInRect(&rc, msg.pt) == 0 {
        return FALSE;
    }

    SetCapture(hwndMain);
    DisplayButton(I_BUTTON_DOWN);
    MapWindowPoints(hwndMain, NULL_HWND, &mut rc as *mut RECT as *mut POINT, 2);

    let mut pressed = true;
    loop {
        if PeekMessageW(&mut msg, hwndMain, WM_MOUSEFIRST, WM_MOUSELAST, PM_REMOVE) != 0 {
            match msg.message {
                WM_LBUTTONUP => {
                    if pressed && PtInRect(&rc, msg.pt) != 0 {
                        iButtonCur = I_BUTTON_HAPPY;
                        DisplayButton(iButtonCur);
                        StartGame();
                    }
                    ReleaseCapture();
                    return TRUE;
                }
                WM_MOUSEMOVE => {
                    if PtInRect(&rc, msg.pt) != 0 {
                        if !pressed {
                            pressed = true;
                            DisplayButton(I_BUTTON_DOWN);
                        }
                    } else if pressed {
                        pressed = false;
                        DisplayButton(iButtonCur);
                    }
                }
                _ => {}
            }
        }
    }
}

pub unsafe extern "system" fn PrefDlgProc(
    h_dlg: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> isize {
    // Custom game dialog mirroring the legacy behavior and help wiring.
    match message {
        WM_INITDIALOG => {
            SetDlgItemInt(h_dlg, ID_EDIT_HEIGHT, Preferences.Height as u32, FALSE);
            SetDlgItemInt(h_dlg, ID_EDIT_WIDTH, Preferences.Width as u32, FALSE);
            SetDlgItemInt(h_dlg, ID_EDIT_MINES, Preferences.Mines as u32, FALSE);
            return TRUE as isize;
        }
        WM_COMMAND => {
            match command_id(w_param) {
                ID_BTN_OK | IDOK_U16 => {
                    Preferences.Height = GetDlgInt(h_dlg, ID_EDIT_HEIGHT, MINHEIGHT, 24);
                    Preferences.Width = GetDlgInt(h_dlg, ID_EDIT_WIDTH, MINWIDTH, 30);
                    let max_mines = min(999, (Preferences.Height - 1) * (Preferences.Width - 1));
                    Preferences.Mines = GetDlgInt(h_dlg, ID_EDIT_MINES, 10, max_mines);
                }
                ID_BTN_CANCEL | IDCANCEL_U16 => {}
                _ => return FALSE as isize,
            }
            EndDialog(h_dlg, TRUE as isize);
            return TRUE as isize;
        }
        WM_HELP => {
            if apply_help_from_info(l_param, &PREF_HELP_IDS) {
                return TRUE as isize;
            }
        }
        WM_CONTEXTMENU => {
            apply_help_to_hwnd(w_param as HWND, &PREF_HELP_IDS);
            return TRUE as isize;
        }
        _ => {}
    }
    FALSE as isize
}

pub unsafe extern "system" fn BestDlgProc(
    h_dlg: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> isize {
    // High-score dialog with reset + context help support.
    match message {
        WM_INITDIALOG => {
            reset_best_dialog(h_dlg);
            return TRUE as isize;
        }
        WM_COMMAND => match command_id(w_param) {
            ID_BTN_RESET => {
                Preferences.rgTime[WGAME_BEGIN as usize] = 999;
                Preferences.rgTime[WGAME_INTER as usize] = 999;
                Preferences.rgTime[WGAME_EXPERT as usize] = 999;
                copy_from_default(name_ptr_for_game_mut(WGAME_BEGIN));
                copy_from_default(name_ptr_for_game_mut(WGAME_INTER));
                copy_from_default(name_ptr_for_game_mut(WGAME_EXPERT));
                fUpdateIni = TRUE;
                reset_best_dialog(h_dlg);
                return TRUE as isize;
            }
            ID_BTN_OK | IDOK_U16 | ID_BTN_CANCEL | IDCANCEL_U16 => {
                EndDialog(h_dlg, TRUE as isize);
                return TRUE as isize;
            }
            _ => {}
        },
        WM_HELP => {
            if apply_help_from_info(l_param, &BEST_HELP_IDS) {
                return TRUE as isize;
            }
        }
        WM_CONTEXTMENU => {
            apply_help_to_hwnd(w_param as HWND, &BEST_HELP_IDS);
            return TRUE as isize;
        }
        _ => {}
    }
    FALSE as isize
}

pub unsafe extern "system" fn EnterDlgProc(
    h_dlg: HWND,
    message: u32,
    w_param: WPARAM,
    _l_param: LPARAM,
) -> isize {
    // Name entry dialog shown when a player beats a high score.
    match message {
        WM_INITDIALOG => {
            let mut buffer = [0u16; CCH_MSG_MAX];
            let string_id = Preferences.wGameType + ID_MSG_BEGIN;
            LoadSz(string_id, buffer.as_mut_ptr(), buffer.len() as u32);
            SetDlgItemTextW(h_dlg, ID_TEXT_BEST, buffer.as_ptr());
            let edit_hwnd = GetDlgItem(h_dlg, ID_EDIT_NAME);
            if edit_hwnd != NULL_HWND {
                SendMessageW(edit_hwnd, EM_SETLIMITTEXT, CCH_NAME_MAX as WPARAM, 0);
            }
            SetDlgItemTextW(h_dlg, ID_EDIT_NAME, current_name_ptr());
            return TRUE as isize;
        }
        WM_COMMAND => match command_id(w_param) {
            ID_BTN_OK | IDOK_U16 | ID_BTN_CANCEL | IDCANCEL_U16 => {
                GetDlgItemTextW(
                    h_dlg,
                    ID_EDIT_NAME,
                    current_name_ptr_mut(),
                    CCH_NAME_MAX as c_int,
                );
                EndDialog(h_dlg, TRUE as isize);
                return TRUE as isize;
            }
            _ => {}
        },
        _ => {}
    }
    FALSE as isize
}

pub unsafe fn AdjustWindow(mut f_adjust: c_int) {
    // Recompute the main window rectangle whenever the board or menu state changes.
    if hwndMain == NULL_HWND {
        return;
    }

    dxWindow = DX_BLK * xBoxMac + DX_GRID_OFF + DX_RIGHT_SPACE;
    dyWindow = DY_BLK * yBoxMac + DY_GRID_OFF + DY_BOTTOM_SPACE;

    let menu_visible = menu_is_visible();
    let mut rect_game = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut rect_help = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut menu_extra = 0;
    let mut diff_level = false;
    if menu_visible
        && GetMenuItemRect(hwndMain, hMenu, 0, &mut rect_game) != 0
        && GetMenuItemRect(hwndMain, hMenu, 1, &mut rect_help) != 0
        && rect_game.top != rect_help.top
    {
        diff_level = true;
        menu_extra = dypMenu;
    }

    let mut desired = RECT {
        left: 0,
        top: 0,
        right: dxWindow,
        bottom: dyWindow,
    };
    let dw_style = GetWindowLongPtrW(hwndMain, GWL_STYLE) as u32;
    let dw_ex_style = GetWindowLongPtrW(hwndMain, GWL_EXSTYLE) as u32;
    let mut frame_extra = dxpBorder;
    if AdjustWindowRectEx(
        &mut desired,
        dw_style,
        bool_to_bool(menu_visible) as BOOL,
        dw_ex_style,
    ) != 0
    {
        let cx_total = desired.right - desired.left;
        let cy_total = desired.bottom - desired.top;
        frame_extra = max(0, cx_total - dxWindow);
        dypAdjust = max(0, cy_total - dyWindow);
    } else {
        dypAdjust = dypCaption;
        if menu_visible {
            dypAdjust += dypMenu;
        }
    }

    dypAdjust += menu_extra;
    dxFrameExtra = frame_extra;

    let mut excess =
        Preferences.xWindow + dxWindow + dxFrameExtra - our_get_system_metrics(SM_CXSCREEN);
    if excess > 0 {
        f_adjust |= F_RESIZE;
        Preferences.xWindow -= excess;
    }
    excess = Preferences.yWindow + dyWindow + dypAdjust - our_get_system_metrics(SM_CYSCREEN);
    if excess > 0 {
        f_adjust |= F_RESIZE;
        Preferences.yWindow -= excess;
    }

    if bInitMinimized == FALSE {
        if (f_adjust & F_RESIZE) != 0 {
            MoveWindow(
                hwndMain,
                Preferences.xWindow,
                Preferences.yWindow,
                dxWindow + dxFrameExtra,
                dyWindow + dypAdjust,
                TRUE,
            );
        }

        if diff_level
            && menu_visible
            && GetMenuItemRect(hwndMain, hMenu, 0, &mut rect_game) != 0
            && GetMenuItemRect(hwndMain, hMenu, 1, &mut rect_help) != 0
            && rect_game.top == rect_help.top
        {
            dypAdjust -= dypMenu;
            MoveWindow(
                hwndMain,
                Preferences.xWindow,
                Preferences.yWindow,
                dxWindow + dxFrameExtra,
                dyWindow + dypAdjust,
                TRUE,
            );
        }

        if (f_adjust & F_DISPLAY) != 0 {
            let rect = RECT {
                left: 0,
                top: 0,
                right: dxWindow,
                bottom: dyWindow,
            };
            InvalidateRect(hwndMain, &rect, TRUE);
        }
    }
}

fn our_get_system_metrics(index: c_int) -> c_int {
    // Favor the virtual screen metrics when available to support multi-monitor setups.
    unsafe {
        match index {
            SM_CXSCREEN => {
                let mut result = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                if result == 0 {
                    result = GetSystemMetrics(SM_CXSCREEN);
                }
                result
            }
            SM_CYSCREEN => {
                let mut result = GetSystemMetrics(SM_CYVIRTUALSCREEN);
                if result == 0 {
                    result = GetSystemMetrics(SM_CYSCREEN);
                }
                result
            }
            _ => GetSystemMetrics(index),
        }
    }
}

fn loword(value: LPARAM) -> c_int {
    ((value as u32) & 0xFFFF) as i16 as c_int
}

fn hiword(value: LPARAM) -> c_int {
    (((value as u32) >> 16) & 0xFFFF) as i16 as c_int
}

fn command_id(w_param: WPARAM) -> u16 {
    (w_param & 0xFFFF) as u16
}

unsafe fn set_dtext(h_dlg: HWND, id: c_int, time: c_int, name: *const u16) {
    let mut buffer = [0u16; CCH_NAME_MAX];
    wsprintfW(buffer.as_mut_ptr(), addr_of!(szTime) as *const u16, time);
    SetDlgItemTextW(h_dlg, id, buffer.as_ptr());
    SetDlgItemTextW(h_dlg, id + 1, name);
}

unsafe fn reset_best_dialog(h_dlg: HWND) {
    set_dtext(
        h_dlg,
        ID_TIME_BEGIN,
        Preferences.rgTime[WGAME_BEGIN as usize],
        name_ptr_for_game(WGAME_BEGIN),
    );
    set_dtext(
        h_dlg,
        ID_TIME_INTER,
        Preferences.rgTime[WGAME_INTER as usize],
        name_ptr_for_game(WGAME_INTER),
    );
    set_dtext(
        h_dlg,
        ID_TIME_EXPERT,
        Preferences.rgTime[WGAME_EXPERT as usize],
        name_ptr_for_game(WGAME_EXPERT),
    );
}

unsafe fn current_name_ptr() -> *const u16 {
    name_ptr_for_game(Preferences.wGameType as c_int)
}

unsafe fn current_name_ptr_mut() -> *mut u16 {
    name_ptr_for_game_mut(Preferences.wGameType as c_int)
}

unsafe fn name_ptr_for_game(game_type: c_int) -> *const u16 {
    match game_type {
        WGAME_BEGIN => addr_of!(Preferences.szBegin) as *const u16,
        WGAME_INTER => addr_of!(Preferences.szInter) as *const u16,
        _ => addr_of!(Preferences.szExpert) as *const u16,
    }
}

unsafe fn name_ptr_for_game_mut(game_type: c_int) -> *mut u16 {
    match game_type {
        WGAME_BEGIN => addr_of_mut!(Preferences.szBegin) as *mut u16,
        WGAME_INTER => addr_of_mut!(Preferences.szInter) as *mut u16,
        _ => addr_of_mut!(Preferences.szExpert) as *mut u16,
    }
}

unsafe fn copy_from_default(dst: *mut u16) {
    let mut i = 0;
    while i < CCH_NAME_MAX {
        let ch = szDefaultName[i];
        *dst.add(i) = ch;
        if ch == 0 {
            return;
        }
        i += 1;
    }
    *dst.add(CCH_NAME_MAX - 1) = 0;
}

unsafe fn apply_help_from_info(l_param: LPARAM, ids: &[u32]) -> bool {
    if l_param == 0 {
        return false;
    }
    let info = &*(l_param as *const HelpInfo);
    WinHelpW(
        info.hItemHandle,
        HELP_FILE,
        HELP_WM_HELP,
        ids.as_ptr() as usize,
    );
    true
}

unsafe fn apply_help_to_hwnd(hwnd: HWND, ids: &[u32]) {
    WinHelpW(hwnd, HELP_FILE, HELP_CONTEXTMENU, ids.as_ptr() as usize);
}

fn menu_is_visible() -> bool {
    unsafe { (Preferences.fMenu & FMENU_FLAG_OFF) == 0 && hMenu != NULL_HMENU }
}
