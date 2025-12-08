use core::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Mutex, OnceLock};

use winsafe::prelude::Handle;
use winsafe::{HICON, HINSTANCE, HMENU, HWND};

use crate::pref::CCH_NAME_MAX;

const F_ICON_BIT: i32 = 0x08;
const F_DEMO_BIT: i32 = 0x10;

pub static bInitMinimized: AtomicBool = AtomicBool::new(false);

pub static fButton1Down: AtomicBool = AtomicBool::new(false);

pub static fBlock: AtomicBool = AtomicBool::new(false);

pub static fIgnoreClick: AtomicBool = AtomicBool::new(false);

pub static fLocalPause: AtomicBool = AtomicBool::new(false);

pub static dypCaption: AtomicI32 = AtomicI32::new(0);

pub static dypMenu: AtomicI32 = AtomicI32::new(0);

pub static dypBorder: AtomicI32 = AtomicI32::new(0);

pub static dxpBorder: AtomicI32 = AtomicI32::new(0);

pub static dxWindow: AtomicI32 = AtomicI32::new(0);

pub static dyWindow: AtomicI32 = AtomicI32::new(0);

pub static dypAdjust: AtomicI32 = AtomicI32::new(0);

pub static dxFrameExtra: AtomicI32 = AtomicI32::new(0);

pub static fStatus: AtomicI32 = AtomicI32::new(F_ICON_BIT | F_DEMO_BIT);

pub struct GlobalState {
	pub h_inst: Mutex<HINSTANCE>,
	pub hwnd_main: Mutex<HWND>,
	pub h_menu: Mutex<HMENU>,
	pub h_icon_main: Mutex<HICON>,
	pub sz_class: Mutex<[u16; CCH_NAME_MAX]>,
	pub sz_time: Mutex<[u16; CCH_NAME_MAX]>,
	pub sz_default_name: Mutex<[u16; CCH_NAME_MAX]>,
}

impl Default for GlobalState {
	fn default() -> Self {
		Self {
			h_inst: Mutex::new(HINSTANCE::NULL),
			hwnd_main: Mutex::new(HWND::NULL),
			h_menu: Mutex::new(HMENU::NULL),
			h_icon_main: Mutex::new(HICON::NULL),
			sz_class: Mutex::new([0; CCH_NAME_MAX]),
			sz_time: Mutex::new([0; CCH_NAME_MAX]),
			sz_default_name: Mutex::new([0; CCH_NAME_MAX]),
		}
	}
}

static GLOBAL_STATE: OnceLock<GlobalState> = OnceLock::new();

pub fn global_state() -> &'static GlobalState {
	GLOBAL_STATE.get_or_init(GlobalState::default)
}
