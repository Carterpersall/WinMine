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
