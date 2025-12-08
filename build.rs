use std::iter;
use std::path::PathBuf;

fn main() {
    let rc_path = PathBuf::from("resources/res.rc");
    if embed_resource::compile(rc_path, iter::empty::<&'static str>())
        .manifest_required()
        .is_err()
    {
        panic!("Failed to compile resources");
    }
}
