#![allow(non_snake_case)]

mod globals;
mod grafix;
mod pref;
mod rtns;
mod sound;
mod util;
mod winmine;

use crate::winmine::run_winmine;
use winsafe::{HINSTANCE, co, prelude::Handle};

fn main() {
    let h_instance_handle = HINSTANCE::GetModuleHandle(None).unwrap_or(HINSTANCE::NULL);
    let exit_code = run_winmine(h_instance_handle, co::SW::SHOWNORMAL.raw());
    std::process::exit(exit_code);
}
