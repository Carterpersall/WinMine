//! Minesweeper Game - Main Entry Point

// Clippy and Built-in Lints used for style and correctness checks
// Some lints are commented out as they are useful for targeted checks that
// may not be applicable project-wide (e.g., absolute_paths).
//#![warn(clippy::absolute_paths)]
#![warn(clippy::clone_on_ref_ptr)]
#![warn(clippy::collection_is_never_read)]
//#![warn(clippy::doc_markdown)]
#![warn(clippy::empty_structs_with_brackets)]
//#![warn(clippy::indexing_slicing)]
#![warn(clippy::manual_string_new)]
#![warn(clippy::map_err_ignore)]
#![warn(clippy::match_bool)]
#![warn(clippy::multiple_unsafe_ops_per_block)]
#![warn(clippy::missing_const_for_fn)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(clippy::missing_inline_in_public_items)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::needless_bitwise_bool)]
#![warn(clippy::needless_collect)]
#![warn(clippy::needless_continue)]
#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::needless_pass_by_value)]
#![warn(clippy::option_if_let_else)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::shadow_unrelated)]
#![warn(clippy::significant_drop_in_scrutinee)]
//#![warn(clippy::significant_drop_tightening)]
//#![warn(clippy::single_call_fn)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::str_to_string)]
#![warn(clippy::trivially_copy_pass_by_ref)]
#![warn(clippy::unused_self)]
#![warn(clippy::unused_trait_names)]
#![warn(clippy::useless_let_if_seq)]
#![warn(missing_docs)]
#![warn(redundant_imports)]
#![warn(redundant_lifetimes)]
#![warn(unnameable_types)]
#![warn(unreachable_pub)]
#![warn(unused_import_braces)]
#![warn(unused_qualifications)]
//#![warn(unused_results)]

mod globals;
mod grafix;
mod help;
mod pref;
mod rtns;
mod sound;
mod util;
mod winmine;
mod xyzzy;

use crate::winmine::WinMineMainWindow;
use winsafe::HINSTANCE;

/// The main entry point for the Minesweeper game application.
/// It initializes the application and starts the main window loop.
/// # Returns
/// - `Ok(())` - If the application ran successfully and exited without errors.
/// - `Err` - If there was an error during app execution
fn main() -> Result<(), Box<dyn core::error::Error>> {
    // Get the module handle for the current process
    let h_instance_handle = HINSTANCE::GetModuleHandle(None)?;
    // Run the main WinMine application logic
    WinMineMainWindow::run(&h_instance_handle)
}
