#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use core::ffi::c_int;
use core::ptr::null_mut;
use windows_sys::core::PSTR;
use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use winmine::run_winmine;

pub fn main() {
    unsafe {
        let exit_code = run_winmine(
            GetModuleHandleW(null_mut()),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        );
        std::process::exit(exit_code);
    }
}

#[no_mangle]
pub extern "system" fn WinMain(
    h_instance: HINSTANCE,
    h_prev_instance: HINSTANCE,
    lp_cmd_line: PSTR,
    n_cmd_show: c_int,
) -> c_int {
    unsafe { run_winmine(h_instance, h_prev_instance, lp_cmd_line, n_cmd_show) }
}
