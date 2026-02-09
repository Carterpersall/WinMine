//! Main window and event handling for the Minesweeper game.

use core::cmp::{max, min};
use core::sync::atomic::{AtomicBool, Ordering};
use std::rc::Rc;
use std::sync::Arc;

use windows_sys::Win32::Data::HtmlHelp::{HH_DISPLAY_INDEX, HH_DISPLAY_TOC};

use winsafe::co::{BN, DLGID, HELPW, ICC, IDC, MK, PM, SC, SM, STOCK_BRUSH, SW, VK, WA, WM, WS};
use winsafe::msg::{WndMsg, em::SetLimitText, wm::Destroy};
use winsafe::{
    AdjustWindowRectExForDpi, AnyResult, GetSystemMetrics, HBRUSH, HINSTANCE, HWND,
    INITCOMMONCONTROLSEX, IdIdiStr, IdStr, InitCommonControlsEx, MSG, POINT, PeekMessage, PtsRc,
    RECT, SIZE, WINDOWPOS, gui, prelude::*,
};

use crate::globals::{BASE_DPI, DEFAULT_PLAYER_NAME, GAME_NAME, MSG_CREDIT, MSG_VERSION_NAME};
use crate::grafix::{
    ButtonSprite, DX_BLK_96, DX_BUTTON_96, DX_LEFT_SPACE_96, DX_RIGHT_SPACE_96, DY_BLK_96,
    DY_BOTTOM_SPACE_96, DY_BUTTON_96, DY_GRID_OFF_96, DY_TOP_LED_96,
};
use crate::help::Help;
use crate::pref::{CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH};
use crate::rtns::{AdjustFlag, GameState, ID_TIMER, StatusFlag};
use crate::sound::Sound;
use crate::util::{ResourceId, StateLock, get_dlg_int, init_const};

/// `WM_APP` request code posted to the main window when a new best time is
/// recorded.
///
/// The main UI thread handles this by showing the name-entry dialog, then the
/// best-times dialog.
pub const NEW_RECORD_DLG: usize = 1;

