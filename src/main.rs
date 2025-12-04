#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ptr::null_mut;
use windows_sys::core::PSTR;
use windows_sys::Win32::Foundation as win32;
use winmine::run_winmine;
use winsafe::{co, prelude::Handle, HINSTANCE};

pub fn main() {
    unsafe {
        let h_instance_handle = HINSTANCE::GetModuleHandle(None)
            .unwrap_or(HINSTANCE::NULL);
        let exit_code = run_winmine(
            h_instance_handle.ptr() as win32::HINSTANCE,
            null_mut(),
            null_mut(),
            co::SW::SHOWNORMAL.raw(),
        );
        std::process::exit(exit_code);
    }
}

#[no_mangle]
pub extern "system" fn WinMain(
    h_instance: win32::HINSTANCE,
    h_prev_instance: win32::HINSTANCE,
    lp_cmd_line: PSTR,
    n_cmd_show: i32,
) -> i32 {
    unsafe { run_winmine(h_instance, h_prev_instance, lp_cmd_line, n_cmd_show) }
}
