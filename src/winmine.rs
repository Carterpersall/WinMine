//! Main window and event handling for the Minesweeper game.

use core::cmp::{max, min};
use core::ops::Deref as _;
use std::rc::Rc;

use windows_sys::Win32::Data::HtmlHelp::{HH_DISPLAY_INDEX, HH_DISPLAY_TOC};

use winsafe::co::{BN, DLGID, HELPW, ICC, IDC, MK, SM, STOCK_BRUSH, SW, VK, WA, WM, WS};
use winsafe::msg::{WndMsg, em::SetLimitText, wm::Destroy};
use winsafe::{
    AdjustWindowRectExForDpi, AnyResult, GetSystemMetrics, HBRUSH, HINSTANCE, INITCOMMONCONTROLSEX,
    IdIdiStr, IdStr, InitCommonControlsEx, POINT, PtInRect, RECT, SIZE, gui, prelude::*,
};

use crate::globals::{BASE_DPI, DEFAULT_PLAYER_NAME, GAME_NAME, MSG_CREDIT, MSG_VERSION_NAME};
use crate::grafix::ButtonSprite;
use crate::help::Help;
use crate::pref::{CCH_NAME_MAX, GameType, MINHEIGHT, MINWIDTH};
use crate::rtns::{AdjustFlag, GameState, ID_TIMER, StatusFlag};
use crate::sound::Sound;
use crate::util::{ResourceId, StateLock, get_dlg_int};

/// `WM_APP` request code posted to the main window when a new best time is
/// recorded.
///
/// The main UI thread handles this by showing the name-entry dialog, then the
/// best-times dialog.
pub(crate) const NEW_RECORD_DLG: usize = 1;

/// Struct containing the main window with its event handlers and the shared state.
#[derive(Clone)]
pub(crate) struct WinMineMainWindow {
    /// The main window, containing the HWND and event callbacks
    pub wnd: gui::WindowMain,
    /// Shared state for the game
    pub state: Rc<StateLock<GameState>>,
}

impl WinMineMainWindow {
    /// Creates the main window and hooks its events.
    /// # Arguments
    /// - `wnd`: The main window to wrap.
    fn new(wnd: gui::WindowMain) -> Self {
        let new_self = Self {
            wnd,
            state: Rc::new(StateLock::new(GameState::new())),
        };
        new_self.events();
        new_self
    }

