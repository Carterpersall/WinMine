#![no_main]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use core::ffi::c_int;
use windows_sys::core::PSTR;
use windows_sys::Win32::Foundation::HINSTANCE;
use winmine_rust::run_winmine;

#[no_mangle]
pub extern "system" fn WinMain(
    h_instance: HINSTANCE,
    h_prev_instance: HINSTANCE,
    lp_cmd_line: PSTR,
    n_cmd_show: c_int,
) -> c_int {
    unsafe { run_winmine(h_instance, h_prev_instance, lp_cmd_line, n_cmd_show) }
}
