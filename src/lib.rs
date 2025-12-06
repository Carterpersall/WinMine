#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![allow(non_upper_case_globals)]
// TODO: Remove this
#![allow(static_mut_refs)]

mod globals;
mod grafix;
mod pref;
mod rtns;
mod sound;
mod util;
mod winmine;

pub use winmine::run_winmine;