    /// Runs the WinMine application.
    /// # Arguments
    /// - `h_instance`: The application instance handle.
    /// # Returns
    /// - `Ok(())` - If the application ran successfully and exited without errors.
    /// - `Err` - If there was an error during app execution.
    pub(crate) fn run(hinst: &HINSTANCE) -> Result<(), Box<dyn core::error::Error>> {
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

    /// Handles smiley-face click completion when the left button is released.
    ///
    /// TODO: Move function into `GameState`.
    ///       Moving this function is blocked by the function `start_game`
    /// # Arguments
    /// - `point`: The coordinates of the mouse cursor.
    /// # Returns
    /// - `Ok(())` - If the mouse button release was handled successfully.
    /// - `Err` - If an error occurred while handling the mouse button release.
    fn handle_face_button_lbutton_up(&self, point: POINT) -> AnyResult<()> {
        let rc = {
            let grafix = &self.state.read().grafix;
            RECT {
                left: (grafix.wnd_pos.x - grafix.dims.button.cx) / 2,
                right: (grafix.wnd_pos.x + grafix.dims.button.cx) / 2,
                top: grafix.dims.top_led,
                bottom: grafix.dims.top_led + grafix.dims.button.cy,
            }
        };
        if PtInRect(rc, point) {
            self.state.write().btn_face_state = ButtonSprite::Happy;
            self.state
                .read()
                .grafix
                .draw_button(self.wnd.hwnd().GetDC()?.deref(), ButtonSprite::Happy)?;
            self.start_game()?;
        } else {
            let state = self.state.read();
            state
                .grafix
                .draw_button(self.wnd.hwnd().GetDC()?.deref(), state.btn_face_state)?;
        }

        Ok(())
    }

    /* Helper Functions */

    /// Adjusts the main window size and position based on the current board and menu state.
    ///
    /// TODO: Move this function into `GameState`.
    ///       Moving this function is complicated by the fact that it needs to call `MoveWindow`,
    ///       which sends a `WM_WINDOWPOSCHANGED` message to the main window,
    ///       which needs to obtain a lock on the game state, always causing a deadlock.
    /// This function is called whenever the board or menu state changes to ensure
    /// that the main window is appropriately sized and positioned on the screen.
    /// # Arguments
    /// - `f_adjust` - Flags indicating how to adjust the window (e.g., resize).
    /// # Returns
    /// - `Ok(())` - If the window adjustment was successful.
    /// - `Err` - If an error occurred while adjusting the window.
    pub(crate) fn adjust_window(&self, mut f_adjust: AdjustFlag) -> AnyResult<()> {
        // Calculate desired window size based on board dimensions and DPI scaling
        let (dx_window, dy_window) = {
            let state = self.state.read();
            let dx_window = state.grafix.dims.block.cx * state.prefs.width as i32
                + state.grafix.dims.left_space
                + state.grafix.dims.right_space;
            let dy_window = state.grafix.dims.block.cy * state.prefs.height as i32
                + state.grafix.dims.grid_offset
                + state.grafix.dims.bottom_space;
            (dx_window, dy_window)
        };
        self.state.write().grafix.wnd_pos = POINT::with(dx_window, dy_window);

        // Get the current window position from preferences
        let mut pos = self.state.read().prefs.wnd_pos;

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
            self.state.read().grafix.dims.dpi,
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
        let mut excess = pos.x + dx_window + frame_extra - cx_screen;
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize;
            pos.x -= excess;
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
        excess = pos.y + dy_window + dyp_adjust - cy_screen;
        if excess > 0 {
            f_adjust |= AdjustFlag::Resize;
            pos.y -= excess;
        }

        // If a window resize has been requested, move and resize the window accordingly
        if f_adjust.contains(AdjustFlag::Resize) {
            self.wnd.hwnd().MoveWindow(
                pos,
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
        self.state.write().prefs.wnd_pos = pos;

        Ok(())
    }

    /* Event Handlers */

    /// Hooks the window messages to their respective handlers.
    fn events(&self) {
        self.wnd.on().wm_create({
            let self2 = self.clone();
            move |_create| -> AnyResult<i32> {
                // Sync global DPI state to the actual monitor DPI where the window was created.
                let mut dpi = self2.wnd.hwnd().GetDpiForWindow();
                if dpi == 0 {
                    dpi = BASE_DPI;
                }
                {
                    let mut state = self2.state.write();
                    state.grafix.dims.update_dpi(dpi);

                    // Initialize graphics resources based on the current DPI and color settings
                    let color = state.prefs.color;
                    state.grafix.load_bitmaps(self2.wnd.hwnd(), color)?;

                    // Reset the game board to a blank state
                    state.clear_field();
                }

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
                let mut dpi = ((msg.wparam) & 0xFFFF) as u32;
                if dpi == 0 {
                    dpi = BASE_DPI;
                }
                self2.state.write().grafix.dims.update_dpi(dpi);

                // Our block + face-button bitmaps are cached pre-scaled, so they must be rebuilt after a DPI transition.
                let color = self2.state.read().prefs.color;
                self2
                    .state
                    .write()
                    .grafix
                    .load_bitmaps(self2.wnd.hwnd(), color)?;

                // Adjust the window size and position based on the new DPI
                self2.adjust_window(AdjustFlag::ResizeAndRedraw)?;
                Ok(0)
            }
        });

        self.wnd.on().wm_window_pos_changed({
            let self2 = self.clone();
            move |wnd_pos| {
                let mut state = self2.state.write();
                if state.game_status.contains(StatusFlag::Minimized) && !self2.wnd.hwnd().IsIconic()
                {
                    // If the window was previously minimized but is no longer, it is being restored from a minimized state
                    state.game_status.remove(StatusFlag::Pause);
                    state.game_status.remove(StatusFlag::Minimized);
                    state.resume_game();
                } else if !state.game_status.contains(StatusFlag::Minimized)
                    && self2.wnd.hwnd().IsIconic()
                {
                    // If the window was not previously minimized but now is, it is being minimized
                    state.pause_game();
                    state.game_status.insert(StatusFlag::Pause);
                    state.game_status.insert(StatusFlag::Minimized);
                } else if !state.game_status.contains(StatusFlag::Minimized) {
                    // If the window is not minimized, but its position has changed, update the stored window position in preferences
                    state.prefs.wnd_pos = POINT {
                        x: wnd_pos.windowpos.x,
                        y: wnd_pos.windowpos.y,
                    };
                }
                Ok(())
            }
        });

        // Handle `WM_APP` requests posted from non-UI modules.
        self.wnd.on().wm(WM::APP, {
            let self2 = self.clone();
            move |msg: WndMsg| {
                if msg.wparam == NEW_RECORD_DLG {
                    EnterDialog::new(Rc::clone(&self2.state)).show_modal(&self2.wnd)?;
                    BestDialog::new(Rc::clone(&self2.state)).show_modal(&self2.wnd)?;
                    return Ok(0);
                }
                Ok(0)
            }
        });

        self.wnd.on().wm_key_down({
            let self2 = self.clone();
            move |key| {
                match key.vkey_code {
                    code if code == VK::F4 => {
                        // TODO: This code is duplicated in the `ResourceId::Sound` menu command handler.
                        //       Refactor to eliminate duplication.
                        let sound_enabled = self2.state.read().prefs.sound_enabled;
                        self2.state.write().prefs.sound_enabled = if sound_enabled {
                            // Stop any currently playing sounds and disable sound
                            Sound::reset();
                            false
                        } else {
                            // Enable sound if the sound system is responsive
                            Sound::reset()
                        };

                        // Update the menu bar to reflect the new sound state
                        self2.set_menu_bar()?;
                    }
                    code if code == VK::SHIFT => GameState::handle_xyzzys_shift(),
                    _ => GameState::handle_xyzzys_default_key(key.vkey_code),
                }

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
                self2
                    .state
                    .write()
                    .handle_mouse_move(self2.wnd.hwnd(), msg.vkey_code, msg.coords)
            }
        });

        self.wnd.on().wm_r_button_down({
            let self2 = self.clone();
            move |r_btn| {
                self2.state.write().handle_rbutton_down(
                    self2.wnd.hwnd(),
                    r_btn.vkey_code,
                    r_btn.coords,
                )
            }
        });

        self.wnd.on().wm_r_button_dbl_clk({
            let self2 = self.clone();
            move |r_btn| {
                self2.state.write().handle_rbutton_down(
                    self2.wnd.hwnd(),
                    r_btn.vkey_code,
                    r_btn.coords,
                )
            }
        });

        self.wnd.on().wm_r_button_up({
            let self2 = self.clone();
            move |r_btn| {
                // If the right button is released while the left button is down, finish the drag operation
                // This replicates the original behavior, though it does add some complexity.
                if r_btn.vkey_code.has(MK::LBUTTON) {
                    self2
                        .state
                        .write()
                        .finish_primary_button_drag(self2.wnd.hwnd())?;
                }
                Ok(())
            }
        });

        self.wnd.on().wm_m_button_down({
            let self2 = self.clone();
            move |m_btn| {
                self2.state.write().handle_mbutton_down(
                    self2.wnd.hwnd(),
                    m_btn.vkey_code,
                    m_btn.coords,
                )
            }
        });

        self.wnd.on().wm_m_button_dbl_clk({
            let self2 = self.clone();
            move |m_btn| {
                self2.state.write().handle_mbutton_down(
                    self2.wnd.hwnd(),
                    m_btn.vkey_code,
                    m_btn.coords,
                )
            }
        });

        self.wnd.on().wm_m_button_up({
            let self2 = self.clone();
            move |_m_btn| {
                self2
                    .state
                    .write()
                    .finish_primary_button_drag(self2.wnd.hwnd())
            }
        });

        self.wnd.on().wm_l_button_down({
            let self2 = self.clone();
            move |l_btn| {
                self2.state.write().handle_lbutton_down(
                    self2.wnd.hwnd(),
                    l_btn.vkey_code,
                    l_btn.coords,
                )
            }
        });

        self.wnd.on().wm_l_button_dbl_clk({
            let self2 = self.clone();
            move |l_btn| {
                self2.state.write().handle_lbutton_down(
                    self2.wnd.hwnd(),
                    l_btn.vkey_code,
                    l_btn.coords,
                )
            }
        });

        self.wnd.on().wm_l_button_up({
            let self2 = self.clone();
            move |l_btn| {
                if self2.state.read().btn_face_pressed {
                    self2.state.write().btn_face_pressed = false;
                    self2.handle_face_button_lbutton_up(l_btn.coords)?;
                } else {
                    self2
                        .state
                        .write()
                        .finish_primary_button_drag(self2.wnd.hwnd())?;
                }
                Ok(())
            }
        });

        self.wnd.on().wm_activate({
            let self2 = self.clone();
            move |activate| {
                if activate.event == WA::CLICKACTIVE {
                    self2.state.write().ignore_next_click = true;
                }
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
                // Show the preferences dialog
                PrefDialog::new(Rc::clone(&self2.state)).show_modal(&self2.wnd)?;

                if self2.state.read().prefs.game_type == GameType::Other {
                    // If a custom game was configured, start it
                    self2.set_menu_bar()?;
                    self2.start_game()?;
                }
                Ok(())
            }
        });

        self.wnd.on().wm_command_acc_menu(ResourceId::Sound, {
            let self2 = self.clone();
            move || {
                let sound_enabled = self2.state.read().prefs.sound_enabled;
                self2.state.write().prefs.sound_enabled = if sound_enabled {
                    // Stop any currently playing sounds and disable sound
                    Sound::reset();
                    false
                } else {
                    // Enable sound if the sound system is responsive
                    Sound::reset()
                };

                // Update the menu bar to reflect the new sound state
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
            move || BestDialog::new(Rc::clone(&self2.state)).show_modal(&self2.wnd)
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
    /// The game state, shared from the main window to read and write preferences
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
    /// - `parent`: The parent GUI element for the modal dialog.
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
            let self2 = self.clone();
            move || -> AnyResult<()> {
                // Retrieve and validate user input from the dialog controls
                let height = get_dlg_int(self2.dlg.hwnd(), ResourceId::HeightEdit, MINHEIGHT, 24)?;
                let width = get_dlg_int(self2.dlg.hwnd(), ResourceId::WidthEdit, MINWIDTH, 30)?;
                let max_mines = min(999, (height - 1) * (width - 1));
                let mines = get_dlg_int(self2.dlg.hwnd(), ResourceId::MinesEdit, 10, max_mines)?;

                // Update preferences with the new settings
                {
                    let mut state = self2.state.write();
                    state.prefs.height = height as usize;
                    state.prefs.width = width as usize;
                    state.prefs.mines = mines as i16;
                    state.prefs.game_type = GameType::Other;
                }

                // Close the dialog
                self2.dlg.hwnd().EndDialog(1)?;
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
                    // Show a context menu when right clicking the title bar
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
    /// Creates a new `BestDialog` instance and sets up event handlers.
    /// # Arguments
    /// - `state`: The shared game state to access best times and names.
    fn new(state: Rc<StateLock<GameState>>) -> Self {
        let dlg = gui::WindowModal::new_dlg(ResourceId::BestDlg as u16);
        let new_self = Self { dlg, state };
        new_self.events();
        new_self
    }

    /// Displays the best-times dialog as a modal window.
    /// # Arguments
    /// - `parent`: The parent GUI element for the modal dialog.
    /// # Returns
    /// `Ok(())` - If the dialog was displayed successfully.
    /// `Err` - If an error occurred while displaying the dialog.
    fn show_modal(&self, parent: &impl GuiParent) -> AnyResult<()> {
        self.dlg.show_modal(parent)
    }

    /* Helper Functions */

    /// Resets the best scores dialog with the provided times and names.
    /// # Arguments
    /// - `time_begin` - The best time for the beginner level.
    /// - `time_inter` - The best time for the intermediate level.
    /// - `time_expert` - The best time for the expert level.
    /// - `name_begin` - The name associated with the beginner level best time.
    /// - `name_inter` - The name associated with the intermediate level best time.
    /// - `name_expert` - The name associated with the expert level best time.
    /// # Returns
    /// `Ok(())` - If the dialog was set successfully.
    /// `Err` - If an error occurred while setting the dialog.
    fn set_best_dialog(
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
                self2.set_best_dialog(
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
                        state.prefs.beginner_name = DEFAULT_PLAYER_NAME.to_owned();
                        state.prefs.inter_name = DEFAULT_PLAYER_NAME.to_owned();
                        state.prefs.expert_name = DEFAULT_PLAYER_NAME.to_owned();
                    };

                    self2.set_best_dialog(
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
                    // Show a context menu when right clicking the title bar
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
    /// Creates a new `EnterDialog` instance and sets up event handlers.
    /// # Arguments
    /// - `state`: A reference-counted pointer to the shared game state.
    fn new(state: Rc<StateLock<GameState>>) -> Self {
        let dlg = gui::WindowModal::new_dlg(ResourceId::EnterDlg as u16);
        let new_self = Self { dlg, state };
        new_self.events();
        new_self
    }

    /// Displays the name entry dialog as a modal window.
    /// # Arguments
    /// - `parent`: The parent GUI element for the modal dialog.
    fn show_modal(&self, parent: &impl GuiParent) -> AnyResult<()> {
        self.dlg.show_modal(parent)
    }

    /// Saves the entered high-score name to preferences.
    /// # Returns
    /// `Ok(())` - If the name was saved successfully.
    /// `Err` - If an error occurred while getting the entered name.
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
                        GameType::Other => String::new(),
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
                        // TODO: The only way to do this without unsafe is for this to be a `WinSafe` `Edit` control,
                        //       which has the `limit_text` function.
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
