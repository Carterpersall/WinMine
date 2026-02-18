use std::iter;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rc_path = PathBuf::from("resources/res.rc");
    embed_resource::compile(rc_path, iter::empty::<&'static str>())
        .manifest_required()
        .map_err(Into::into)
}
