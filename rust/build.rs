use std::iter;
use std::path::PathBuf;

fn main() {
    let rc_path = PathBuf::from(".." ).join("res.rc");
    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed=../res.h");
    println!("cargo:rerun-if-changed=../winmine.manifest");
    println!("cargo:rerun-if-changed=../strings.inc");
    println!("cargo:rerun-if-changed=../bmp");

    embed_resource::compile(rc_path, iter::empty::<&'static str>());
}
