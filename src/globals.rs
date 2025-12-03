use core::ffi::c_int;
use core::ptr::null_mut;

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{FALSE, HINSTANCE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::{HICON, HMENU};

use crate::pref::CCH_NAME_MAX;

const F_ICON_BIT: c_int = 0x08;
const F_DEMO_BIT: c_int = 0x10;

pub static mut bInitMinimized: BOOL = FALSE;

pub static mut hInst: HINSTANCE = null_mut();

pub static mut hwndMain: HWND = null_mut();

pub static mut hMenu: HMENU = null_mut();

pub static mut hIconMain: HICON = null_mut();

pub static mut fButton1Down: BOOL = FALSE;

pub static mut fBlock: BOOL = FALSE;

pub static mut fIgnoreClick: BOOL = FALSE;

pub static mut fLocalPause: BOOL = FALSE;

pub static mut dypCaption: c_int = 0;

pub static mut dypMenu: c_int = 0;

pub static mut dypBorder: c_int = 0;

pub static mut dxpBorder: c_int = 0;

pub static mut dxWindow: c_int = 0;

pub static mut dyWindow: c_int = 0;

pub static mut dypAdjust: c_int = 0;

pub static mut dxFrameExtra: c_int = 0;

pub static mut fStatus: c_int = F_ICON_BIT | F_DEMO_BIT;

pub static mut szClass: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];

pub static mut szTime: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];

pub static mut szDefaultName: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];
