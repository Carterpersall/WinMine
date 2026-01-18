// Clippy Lints used for style and correctness checks
// Some lints are commented out as they are useful for targeted checks that
// may not be applicable project-wide (e.g., absolute_paths).
//#![warn(clippy::absolute_paths)]
#![warn(clippy::collection_is_never_read)]
//#![warn(clippy::doc_markdown)]
//#![warn(clippy::indexing_slicing)]
//#![warn(clippy::map_err_ignore)]
//#![warn(clippy::multiple_unsafe_ops_per_block)]
#![warn(clippy::missing_const_for_fn)]
//#![warn(clippy::missing_docs_in_private_items)]
#![warn(clippy::needless_pass_by_value)]
//#![warn(clippy::option_if_let_else)]
#![warn(clippy::redundant_pub_crate)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::shadow_unrelated)]
//#![warn(clippy::significant_drop_tightening)]
//#![warn(clippy::single_call_fn)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::unused_trait_names)]
#![warn(clippy::useless_let_if_seq)]
#![allow(non_snake_case)]

mod globals;
mod grafix;
mod pref;
mod rtns;
mod sound;
mod util;
mod winmine;

use crate::winmine::run_winmine;
use winsafe::{HINSTANCE, co::SW, prelude::Handle as _};

fn main() {
    // Get the module handle for the current process
    let h_instance_handle = HINSTANCE::GetModuleHandle(None).unwrap_or(HINSTANCE::NULL);
    // Run the main WinMine application logic
    // TODO: Handle the return value appropriately
    let _ = run_winmine(&h_instance_handle, SW::SHOWNORMAL.raw());
}