/// Struct containing the main window with its event handlers and the shared state.
#[derive(Clone)]
pub struct WinMineMainWindow {
    /// The main window, containing the HWND and event callbacks
    pub wnd: gui::WindowMain,
    /// Shared state for the game
    pub state: Rc<StateLock<GameState>>,
    /// Whether a drag operation is currently active
    drag_active: Arc<AtomicBool>,
    /// Signals that the next click should be ignored
    ///
    /// This is used after window activation to prevent accidental clicks.
    ignore_next_click: Arc<AtomicBool>,
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
            state: Rc::new(StateLock::new(GameState::new())),
            drag_active: Arc::new(AtomicBool::new(false)),
            ignore_next_click: Arc::new(AtomicBool::new(false)),
        };
        new_self.events();
        new_self
    }

    /// Runs the WinMine application.
    /// # Arguments
    /// * `h_instance`: The application instance handle.
    /// # Returns
    /// Ok(()) on success, or an error on failure.
    pub fn run(hinst: &HINSTANCE) -> Result<(), Box<dyn core::error::Error>> {
        // Seed the RNG, initialize global values, and ensure the preferences registry key exists
        init_const();

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
        let mut menu = hinst.LoadMenu(IdStr::Id(ResourceId::Menu as u16))?;

        // Get a handle to the accelerators resource
        let h_accel = hinst.LoadAccelerators(IdStr::Id(ResourceId::MenuAccel as u16))?;

        // Create the main application window
        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            class_name: GAME_NAME,
            title: GAME_NAME,
            class_icon: gui::Icon::Id(ResourceId::Icon as u16),
            class_cursor: gui::Cursor::Idc(IDC::ARROW),
            class_bg_brush: gui::Brush::Handle(HBRUSH::GetStockObject(STOCK_BRUSH::LTGRAY)?),
            style: WS::OVERLAPPED | WS::MINIMIZEBOX | WS::CAPTION | WS::SYSMENU,
            menu: menu.leak(),
            accel_table: Some(h_accel),
            ..Default::default()
        });

        // Create the main application state
        let app = WinMineMainWindow::new(wnd);

        // Read user preferences into the global state
        app.state.write().prefs.read_preferences()?;

        // Run the main application window, blocking until exit
        match app.wnd.run_main(None) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Unhandled error during main window execution: {e}").into()),
        }
    }

    /* Message Helper Functions */

    /// Begins a primary button drag operation.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn begin_primary_button_drag(&self) -> AnyResult<()> {
        self.drag_active.store(true, Ordering::Relaxed);
        self.state.write().cursor_x = usize::MAX - 1;
        self.state.write().cursor_y = usize::MAX - 1;
        self.state
            .read()
            .grafix
            .display_button(&self.wnd.hwnd().GetDC()?, ButtonSprite::Caution)
    }

    /// Finishes a primary button drag operation.
    /// # Returns
    /// An `Ok(())` if successful, or an error if drawing failed.
    fn finish_primary_button_drag(&self) -> AnyResult<()> {
        self.drag_active.store(false, Ordering::Relaxed);
        if self.state.read().game_status.contains(StatusFlag::Play) {
            self.state.write().do_button_1_up(self.wnd.hwnd())?;
        } else {
            self.state.write().track_mouse(
                &self.wnd.hwnd().GetDC()?,
                usize::MAX - 2,
                usize::MAX - 2,
            )?;
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
                let new_sound = match self.state.read().prefs.sound_enabled {
                    true => {
                        Sound::stop_all();
                        false
                    }
                    false => Sound::init(),
                };

                {
                    self.state.write().prefs.sound_enabled = new_sound;
                };

                self.set_menu_bar()?;
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
        if self.drag_active.load(Ordering::Relaxed) {
            // If the user is dragging, track the mouse position
            if self.state.read().game_status.contains(StatusFlag::Play) {
                let x_new = self.x_box_from_xpos(point.x);
                let y_new = self.y_box_from_ypos(point.y);
                self.state
                    .write()
                    .track_mouse(&self.wnd.hwnd().GetDC()?, x_new, y_new)?;
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
        if self.ignore_next_click.swap(false, Ordering::Relaxed)
            || !self.state.read().game_status.contains(StatusFlag::Play)
        {
            return Ok(());
        }

        // If the left and right buttons are both down, and the middle button is not down, start a chord operation
        if btn & (MK::LBUTTON | MK::RBUTTON | MK::MBUTTON) == MK::LBUTTON | MK::RBUTTON {
            self.state.write().chord_active = true;
            self.state.write().track_mouse(
                &self.wnd.hwnd().GetDC()?,
                usize::MAX - 3,
                usize::MAX - 3,
            )?;
            self.begin_primary_button_drag()?;
            self.handle_mouse_move(btn, point)?;
            return Ok(());
        }

        // Regular right-click: make a guess
        let x = self.x_box_from_xpos(point.x);
        let y = self.y_box_from_ypos(point.y);
        self.state.write().make_guess(self.wnd.hwnd(), x, y)?;
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
            self.ignore_next_click.store(false, Ordering::Relaxed);
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

        let mut state = self.state.write();
        state.prefs.wnd_x_pos = pos.x;
        state.prefs.wnd_y_pos = pos.y;
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

        let dx_window = self.state.read().grafix.wnd_pos.x;
        let dx_button = self.state.read().grafix.scale_dpi(DX_BUTTON_96);
        let dy_button = self.state.read().grafix.scale_dpi(DY_BUTTON_96);
        let dy_top_led = self.state.read().grafix.scale_dpi(DY_TOP_LED_96);
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

        let hdc = self.wnd.hwnd().GetDC()?;
        self.state
            .read()
            .grafix
            .display_button(&hdc, ButtonSprite::Down)?;
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
                            self.state
                                .read()
                                .grafix
                                .display_button(&hdc, ButtonSprite::Happy)?;
                            self.start_game()?;
                        }
                        return Ok(true);
                    }
                    WM::MOUSEMOVE => {
                        if winsafe::PtInRect(rc, msg.pt) {
                            if !pressed {
                                pressed = true;
                                self.state
                                    .read()
                                    .grafix
                                    .display_button(&hdc, ButtonSprite::Down)?;
                            }
                        } else if pressed {
                            pressed = false;
                            self.state
                                .read()
                                .grafix
                                .display_button(&hdc, self.state.read().btn_face_state)?;
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
        // Calculate desired window size based on board dimensions and DPI scaling
        let (dx_window, dy_window) = {
            let state = self.state.read();
            let dx_window = state.grafix.scale_dpi(DX_BLK_96) * state.board_width as i32
                + state.grafix.scale_dpi(DX_LEFT_SPACE_96)
                + state.grafix.scale_dpi(DX_RIGHT_SPACE_96);
            let dy_window = state.grafix.scale_dpi(DY_BLK_96) * state.board_height as i32
                + state.grafix.scale_dpi(DY_GRID_OFF_96)
                + state.grafix.scale_dpi(DY_BOTTOM_SPACE_96);
            (dx_window, dy_window)
        };
        self.state.write().grafix.wnd_pos.x = dx_window;
        self.state.write().grafix.wnd_pos.y = dy_window;

        // Get the current window position from preferences
        let (mut x_window, mut y_window) = {
            let state = self.state.read();
            (state.prefs.wnd_x_pos, state.prefs.wnd_y_pos)
        };

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
            true,
            self.wnd.hwnd().style_ex(),
            self.state.read().grafix.dpi,
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
        let mut state = self.state.write();
        state.prefs.wnd_x_pos = x_window;
        state.prefs.wnd_y_pos = y_window;

        Ok(())
    }

    /// Converts an x-coordinate in pixels to a box index.
    /// # Arguments
    /// * `x`: The x-coordinate in pixels.
    /// # Returns
    /// The corresponding box index.
    pub fn x_box_from_xpos(&self, x: i32) -> usize {
        let cell = self.state.read().grafix.scale_dpi(DX_BLK_96);
        if cell <= 0 {
            return 0;
        }
        ((x - (self.state.read().grafix.scale_dpi(DX_LEFT_SPACE_96) - cell)) / cell) as usize
    }

    /// Converts a y-coordinate in pixels to a box index.
    /// # Arguments
    /// * `y`: The y-coordinate in pixels.
    /// # Returns
    /// The corresponding box index.
    pub fn y_box_from_ypos(&self, y: i32) -> usize {
        let cell = self.state.read().grafix.scale_dpi(DY_BLK_96);
        if cell <= 0 {
            return 0;
        }
        ((y - (self.state.read().grafix.scale_dpi(DY_GRID_OFF_96) - cell)) / cell) as usize
    }

    /* Event Handlers */

    /// Hooks the window messages to their respective handlers.
    fn events(&self) {
        self.wnd.on().wm_create({
            let self2 = self.clone();
            move |_create| -> winsafe::AnyResult<i32> {
                // Sync global DPI state to the actual monitor DPI where the window was created.
                let dpi = self2.wnd.hwnd().GetDpiForWindow();
                self2.state.write().grafix.dpi = if dpi == 0 { BASE_DPI } else { dpi };

                // Initialize local resources.
                self2.state.write().init_game(self2.wnd.hwnd())?;

                // Update the menu bar and start a new game
                self2.set_menu_bar()?;
                self2.start_game()?;

                Ok(0)
            }
        });

        self.wnd.on().wm(WM::DPICHANGED, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                // wParam: new DPI in LOWORD/HIWORD (X/Y). lParam: suggested new window rect.
                let dpi = (msg.wparam) & 0xFFFF;
                if dpi > 0 {
                    self2.state.write().grafix.dpi = if dpi == 0 { BASE_DPI } else { dpi as u32 };
                }
                let suggested = unsafe { (msg.lparam as *const RECT).as_ref() };

                if let Some(rc) = suggested {
                    // Persist the suggested top-left so adjust_window keeps us on the same monitor
                    {
                        let mut state = self2.state.write();
                        state.prefs.wnd_x_pos = rc.left;
                        state.prefs.wnd_y_pos = rc.top;
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
                let color = self2.state.read().prefs.color;
                self2
                    .state
                    .write()
                    .grafix
                    .load_bitmaps(self2.wnd.hwnd(), color)?;

                self2.adjust_window(AdjustFlag::ResizeAndRedraw)?;
                Ok(0)
            }
        });

        self.wnd.on().wm_window_pos_changed({
            let self2 = self.clone();
            move |wnd_pos| {
                self2.handle_window_pos_changed(wnd_pos.windowpos);
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
                    EnterDialog::new(self2.state.clone()).show_modal(&self2.wnd)?;
                    BestDialog::new(self2.state.clone()).show_modal(&self2.wnd)?;
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
                self2.state.write().prefs.write_preferences()?;

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
                if self2.ignore_next_click.swap(false, Ordering::Relaxed) {
                    return Ok(());
                }

                if m_btn.vkey_code.has(MK::MBUTTON) {
                    // If the middle button is pressed, start a chord operation
                    // However, if a chord is already active, end the chord instead
                    let chord_active = self2.state.read().chord_active;
                    self2.state.write().chord_active = !chord_active;
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

        // TODO: Handle double clicks
        self.wnd.on().wm_l_button_down({
            let self2 = self.clone();
            move |l_btn| {
                if self2.ignore_next_click.swap(false, Ordering::Relaxed) {
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
                    self2.ignore_next_click.store(true, Ordering::Relaxed);
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
                let paint_guard = self2.wnd.hwnd().BeginPaint()?;
                self2
                    .state
                    .read()
                    .grafix
                    .draw_screen(&paint_guard, &self2.state.read())?;
                Ok(())
            }
        });

        /* Menu Commands */

        self.wnd.on().wm_command_acc_menu(ResourceId::NewGame, {
            let self2 = self.clone();
            move || {
                self2.start_game()?;
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Exit, {
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
            move |command: ResourceId| {
                let game = match command {
                    ResourceId::Begin => GameType::Begin,
                    ResourceId::Inter => GameType::Inter,
                    ResourceId::Expert => GameType::Expert,
                    _ => GameType::Other,
                };

                {
                    let mut state = self2.state.write();
                    if let Some(data) = game.preset_data() {
                        state.prefs.game_type = game;
                        state.prefs.mines = data.0;
                        state.prefs.height = data.1 as usize;
                        state.prefs.width = data.2 as usize;
                    }
                }
                self2.set_menu_bar()?;
                self2.start_game()?;
                Ok(())
            }
        };

        self.wnd.on().wm_command_acc_menu(ResourceId::Begin, {
            let difficulty_command = difficulty_command.clone();
            move || difficulty_command.clone()(ResourceId::Begin)
        });
        self.wnd.on().wm_command_acc_menu(ResourceId::Inter, {
            let difficulty_command = difficulty_command.clone();
            move || difficulty_command.clone()(ResourceId::Inter)
        });
        self.wnd.on().wm_command_acc_menu(ResourceId::Expert, {
            let difficulty_command = difficulty_command.clone();
            move || difficulty_command.clone()(ResourceId::Expert)
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Custom, {
            let self2 = self.clone();
            move || {
                // TODO: The way that the preferences dialog is handled causes a custom game to always
                // be started when the dialog is closed, even if the user clicked "Cancel". Fix

                // Show the preferences dialog
                PrefDialog::new(self2.state.clone()).show_modal(&self2.wnd)?;

                self2.state.write().prefs.game_type = GameType::Other;

                self2.set_menu_bar()?;
                self2.start_game()?;
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Sound, {
            let self2 = self.clone();
            move || {
                let new_sound = match self2.state.read().prefs.sound_enabled {
                    true => {
                        Sound::stop_all();
                        false
                    }
                    false => Sound::init(),
                };
                {
                    self2.state.write().prefs.sound_enabled = new_sound;
                };
                self2.set_menu_bar()?;
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Color, {
            let self2 = self.clone();
            move || {
                let color = !self2.state.read().prefs.color;
                self2.state.write().prefs.color = color;

                self2
                    .state
                    .write()
                    .grafix
                    .load_bitmaps(self2.wnd.hwnd(), color)?;

                // Repaint immediately so toggling color off updates without restarting.
                self2
                    .state
                    .read()
                    .grafix
                    .draw_screen(&*self2.wnd.hwnd().GetDC()?, &self2.state.read())?;
                self2.set_menu_bar()?;
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Mark, {
            let self2 = self.clone();
            move || {
                {
                    let marks_enabled = self2.state.read().prefs.mark_enabled;
                    self2.state.write().prefs.mark_enabled = !marks_enabled;
                };
                self2.set_menu_bar()?;
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Best, {
            let self2 = self.clone();
            move || BestDialog::new(self2.state.clone()).show_modal(&self2.wnd)
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::HelpSubmenu, {
            let self2 = self.clone();
            move || {
                Help::do_help(self2.wnd.hwnd(), HELPW::INDEX, HH_DISPLAY_TOC);
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::HowToPlay, {
            let self2 = self.clone();
            move || {
                Help::do_help(self2.wnd.hwnd(), HELPW::CONTEXT, HH_DISPLAY_INDEX);
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::HelpOnHelp, {
            let self2 = self.clone();
            move || {
                Help::do_help(self2.wnd.hwnd(), HELPW::HELPONHELP, HH_DISPLAY_TOC);
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::About, {
            let self2 = self.clone();
            move || {
                let icon = self2
                    .wnd
                    .hwnd()
                    .hinstance()
                    .LoadIcon(IdIdiStr::Id(ResourceId::Icon as u16))?;

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

/// Struct containing the state shared by the Preferences dialog
#[derive(Clone)]
struct PrefDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
    state: Rc<StateLock<GameState>>,
}

impl PrefDialog {
    /// Creates a new Preferences dialog instance and sets up event handlers.
    fn new(state: Rc<StateLock<GameState>>) -> Self {
        let dlg = gui::WindowModal::new_dlg(ResourceId::PrefDlg as u16);
        let new_self = Self { dlg, state };
        new_self.events();
        new_self
    }

    /// Displays the Preferences dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) -> AnyResult<()> {
        self.dlg.show_modal(parent)
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let self2 = self.clone();
            move |_| -> AnyResult<bool> {
                // Get current board settings from preferences
                let (height, width, mines) = {
                    let state = self2.state.read();
                    (state.prefs.height, state.prefs.width, state.prefs.mines)
                };

                // Populate the dialog controls with the current settings
                self2
                    .dlg
                    .hwnd()
                    .GetDlgItem(ResourceId::HeightEdit as u16)
                    .and_then(|edit| edit.SetWindowText(&height.to_string()))?;
                self2
                    .dlg
                    .hwnd()
                    .GetDlgItem(ResourceId::WidthEdit as u16)
                    .and_then(|edit| edit.SetWindowText(&width.to_string()))?;
                self2
                    .dlg
                    .hwnd()
                    .GetDlgItem(ResourceId::MinesEdit as u16)
                    .and_then(|edit| edit.SetWindowText(&mines.to_string()))?;

                Ok(true)
            }
        });

        self.dlg.on().wm_command(DLGID::OK, BN::CLICKED, {
            let dlg = self.dlg.clone();
            let state = self.state.clone();
            move || -> AnyResult<()> {
                // Retrieve and validate user input from the dialog controls
                let height = get_dlg_int(dlg.hwnd(), ResourceId::HeightEdit, MINHEIGHT, 24)?;
                let width = get_dlg_int(dlg.hwnd(), ResourceId::WidthEdit, MINWIDTH, 30)?;
                let max_mines = min(999, (height - 1) * (width - 1));
                let mines = get_dlg_int(dlg.hwnd(), ResourceId::MinesEdit, 10, max_mines)?;

                // Update preferences with the new settings
                let mut state = state.write();
                state.prefs.height = height as usize;
                state.prefs.width = width as usize;
                state.prefs.mines = mines as i16;

                // Close the dialog
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL, BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                // Close the dialog without saving changes
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_help({
            move |help| {
                Help::apply_help_from_info(help.helpinfo, &Help::PREF_HELP_IDS);
                Ok(())
            }
        });

        self.dlg.on().wm_context_menu({
            let self2 = self.clone();
            move |context_menu| {
                // Apply context-sensitive help to all controls except the dialog itself
                if context_menu.hwnd.GetDlgCtrlID() != Ok(0) {
                    Help::apply_help_to_control(&context_menu.hwnd, &Help::PREF_HELP_IDS);
                } else {
                    unsafe { self2.dlg.hwnd().DefWindowProc(context_menu) };
                }
                Ok(())
            }
        });
    }
}

/// Best times dialog
#[derive(Clone)]
struct BestDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
    /// Shared game state
    state: Rc<StateLock<GameState>>,
}

impl BestDialog {
    /// Creates a new BestDialog instance and sets up event handlers.
    /// # Returns
    /// A new BestDialog instance.
    fn new(state: Rc<StateLock<GameState>>) -> Self {
        let dlg = gui::WindowModal::new_dlg(ResourceId::BestDlg as u16);
        let new_self = Self { dlg, state };
        new_self.events();
        new_self
    }

    /// Displays the best-times dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) -> AnyResult<()> {
        self.dlg.show_modal(parent)
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
            .GetDlgItem(ResourceId::BeginTime as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_begin} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ResourceId::BeginName as u16)
            .and_then(|hwnd| hwnd.SetWindowText(name_begin))?;

        // Set the intermediate time and name
        self.dlg
            .hwnd()
            .GetDlgItem(ResourceId::InterTime as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_inter} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ResourceId::InterName as u16)
            .and_then(|hwnd| hwnd.SetWindowText(name_inter))?;

        // Set the expert time and name
        self.dlg
            .hwnd()
            .GetDlgItem(ResourceId::ExpertTime as u16)
            .and_then(|hwnd| hwnd.SetWindowText(&format!("{time_expert} seconds")))?;
        self.dlg
            .hwnd()
            .GetDlgItem(ResourceId::ExpertName as u16)
            .and_then(|hwnd| hwnd.SetWindowText(name_expert))?;

        Ok(())
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let self2 = self.clone();
            move |_| -> AnyResult<bool> {
                let state = self2.state.read();
                self2.reset_best_dialog(
                    state.prefs.best_times[GameType::Begin as usize],
                    state.prefs.best_times[GameType::Inter as usize],
                    state.prefs.best_times[GameType::Expert as usize],
                    &state.prefs.beginner_name,
                    &state.prefs.inter_name,
                    &state.prefs.expert_name,
                )?;

                Ok(true)
            }
        });

        self.dlg
            .on()
            .wm_command(ResourceId::ResetBtn, BN::CLICKED, {
                let self2 = self.clone();
                move || -> AnyResult<()> {
                    // Set best times and names to defaults
                    {
                        let mut state = self2.state.write();

                        // Set all best times to 999 seconds
                        state.prefs.best_times[GameType::Begin as usize] = 999;
                        state.prefs.best_times[GameType::Inter as usize] = 999;
                        state.prefs.best_times[GameType::Expert as usize] = 999;

                        // Set the three best names to the default values
                        state.prefs.beginner_name = DEFAULT_PLAYER_NAME.to_string();
                        state.prefs.inter_name = DEFAULT_PLAYER_NAME.to_string();
                        state.prefs.expert_name = DEFAULT_PLAYER_NAME.to_string();
                    };

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

        self.dlg.on().wm_command(DLGID::OK, BN::CLICKED, {
            let dlg = self.dlg.clone();
            move || -> AnyResult<()> {
                dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_help({
            move |help| {
                Help::apply_help_from_info(help.helpinfo, &Help::BEST_HELP_IDS);
                Ok(())
            }
        });

        self.dlg.on().wm_context_menu({
            let self2 = self.clone();
            move |context_menu| -> AnyResult<()> {
                // Apply context-sensitive help to all controls except the dialog itself
                if context_menu.hwnd.GetDlgCtrlID() != Ok(0) {
                    Help::apply_help_to_control(&context_menu.hwnd, &Help::BEST_HELP_IDS);
                } else {
                    unsafe { self2.dlg.hwnd().DefWindowProc(context_menu) };
                }
                Ok(())
            }
        });
    }
}

/// New record name entry dialog
#[derive(Clone)]
struct EnterDialog {
    /// The modal dialog window
    dlg: gui::WindowModal,
    /// Shared game state
    state: Rc<StateLock<GameState>>,
}

impl EnterDialog {
    /// Creates a new EnterDialog instance and sets up event handlers.
    /// # Returns
    /// A new EnterDialog instance.
    fn new(state: Rc<StateLock<GameState>>) -> Self {
        let dlg = gui::WindowModal::new_dlg(ResourceId::EnterDlg as u16);
        let new_self = Self { dlg, state };
        new_self.events();
        new_self
    }

    /// Displays the name entry dialog as a modal window.
    /// # Arguments
    /// * `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) -> AnyResult<()> {
        self.dlg.show_modal(parent)
    }

    /// Saves the entered high-score name to preferences.
    /// # Returns
    /// A `Result` indicating success or failure.
    fn save_high_score_name(&self) -> AnyResult<()> {
        // Retrieve the entered name from the dialog's edit control
        let new_name = self
            .dlg
            .hwnd()
            .GetDlgItem(ResourceId::NameEdit as u16)
            .and_then(|edit_hwnd| edit_hwnd.GetWindowText())?;

        let mut state = self.state.write();
        match state.prefs.game_type {
            GameType::Begin => state.prefs.beginner_name = new_name,
            GameType::Inter => state.prefs.inter_name = new_name,
            GameType::Expert => state.prefs.expert_name = new_name,
            // Unreachable
            GameType::Other => {}
        }
        Ok(())
    }

    /// Hooks the dialog window messages to their respective handlers.
    fn events(&self) {
        self.dlg.on().wm_init_dialog({
            let self2 = self.clone();
            move |_| -> AnyResult<bool> {
                let (game_type, current_name) = {
                    let state = self2.state.read();
                    let name = match state.prefs.game_type {
                        GameType::Begin => state.prefs.beginner_name.clone(),
                        GameType::Inter => state.prefs.inter_name.clone(),
                        GameType::Expert => state.prefs.expert_name.clone(),
                        // Unreachable
                        GameType::Other => "".to_string(),
                    };
                    (state.prefs.game_type, name)
                };

                self2
                    .dlg
                    .hwnd()
                    .GetDlgItem(ResourceId::BestText as u16)
                    .and_then(|best_hwnd| best_hwnd.SetWindowText(game_type.fastest_time_msg()))?;

                self2
                    .dlg
                    .hwnd()
                    .GetDlgItem(ResourceId::NameEdit as u16)
                    .and_then(|edit_hwnd| {
                        // TODO: Is there a way to do this without sending a message?
                        unsafe {
                            edit_hwnd.SendMessage(SetLimitText {
                                max_chars: Some(CCH_NAME_MAX as u32),
                            });
                        };

                        edit_hwnd.SetWindowText(&current_name)
                    })?;

                Ok(true)
            }
        });

        self.dlg.on().wm_command(ResourceId::OkBtn, BN::CLICKED, {
            let self2 = self.clone();
            move || -> AnyResult<()> {
                self2.save_high_score_name()?;
                self2.dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });

        self.dlg.on().wm_command(DLGID::CANCEL, BN::CLICKED, {
            let self2 = self.clone();
            move || -> AnyResult<()> {
                self2.save_high_score_name()?;
                self2.dlg.hwnd().EndDialog(1)?;
                Ok(())
            }
        });
    }
}
