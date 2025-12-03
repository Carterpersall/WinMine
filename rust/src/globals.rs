use core::ffi::c_int;
use core::ptr::null_mut;

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, FALSE};
use windows_sys::Win32::UI::WindowsAndMessaging::{HICON, HMENU};

use crate::pref::CCH_NAME_MAX;

const F_ICON_BIT: c_int = 0x08;
const F_DEMO_BIT: c_int = 0x10;

#[no_mangle]
pub static mut bInitMinimized: BOOL = FALSE;
#[no_mangle]
pub static mut hInst: HINSTANCE = null_mut();
#[no_mangle]
pub static mut hwndMain: HWND = null_mut();
#[no_mangle]
pub static mut hMenu: HMENU = null_mut();
#[no_mangle]
pub static mut hIconMain: HICON = null_mut();

#[no_mangle]
pub static mut fButton1Down: BOOL = FALSE;
#[no_mangle]
pub static mut fBlock: BOOL = FALSE;
#[no_mangle]
pub static mut fIgnoreClick: BOOL = FALSE;
#[no_mangle]
pub static mut fLocalPause: BOOL = FALSE;

#[no_mangle]
pub static mut dypCaption: c_int = 0;
#[no_mangle]
pub static mut dypMenu: c_int = 0;
#[no_mangle]
pub static mut dypBorder: c_int = 0;
#[no_mangle]
pub static mut dxpBorder: c_int = 0;
#[no_mangle]
pub static mut dxWindow: c_int = 0;
#[no_mangle]
pub static mut dyWindow: c_int = 0;
#[no_mangle]
pub static mut dypAdjust: c_int = 0;
#[no_mangle]
pub static mut dxFrameExtra: c_int = 0;

#[no_mangle]
pub static mut fStatus: c_int = F_ICON_BIT | F_DEMO_BIT;

#[no_mangle]
pub static mut szClass: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];
#[no_mangle]
pub static mut szTime: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];
#[no_mangle]
pub static mut szDefaultName: [u16; CCH_NAME_MAX] = [0; CCH_NAME_MAX];
