use core::ffi::c_int;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, AtomicI32};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::{HICON, HMENU};

use crate::pref::CCH_NAME_MAX;

const F_ICON_BIT: c_int = 0x08;
const F_DEMO_BIT: c_int = 0x10;

pub static bInitMinimized: AtomicBool = AtomicBool::new(false);

pub static mut hInst: HINSTANCE = null_mut();

pub static mut hwndMain: HWND = null_mut();

pub static mut hMenu: HMENU = null_mut();

pub static mut hIconMain: HICON = null_mut();

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

pub static mut szClass: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];

pub static mut szTime: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];

pub static mut szDefaultName: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];
