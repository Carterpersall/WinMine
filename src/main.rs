#![allow(clippy::not_unsafe_ptr_arg_deref)]

use winmine::run_winmine;
use winsafe::{co, prelude::Handle, HINSTANCE};

pub fn main() {
    let exit_code = unsafe {
        let h_instance_handle = HINSTANCE::GetModuleHandle(None).unwrap_or(HINSTANCE::NULL);
        run_winmine(
            h_instance_handle.ptr() as _ ,
            HINSTANCE::NULL.ptr() as _ ,
            std::ptr::null_mut(),
            co::SW::SHOWNORMAL.raw(),
        )
    };
    std::process::exit(exit_code);
}

#[no_mangle]
pub extern "system" fn WinMain(
    h_instance: HINSTANCE,
    h_prev_instance: HINSTANCE,
    lp_cmd_line: *mut u8,
    n_cmd_show: i32,
) -> i32 {
    unsafe {
        run_winmine(
            h_instance.ptr() as _,
            h_prev_instance.ptr() as _,
            lp_cmd_line,
            n_cmd_show,
        )
    }
}
