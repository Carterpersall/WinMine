use core::cmp::{max, min};
use core::mem;
use core::ptr::{addr_of, addr_of_mut, null_mut};
use core::sync::atomic::{AtomicI32, Ordering};

use windows_sys::Win32::Data::HtmlHelp::{HH_DISPLAY_INDEX, HH_DISPLAY_TOPIC};
use windows_sys::Win32::Foundation::{
    FALSE, HANDLE, HINSTANCE as RawHINSTANCE, HWND as RawHWND, LPARAM, LRESULT, TRUE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{PtInRect, SetPixel};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetDlgItem, GetDlgItemTextW, RegisterClassW, SendMessageW,
    SetDlgItemInt, SetDlgItemTextW, WNDCLASSW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{HMENU as RawHMENU};
use windows_sys::core::PSTR;

use winsafe::co::{self, DLGID, GWLP, HELPW, ICC, IDC, MK, SM, STOCK_BRUSH, WA, WS, WS_EX};
use winsafe::msg::WndMsg;
use winsafe::prelude::Handle;
use winsafe::{
    AdjustWindowRectEx as ws_AdjustWindowRectEx, COLORREF, DLGPROC, DispatchMessage, GetMessage,
    GetSystemMetrics as win_get_system_metrics, INITCOMMONCONTROLSEX, IdIdcStr, IdIdiStr, IdStr,
    InitCommonControlsEx, MSG, POINT, PtsRc, RECT, SIZE, WINDOWPOS, PeekMessage,
    PostQuitMessage, TranslateMessage,
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
    CCH_NAME_MAX, FMENU_ALWAYS_ON, FMENU_ON, FSOUND_OFF, FSOUND_ON, MINHEIGHT, MINWIDTH,
    ReadPreferences, WGAME_BEGIN, WGAME_EXPERT, WGAME_INTER, WritePreferences, fUpdateIni,
};
use crate::rtns::rgBlk;
use crate::rtns::{
    DoButton1Up, DoTimer, MakeGuess, PauseGame, Preferences, ResumeGame, StartGame, TrackMouse,
    iButtonCur, xBoxMac, xCur, yBoxMac, yCur,
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
const ID_EDIT_HEIGHT: i32 = 141;
const ID_EDIT_WIDTH: i32 = 142;
const ID_EDIT_MINES: i32 = 143;
const ID_BTN_OK: u16 = 100;
const ID_BTN_CANCEL: u16 = 109;
const ID_BTN_RESET: u16 = 707;
const ID_TEXT_BEST: i32 = 601;
const ID_EDIT_NAME: i32 = 602;
const ID_TIME_BEGIN: i32 = 701;
const ID_NAME_BEGIN: i32 = 702;
const ID_TIME_INTER: i32 = 703;
const ID_NAME_INTER: i32 = 704;
const ID_TIME_EXPERT: i32 = 705;
const ID_NAME_EXPERT: i32 = 706;
const ID_STEXT1: i32 = 708;
const ID_STEXT2: i32 = 709;
const ID_STEXT3: i32 = 710;
const ID_TXT_MINES: i32 = 111;
const ID_TXT_HEIGHT: i32 = 112;
const ID_TXT_WIDTH: i32 = 113;
const IDH_PREF_EDIT_HEIGHT: u32 = 1000;
const IDH_PREF_EDIT_WIDTH: u32 = 1001;
const IDH_PREF_EDIT_MINES: u32 = 1002;
const IDH_BEST_BTN_RESET: u32 = 1003;
const IDH_STEXT: u32 = 1004;
const ID_MSG_BEGIN: u16 = 9;
const CCH_MSG_MAX: usize = 128;

const DX_BLK: i32 = 16;
const DY_BLK: i32 = 16;
const DX_LEFT_SPACE: i32 = 12;
const DX_RIGHT_SPACE: i32 = 12;
const DY_TOP_SPACE: i32 = 12;
const DY_BOTTOM_SPACE: i32 = 12;
const DX_GRID_OFF: i32 = DX_LEFT_SPACE;
const DY_LED: i32 = 23;
const DY_TOP_LED: i32 = DY_TOP_SPACE + 4;
const DY_GRID_OFF: i32 = DY_TOP_LED + DY_LED + 16;
const DX_BUTTON: i32 = 24;
const DY_BUTTON: i32 = 24;

const I_BUTTON_HAPPY: i32 = 0;
const I_BUTTON_CAUTION: i32 = 1;
const I_BUTTON_DOWN: i32 = 4;

const FMENU_FLAG_OFF: i32 = 0x01;
const F_CALC: i32 = 0x01;
const F_RESIZE: i32 = 0x02;
const F_DISPLAY: i32 = 0x04;

const WINDOW_STYLE: u32 = co::WS::OVERLAPPED.raw()
    | co::WS::MINIMIZEBOX.raw()
    | co::WS::CAPTION.raw()
    | co::WS::SYSMENU.raw();

const LEVEL_DATA: [[i32; 3]; 3] = [[10, MINHEIGHT, MINWIDTH], [40, 16, 16], [99, 16, 30]];

const F_PLAY: i32 = 0x01;
const F_PAUSE: i32 = 0x02;
const F_ICON: i32 = 0x08;

const FMENU_OFF: i32 = 1;
const SC_MASK: WPARAM = 0xFFF0;
const WA_CLICKACTIVE: u16 = WA::CLICKACTIVE.raw();

const MK_LBUTTON: WPARAM = MK::LBUTTON.raw() as WPARAM;
const MK_RBUTTON: WPARAM = MK::RBUTTON.raw() as WPARAM;
const MK_SHIFT_FLAG: WPARAM = MK::SHIFT.raw() as WPARAM;
const MK_CHORD_MASK: WPARAM = MK_SHIFT_FLAG | MK_RBUTTON;
const MK_CONTROL_FLAG: WPARAM = MK::CONTROL.raw() as WPARAM;

const VK_F4_CODE: u32 = co::VK::F4.raw() as u32;
const VK_F5_CODE: u32 = co::VK::F5.raw() as u32;
const VK_F6_CODE: u32 = co::VK::F6.raw() as u32;
const VK_SHIFT_CODE: u32 = co::VK::SHIFT.raw() as u32;

const C_BLK_MAX: usize = 27 * 32;
const BOARD_INDEX_SHIFT: usize = 5;
const MASK_BOMB: u8 = 0x80;
const COLOR_BLACK: COLORREF = COLORREF::from_rgb(0, 0, 0);
const COLOR_WHITE: COLORREF = COLORREF::from_rgb(0xFF, 0xFF, 0xFF);

const HELP_FILE: &str = "winmine.hlp";

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
const IDOK_U16: u16 = DLGID::OK.raw();
const IDCANCEL_U16: u16 = DLGID::CANCEL.raw();
const NULL_HWND_RAW: RawHWND = 0 as RawHWND;

#[repr(C)]
struct HelpInfo {
    cbSize: u32,
    iContextType: i32,
    iCtrlId: i32,
    hItemHandle: HANDLE,
    dwContextId: usize,
    mouse_pos: POINT,
}

type DialogProc = DLGPROC;

fn show_dialog(template_id: u16, proc: DialogProc) {
    let hinst_wrap = unsafe { winsafe::HINSTANCE::from_ptr(hInst.ptr()) };
    let parent_hwnd = unsafe { hwndMain.as_opt() };
    unsafe {
        let _ = hinst_wrap.DialogBoxParam(IdStr::Id(template_id), parent_hwnd, proc, Some(0));
    }
}

fn class_name_ptr() -> *const u16 {
    unsafe { addr_of!(szClass[0]) }
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
    unsafe {
        let mut wc: WNDCLASSW = mem::zeroed();
        wc.lpfnWndProc = Some(MainWndProc);
        wc.hInstance = hInst.ptr() as RawHINSTANCE;
        wc.hIcon = hIconMain.ptr() as _;
        wc.hCursor = winsafe::HINSTANCE::NULL
            .LoadCursor(IdIdcStr::Idc(IDC::ARROW))
            .map(|mut cursor| cursor.leak().ptr() as _)
            .unwrap_or(null_mut());
        wc.hbrBackground = winsafe::HBRUSH::GetStockObject(STOCK_BRUSH::LTGRAY)
            .map(|brush| brush.ptr() as _)
            .unwrap_or(null_mut());
        wc.lpszMenuName = core::ptr::null();
        wc.lpszClassName = class_name_ptr();
        RegisterClassW(&wc) != 0
    }
}

pub fn run_winmine(
    h_instance: RawHINSTANCE,
    _h_prev_instance: RawHINSTANCE,
    _lp_cmd_line: PSTR,
    n_cmd_show: i32,
) -> i32 {
    unsafe {
        hInst = winsafe::HINSTANCE::from_ptr(h_instance as _);
        InitConst();

        bInitMinimized.store(initial_minimized_state(n_cmd_show), Ordering::Relaxed);

        init_common_controls();
        let hinst_wrap = winsafe::HINSTANCE::from_ptr(hInst.ptr());
        hIconMain = hinst_wrap
            .LoadIcon(IdIdiStr::Id(ID_ICON_MAIN))
            .map(|mut icon| icon.leak())
            .unwrap_or(winsafe::HICON::NULL);

        if !register_main_window_class() {
            return FALSE;
        }

        hMenu = hinst_wrap
            .LoadMenu(IdStr::Id(ID_MENU))
            .map(|mut menu| menu.leak())
            .unwrap_or(winsafe::HMENU::NULL);
        let h_accel = hinst_wrap
            .LoadAccelerators(IdStr::Id(ID_MENU_ACCEL))
            .map(|mut accel| accel.leak())
            .unwrap_or(winsafe::HACCEL::NULL);

        ReadPreferences();

        let dx_window = dxWindow.load(Ordering::Relaxed);
        let dy_window = dyWindow.load(Ordering::Relaxed);
        let dxp_border = dxpBorder.load(Ordering::Relaxed);
        let dyp_adjust = dypAdjust.load(Ordering::Relaxed);

        let raw_created_hwnd = CreateWindowExW(
            0,
            class_name_ptr(),
            class_name_ptr(),
            WINDOW_STYLE,
            Preferences.xWindow - dxp_border,
            Preferences.yWindow - dyp_adjust,
            dx_window + dxp_border,
            dy_window + dyp_adjust,
            NULL_HWND_RAW,
            hMenu.ptr() as RawHMENU,
            hInst.ptr() as RawHINSTANCE,
            null_mut(),
        );
        hwndMain = winsafe::HWND::from_ptr(raw_created_hwnd as _);

        if hwndMain.as_opt().is_none() {
            ReportErr(1000);
            return FALSE;
        }

        AdjustWindow(F_CALC);

        if !FInitLocal() {
            ReportErr(ID_ERR_MEM);
            return FALSE;
        }

        SetMenuBar(Preferences.fMenu);
        StartGame();

        if let Some(hwnd_wrap) = hwndMain.as_opt() {
            hwnd_wrap.ShowWindow(co::SW::SHOWNORMAL);
            let _ = hwnd_wrap.UpdateWindow();
        }

        bInitMinimized.store(false, Ordering::Relaxed);

        let mut msg = MSG::default();
        loop {
            let has_msg = match GetMessage(&mut msg, None, 0, 0) {
                Ok(flag) => flag,
                Err(_) => break,
            };
            if !has_msg {
                break;
            }

            let handled = h_accel
                .as_opt()
                .and_then(|accel| {
                    hwndMain
                        .as_opt()
                        .and_then(|hwnd| hwnd.TranslateAccelerator(accel, &mut msg).ok())
                })
                .is_some();

            if !handled {
                TranslateMessage(&msg);
                let _ = DispatchMessage(&msg);
            }
        }

        CleanUp();

        if fUpdateIni.load(Ordering::Relaxed) {
            WritePreferences();
        }

        msg.wParam as i32
    }
}

fn x_box_from_xpos(x: i32) -> i32 {
    (x - (DX_GRID_OFF - DX_BLK)) >> 4
}

fn y_box_from_ypos(y: i32) -> i32 {
    (y - (DY_GRID_OFF - DY_BLK)) >> 4
}

fn status_icon() -> bool {
    fStatus.load(Ordering::Relaxed) & F_ICON != 0
}

fn status_play() -> bool {
    fStatus.load(Ordering::Relaxed) & F_PLAY != 0
}

fn set_status_pause() {
    fStatus.fetch_or(F_PAUSE, Ordering::Relaxed);
}

fn clr_status_pause() {
    fStatus.fetch_and(!F_PAUSE, Ordering::Relaxed);
}

fn set_status_icon() {
    fStatus.fetch_or(F_ICON, Ordering::Relaxed);
}

fn clr_status_icon() {
    fStatus.fetch_and(!F_ICON, Ordering::Relaxed);
}

fn set_block_flag(active: bool) {
    fBlock.store(active, Ordering::Relaxed);
}

fn begin_primary_button_drag(h_wnd: RawHWND) {
    unsafe {
        SetCapture(h_wnd);
    }
    fButton1Down.store(true, Ordering::Relaxed);
    xCur.store(-1, Ordering::Relaxed);
    yCur.store(-1, Ordering::Relaxed);
    DisplayButton(I_BUTTON_CAUTION);
}

fn finish_primary_button_drag() {
    fButton1Down.store(false, Ordering::Relaxed);
    unsafe {
        ReleaseCapture();
    }
    if status_play() {
        DoButton1Up();
    } else {
        TrackMouse(-2, -2);
    }
}

fn handle_mouse_move(w_param: WPARAM, l_param: LPARAM) {
    if fButton1Down.load(Ordering::Relaxed) {
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

fn handle_rbutton_down(h_wnd: RawHWND, w_param: WPARAM, l_param: LPARAM) -> Option<LRESULT> {
    unsafe {
        if handle_ignore_click() {
            return Some(0);
        }

        if !status_play() {
            return None;
        }

        if fButton1Down.load(Ordering::Relaxed) {
            TrackMouse(-3, -3);
            set_block_flag(true);
            let _ = hwndMain.PostMessage(WndMsg::new(co::WM::MOUSEMOVE, w_param, l_param));
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
}

fn handle_command(w_param: WPARAM, _l_param: LPARAM) -> Option<LRESULT> {
    unsafe {
        match command_id(w_param) {
            IDM_NEW => StartGame(),
            IDM_EXIT => {
                hwndMain.ShowWindow(co::SW::HIDE);
                SendMessageW(
                    hwndMain.ptr() as RawHWND,
                    co::WM::SYSCOMMAND.raw(),
                    co::SC::CLOSE.raw() as WPARAM,
                    0,
                );
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
                if sound_on() {
                    EndTunes();
                    Preferences.fSound = FSOUND_OFF;
                } else {
                    Preferences.fSound = FInitTunes();
                }
                update_menu_from_preferences();
            }
            IDM_COLOR => {
                Preferences.fColor = toggle_bool(Preferences.fColor);
                FreeBitmaps();
                if !FLoadBitmaps() {
                    ReportErr(ID_ERR_MEM);
                    SendMessageW(
                        hwndMain.ptr() as RawHWND,
                        co::WM::SYSCOMMAND.raw(),
                        co::SC::CLOSE.raw() as WPARAM,
                        0,
                    );
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
            IDM_HELP => DoHelp(HELPW::INDEX.raw() as u16, HH_DISPLAY_TOPIC as u32),
            IDM_HOW2PLAY => DoHelp(HELPW::CONTEXT.raw() as u16, HH_DISPLAY_INDEX as u32),
            IDM_HELP_HELP => DoHelp(HELPW::HELPONHELP.raw() as u16, HH_DISPLAY_TOPIC as u32),
            IDM_HELP_ABOUT => {
                DoAbout();
                return Some(0);
            }
            _ => {}
        }
    }

    None
}

fn handle_keydown(w_param: WPARAM) {
    unsafe {
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
}

fn handle_window_pos_changed(l_param: LPARAM) {
    unsafe {
        if status_icon() || l_param == 0 {
            return;
        }

        let pos = &*(l_param as *const WINDOWPOS);
        Preferences.xWindow = pos.x;
        Preferences.yWindow = pos.y;
    }
}

fn handle_syscommand(w_param: WPARAM) {
    let command = (w_param & SC_MASK) as u32;
    if command == co::SC::MINIMIZE.raw() {
        PauseGame();
        set_status_pause();
        set_status_icon();
    } else if command == co::SC::RESTORE.raw() {
        clr_status_pause();
        clr_status_icon();
        ResumeGame();
        fIgnoreClick.store(false, Ordering::Relaxed);
    }
}

fn handle_ignore_click() -> bool {
    fIgnoreClick.swap(false, Ordering::Relaxed)
}

fn local_pause() -> bool {
    fLocalPause.load(Ordering::Relaxed)
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

fn update_menu_from_preferences() {
    unsafe {
        fUpdateIni.store(true, Ordering::Relaxed);
        SetMenuBar(Preferences.fMenu);
    }
}

fn toggle_bool(value: bool) -> bool {
    !value
}

fn get_activate_state(w_param: WPARAM) -> u16 {
    (w_param & 0xFFFF) as u16
}

fn in_range(x: i32, y: i32) -> bool {
    let x_max = xBoxMac.load(Ordering::Relaxed);
    let y_max = yBoxMac.load(Ordering::Relaxed);
    x > 0 && y > 0 && x <= x_max && y <= y_max
}

fn board_index(x: i32, y: i32) -> usize {
    let offset = ((y as isize) << BOARD_INDEX_SHIFT) + x as isize;
    offset.max(0) as usize
}

fn cell_is_bomb(x: i32, y: i32) -> bool {
    unsafe {
        if !in_range(x, y) {
            return false;
        }
        let idx = board_index(x, y);
        if idx >= C_BLK_MAX {
            return false;
        }
        (rgBlk[idx] as u8 & MASK_BOMB) != 0
    }
}

const CCH_XYZZY: i32 = 5;
static I_XYZZY: AtomicI32 = AtomicI32::new(0);
const XYZZY_SEQUENCE: [u16; 5] = [
    b'X' as u16,
    b'Y' as u16,
    b'Z' as u16,
    b'Z' as u16,
    b'Y' as u16,
];

fn handle_xyzzys_shift() {
    if I_XYZZY.load(Ordering::Relaxed) >= CCH_XYZZY {
        I_XYZZY.fetch_xor(20, Ordering::Relaxed);
    }
}

fn handle_xyzzys_default_key(w_param: WPARAM) {
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

fn handle_xyzzys_mouse(w_param: WPARAM, l_param: LPARAM) {
    let state = I_XYZZY.load(Ordering::Relaxed);
    if state == 0 {
        return;
    }

    let control_down = (w_param & MK_CONTROL_FLAG) != 0;
    if (state == CCH_XYZZY && control_down) || state > CCH_XYZZY {
        let x_pos = x_box_from_xpos(loword(l_param));
        let y_pos = y_box_from_ypos(hiword(l_param));
        xCur.store(x_pos, Ordering::Relaxed);
        yCur.store(y_pos, Ordering::Relaxed);
        if in_range(x_pos, y_pos)
            && let Ok(hdc) = winsafe::HWND::NULL.GetDC()
        {
            let color = if cell_is_bomb(x_pos, y_pos) {
                COLOR_BLACK
            } else {
                COLOR_WHITE
            };
            unsafe {
                SetPixel(hdc.ptr(), 0, 0, color.raw());
            }
        }
    }
}

pub unsafe extern "system" fn MainWndProc(
    h_wnd: RawHWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    unsafe {
        let msg_code = co::WM::from_raw(message);
        match msg_code {
            co::WM::WINDOWPOSCHANGED => handle_window_pos_changed(l_param),
            co::WM::SYSCOMMAND => handle_syscommand(w_param),
            co::WM::COMMAND => {
                if let Some(result) = handle_command(w_param, l_param) {
                    return result;
                }
            }
            co::WM::KEYDOWN => handle_keydown(w_param),
            co::WM::DESTROY => {
                let _ = hwndMain.KillTimer(ID_TIMER);
                PostQuitMessage(0);
            }
            co::WM::MBUTTONDOWN => {
                if handle_ignore_click() {
                    return 0;
                }
                if status_play() {
                    set_block_flag(true);
                    begin_primary_button_drag(h_wnd);
                    handle_mouse_move(w_param, l_param);
                }
            }
            co::WM::LBUTTONDOWN => {
                if handle_ignore_click() {
                    return 0;
                }
                if FLocalButton(l_param) {
                    return 0;
                }
                if status_play() {
                    set_block_flag((w_param & MK_CHORD_MASK) != 0);
                    begin_primary_button_drag(h_wnd);
                    handle_mouse_move(w_param, l_param);
                }
            }
            co::WM::MOUSEMOVE => handle_mouse_move(w_param, l_param),
            co::WM::RBUTTONUP | co::WM::MBUTTONUP | co::WM::LBUTTONUP => {
                if fButton1Down.load(Ordering::Relaxed) {
                    finish_primary_button_drag();
                }
            }
            co::WM::RBUTTONDOWN => {
                if let Some(result) = handle_rbutton_down(h_wnd, w_param, l_param) {
                    return result;
                }
            }
            co::WM::ACTIVATE => {
                if get_activate_state(w_param) == WA_CLICKACTIVE {
                    fIgnoreClick.store(true, Ordering::Relaxed);
                }
            }
            co::WM::TIMER => {
                DoTimer();
                return 0;
            }
            co::WM::ENTERMENULOOP => fLocalPause.store(true, Ordering::Relaxed),
            co::WM::EXITMENULOOP => fLocalPause.store(false, Ordering::Relaxed),
            co::WM::PAINT => {
                let hwnd_wrap = winsafe::HWND::from_ptr(h_wnd as _);
                if let Ok(paint_guard) = hwnd_wrap.BeginPaint() {
                    DrawScreen(&paint_guard);
                }
                return 0;
            }
            _ => {}
        }

        DefWindowProcW(h_wnd, message, w_param, l_param)
    }
}

pub fn FixMenus() {
    unsafe {
        // Keep the menu checkmarks synchronized with the current difficulty/option flags.
        let game = Preferences.wGameType;
        CheckEm(IDM_BEGIN, game == WGAME_BEGIN as u16);
        CheckEm(IDM_INTER, game == WGAME_INTER as u16);
        CheckEm(IDM_EXPERT, game == WGAME_EXPERT as u16);
        CheckEm(IDM_CUSTOM, game == WGAME_OTHER);

        CheckEm(IDM_COLOR, Preferences.fColor);
        CheckEm(IDM_MARK, Preferences.fMark);
        CheckEm(IDM_SOUND, Preferences.fSound == FSOUND_ON);
    }
}

pub fn DoPref() {
    // Launch the custom game dialog, then treat the result as a "Custom" board.
    show_dialog(ID_DLG_PREF, PrefDlgProc);

    unsafe {
        Preferences.wGameType = WGAME_OTHER;
    }
    FixMenus();
    fUpdateIni.store(true, Ordering::Relaxed);
    StartGame();
}

pub fn DoEnterName() {
    // Show the high-score entry dialog and mark preferences dirty.
    show_dialog(ID_DLG_ENTER, EnterDlgProc);
    fUpdateIni.store(true, Ordering::Relaxed);
}

pub fn DoDisplayBest() {
    // Present the high-score list dialog as-is; no post-processing required here.
    show_dialog(ID_DLG_BEST, BestDlgProc);
}

pub fn FLocalButton(l_param: LPARAM) -> bool {
    unsafe {
        // Handle clicks on the smiley face button while providing the pressed animation.
        let mut msg = MSG::default();

        msg.pt.x = loword(l_param);
        msg.pt.y = hiword(l_param);

        let dx_window = dxWindow.load(Ordering::Relaxed);
        let mut rc = RECT {
            left: (dx_window - DX_BUTTON) >> 1,
            top: DY_TOP_LED,
            right: 0,
            bottom: 0,
        };
        rc.right = rc.left + DX_BUTTON;
        rc.bottom = rc.top + DY_BUTTON;

        let mut rc_sys = windows_sys::Win32::Foundation::RECT {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        };
        let mut pt_sys = windows_sys::Win32::Foundation::POINT {
            x: msg.pt.x,
            y: msg.pt.y,
        };
        if PtInRect(&rc_sys, pt_sys) == 0 {
            return false;
        }

        SetCapture(hwndMain.ptr() as RawHWND);
        DisplayButton(I_BUTTON_DOWN);
        if let Some(hwnd) = hwndMain.as_opt() {
            let _ = hwnd.MapWindowPoints(&winsafe::HWND::NULL, PtsRc::Rc(&mut rc));
        }

        let mut pressed = true;
        let hwnd_opt = hwndMain.as_opt();
        loop {
            if PeekMessage(
                &mut msg,
                hwnd_opt,
                co::WM::MOUSEFIRST.raw(),
                co::WM::MOUSELAST.raw(),
                co::PM::REMOVE,
            ) {
                rc_sys = windows_sys::Win32::Foundation::RECT {
                    left: rc.left,
                    top: rc.top,
                    right: rc.right,
                    bottom: rc.bottom,
                };
                pt_sys = windows_sys::Win32::Foundation::POINT {
                    x: msg.pt.x,
                    y: msg.pt.y,
                };
                match msg.message {
                    co::WM::LBUTTONUP => {
                        if pressed && PtInRect(&rc_sys, pt_sys) != 0 {
                            iButtonCur.store(I_BUTTON_HAPPY, Ordering::Relaxed);
                            DisplayButton(I_BUTTON_HAPPY);
                            StartGame();
                        }
                        ReleaseCapture();
                        return true;
                    }
                    co::WM::MOUSEMOVE => {
                        if PtInRect(&rc_sys, pt_sys) != 0 {
                            if !pressed {
                                pressed = true;
                                DisplayButton(I_BUTTON_DOWN);
                            }
                        } else if pressed {
                            pressed = false;
                            DisplayButton(iButtonCur.load(Ordering::Relaxed));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

pub extern "system" fn PrefDlgProc(
    h_dlg: winsafe::HWND,
    message: co::WM,
    w_param: usize,
    l_param: isize,
) -> isize {
    // Custom game dialog mirroring the legacy behavior and help wiring.
    let h_dlg_raw = h_dlg.ptr() as RawHWND;
    match message {
        co::WM::INITDIALOG => {
            unsafe {
                SetDlgItemInt(h_dlg_raw, ID_EDIT_HEIGHT, Preferences.Height as u32, FALSE);
                SetDlgItemInt(h_dlg_raw, ID_EDIT_WIDTH, Preferences.Width as u32, FALSE);
                SetDlgItemInt(h_dlg_raw, ID_EDIT_MINES, Preferences.Mines as u32, FALSE);
            }
            return TRUE as isize;
        }
        co::WM::COMMAND => {
            match command_id(w_param as WPARAM) {
                ID_BTN_OK | IDOK_U16 => unsafe {
                    Preferences.Height = GetDlgInt(&h_dlg, ID_EDIT_HEIGHT, MINHEIGHT, 24);
                    Preferences.Width = GetDlgInt(&h_dlg, ID_EDIT_WIDTH, MINWIDTH, 30);
                    let max_mines = min(999, (Preferences.Height - 1) * (Preferences.Width - 1));
                    Preferences.Mines = GetDlgInt(&h_dlg, ID_EDIT_MINES, 10, max_mines);
                },
                ID_BTN_CANCEL | IDCANCEL_U16 => {}
                _ => return FALSE as isize,
            }
            let _ = h_dlg.EndDialog(TRUE as isize);
            return TRUE as isize;
        }
        co::WM::HELP => {
            if apply_help_from_info(l_param as LPARAM, &PREF_HELP_IDS) {
                return TRUE as isize;
            }
        }
        co::WM::CONTEXTMENU => {
            apply_help_to_hwnd(w_param as RawHWND, &PREF_HELP_IDS);
            return TRUE as isize;
        }
        _ => {}
    }
    FALSE as isize
}

pub extern "system" fn BestDlgProc(
    h_dlg: winsafe::HWND,
    message: co::WM,
    w_param: usize,
    l_param: isize,
) -> isize {
    // High-score dialog with reset + context help support.
    let h_dlg_raw = h_dlg.ptr() as RawHWND;
    match message {
        co::WM::INITDIALOG => {
            reset_best_dialog(h_dlg_raw);
            return TRUE as isize;
        }
        co::WM::COMMAND => match command_id(w_param as WPARAM) {
            ID_BTN_RESET => unsafe {
                Preferences.rgTime[WGAME_BEGIN as usize] = 999;
                Preferences.rgTime[WGAME_INTER as usize] = 999;
                Preferences.rgTime[WGAME_EXPERT as usize] = 999;
                copy_from_default(name_ptr_for_game_mut(WGAME_BEGIN));
                copy_from_default(name_ptr_for_game_mut(WGAME_INTER));
                copy_from_default(name_ptr_for_game_mut(WGAME_EXPERT));
                fUpdateIni.store(true, Ordering::Relaxed);
                reset_best_dialog(h_dlg_raw);
                return TRUE as isize;
            },
            ID_BTN_OK | IDOK_U16 | ID_BTN_CANCEL | IDCANCEL_U16 => {
                let _ = h_dlg.EndDialog(TRUE as isize);
                return TRUE as isize;
            }
            _ => {}
        },
        co::WM::HELP => {
            if apply_help_from_info(l_param as LPARAM, &BEST_HELP_IDS) {
                return TRUE as isize;
            }
        }
        co::WM::CONTEXTMENU => {
            apply_help_to_hwnd(w_param as RawHWND, &BEST_HELP_IDS);
            return TRUE as isize;
        }
        _ => {}
    }
    FALSE as isize
}

pub extern "system" fn EnterDlgProc(
    h_dlg: winsafe::HWND,
    message: co::WM,
    w_param: usize,
    _l_param: isize,
) -> isize {
    // Name entry dialog shown when a player beats a high score.
    let h_dlg_raw = h_dlg.ptr() as RawHWND;
    match message {
        co::WM::INITDIALOG => {
            unsafe {
                let mut buffer = [0u16; CCH_MSG_MAX];
                let string_id = Preferences.wGameType + ID_MSG_BEGIN;
                LoadSz(string_id, buffer.as_mut_ptr(), buffer.len() as u32);
                SetDlgItemTextW(h_dlg_raw, ID_TEXT_BEST, buffer.as_ptr());
                let edit_hwnd = GetDlgItem(h_dlg_raw, ID_EDIT_NAME);
                if edit_hwnd != NULL_HWND_RAW {
                    SendMessageW(edit_hwnd, EM_SETLIMITTEXT, CCH_NAME_MAX as WPARAM, 0);
                }
                SetDlgItemTextW(h_dlg_raw, ID_EDIT_NAME, current_name_ptr());
            }
            return TRUE as isize;
        }
        co::WM::COMMAND => match command_id(w_param as WPARAM) {
            ID_BTN_OK | IDOK_U16 | ID_BTN_CANCEL | IDCANCEL_U16 => {
                unsafe {
                    GetDlgItemTextW(
                        h_dlg_raw,
                        ID_EDIT_NAME,
                        current_name_ptr_mut(),
                        CCH_NAME_MAX as i32,
                    );
                    let _ = h_dlg.EndDialog(TRUE as isize);
                }
                return TRUE as isize;
            }
            _ => {}
        },
        _ => {}
    }
    FALSE as isize
}

pub fn AdjustWindow(mut f_adjust: i32) {
    unsafe {
        // Recompute the main window rectangle whenever the board or menu state changes.
        if hwndMain.as_opt().is_none() {
            return;
        }

        let x_boxes = xBoxMac.load(Ordering::Relaxed);
        let y_boxes = yBoxMac.load(Ordering::Relaxed);
        let dx_window = DX_BLK * x_boxes + DX_GRID_OFF + DX_RIGHT_SPACE;
        let dy_window = DY_BLK * y_boxes + DY_GRID_OFF + DY_BOTTOM_SPACE;
        dxWindow.store(dx_window, Ordering::Relaxed);
        dyWindow.store(dy_window, Ordering::Relaxed);

        let menu_visible = menu_is_visible();
        let mut menu_extra = 0;
        let mut diff_level = false;
        if menu_visible {
            if let (Some(hwnd), Some(menu)) = (hwndMain.as_opt(), hMenu.as_opt()) {
                if let (Ok(game_rect), Ok(help_rect)) =
                    (hwnd.GetMenuItemRect(&menu, 0), hwnd.GetMenuItemRect(&menu, 1))
                {
                    if game_rect.top != help_rect.top {
                        diff_level = true;
                        menu_extra = dypMenu.load(Ordering::Relaxed);
                    }
                }
            }
        }

        let desired = RECT {
            left: 0,
            top: 0,
            right: dx_window,
            bottom: dy_window,
        };
        let hwnd_main = hwndMain.as_opt().unwrap();
        let dw_style = hwnd_main.GetWindowLongPtr(GWLP::STYLE) as u32;
        let dw_ex_style = hwnd_main.GetWindowLongPtr(GWLP::EXSTYLE) as u32;
        let mut frame_extra = dxpBorder.load(Ordering::Relaxed);
        let mut dyp_adjust;
        if let Ok(adjusted) = ws_AdjustWindowRectEx(
            desired,
            WS::from_raw(dw_style),
            menu_visible,
            WS_EX::from_raw(dw_ex_style),
        ) {
            let cx_total = adjusted.right - adjusted.left;
            let cy_total = adjusted.bottom - adjusted.top;
            frame_extra = max(0, cx_total - dx_window);
            dyp_adjust = max(0, cy_total - dy_window);
        } else {
            dyp_adjust = dypCaption.load(Ordering::Relaxed);
            if menu_visible {
                dyp_adjust += dypMenu.load(Ordering::Relaxed);
            }
        }

        dyp_adjust += menu_extra;
        dypAdjust.store(dyp_adjust, Ordering::Relaxed);
        dxFrameExtra.store(frame_extra, Ordering::Relaxed);

        let mut excess =
            Preferences.xWindow + dx_window + frame_extra - our_get_system_metrics(SM::CXSCREEN);
        if excess > 0 {
            f_adjust |= F_RESIZE;
            Preferences.xWindow -= excess;
        }
        excess =
            Preferences.yWindow + dy_window + dyp_adjust - our_get_system_metrics(SM::CYSCREEN);
        if excess > 0 {
            f_adjust |= F_RESIZE;
            Preferences.yWindow -= excess;
        }

        if !bInitMinimized.load(Ordering::Relaxed) {
            if (f_adjust & F_RESIZE) != 0 {
                let _ = hwnd_main.MoveWindow(
                    POINT {
                        x: Preferences.xWindow,
                        y: Preferences.yWindow,
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
                && hMenu.as_opt().is_some()
                && hMenu
                    .as_opt()
                    .and_then(|menu| {
                        hwnd_main
                            .GetMenuItemRect(&menu, 0)
                            .ok()
                            .zip(hwnd_main.GetMenuItemRect(&menu, 1).ok())
                    })
                    .is_some_and(|(g, h)| g.top == h.top)
            {
                dyp_adjust -= dypMenu.load(Ordering::Relaxed);
                dypAdjust.store(dyp_adjust, Ordering::Relaxed);
                let _ = hwnd_main.MoveWindow(
                    POINT {
                        x: Preferences.xWindow,
                        y: Preferences.yWindow,
                    },
                    SIZE {
                        cx: dx_window + frame_extra,
                        cy: dy_window + dyp_adjust,
                    },
                    true,
                );
            }

            if (f_adjust & F_DISPLAY) != 0 {
                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: dx_window,
                    bottom: dy_window,
                };
                let _ = hwnd_main.InvalidateRect(Some(&rect), true);
            }
        }
    }
}

fn our_get_system_metrics(index: SM) -> i32 {
    // Favor the virtual screen metrics when available to support multi-monitor setups.
    match index {
        SM::CXSCREEN => {
            let mut result = win_get_system_metrics(SM::CXVIRTUALSCREEN);
            if result == 0 {
                result = win_get_system_metrics(SM::CXSCREEN);
            }
            result
        }
        SM::CYSCREEN => {
            let mut result = win_get_system_metrics(SM::CYVIRTUALSCREEN);
            if result == 0 {
                result = win_get_system_metrics(SM::CYSCREEN);
            }
            result
        }
        _ => win_get_system_metrics(index),
    }
}

fn loword(value: LPARAM) -> i32 {
    ((value as u32) & 0xFFFF) as i16 as i32
}

fn hiword(value: LPARAM) -> i32 {
    (((value as u32) >> 16) & 0xFFFF) as i16 as i32
}

fn command_id(w_param: WPARAM) -> u16 {
    (w_param & 0xFFFF) as u16
}

fn set_dtext(h_dlg: RawHWND, id: i32, time: i32, name: *const u16) {
    unsafe {
        let mut buffer = [0u16; CCH_NAME_MAX];
        let fmt_len = szTime
            .iter()
            .position(|&ch| ch == 0)
            .unwrap_or(szTime.len());
        let fmt = String::from_utf16_lossy(&szTime[..fmt_len]);
        let text = fmt.replace("%d", &time.to_string());

        for (i, code_unit) in text
            .encode_utf16()
            .chain(Some(0))
            .take(buffer.len())
            .enumerate()
        {
            *buffer.as_mut_ptr().add(i) = code_unit;
        }

        SetDlgItemTextW(h_dlg, id, buffer.as_ptr());
        SetDlgItemTextW(h_dlg, id + 1, name);
    }
}

fn reset_best_dialog(h_dlg: RawHWND) {
    unsafe {
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
}

fn current_name_ptr() -> *const u16 {
    unsafe { name_ptr_for_game(Preferences.wGameType as i32) }
}

fn current_name_ptr_mut() -> *mut u16 {
    unsafe { name_ptr_for_game_mut(Preferences.wGameType as i32) }
}

fn name_ptr_for_game(game_type: i32) -> *const u16 {
    unsafe {
        match game_type {
            WGAME_BEGIN => addr_of!(Preferences.szBegin) as *const u16,
            WGAME_INTER => addr_of!(Preferences.szInter) as *const u16,
            _ => addr_of!(Preferences.szExpert) as *const u16,
        }
    }
}

fn name_ptr_for_game_mut(game_type: i32) -> *mut u16 {
    unsafe {
        match game_type {
            WGAME_BEGIN => addr_of_mut!(Preferences.szBegin) as *mut u16,
            WGAME_INTER => addr_of_mut!(Preferences.szInter) as *mut u16,
            _ => addr_of_mut!(Preferences.szExpert) as *mut u16,
        }
    }
}

fn copy_from_default(dst: *mut u16) {
    unsafe {
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
}

fn apply_help_from_info(l_param: LPARAM, ids: &[u32]) -> bool {
    unsafe {
        if l_param == 0 {
            return false;
        }
        let info = &*(l_param as *const HelpInfo);
        if info.hItemHandle.is_null() {
            return false;
        }
        let hwnd = winsafe::HWND::from_ptr(info.hItemHandle);
        let _ = hwnd.WinHelp(HELP_FILE, HELPW::WM_HELP, ids.as_ptr() as usize);
        true
    }
}

fn apply_help_to_hwnd(hwnd: RawHWND, ids: &[u32]) {
    unsafe {
        if hwnd.is_null() {
            return;
        }
        let hwnd = winsafe::HWND::from_ptr(hwnd);
        let _ = hwnd.WinHelp(HELP_FILE, HELPW::CONTEXTMENU, ids.as_ptr() as usize);
    }
}

fn menu_is_visible() -> bool {
    unsafe { (Preferences.fMenu & FMENU_FLAG_OFF) == 0 && hMenu.as_opt().is_some() }
}
