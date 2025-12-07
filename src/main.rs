#![allow(clippy::not_unsafe_ptr_arg_deref)]

use winmine::run_winmine;
use winsafe::{HINSTANCE, co, prelude::Handle};

pub fn main() {
    let h_instance_handle = HINSTANCE::GetModuleHandle(None).unwrap_or(HINSTANCE::NULL);
    let exit_code = run_winmine(
        h_instance_handle,
        HINSTANCE::NULL,
        std::ptr::null_mut(),
        co::SW::SHOWNORMAL.raw(),
    );
    std::process::exit(exit_code);
}

#[unsafe(no_mangle)]
pub extern "system" fn WinMain(
    h_instance: HINSTANCE,
    h_prev_instance: HINSTANCE,
    lp_cmd_line: *mut u8,
    n_cmd_show: i32,
) -> i32 {
    run_winmine(h_instance, h_prev_instance, lp_cmd_line, n_cmd_show)
}
