use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=migrations");

    if env::var("CRYSTAL_FORGE_UI_DIST").is_ok() {
        return;
    }

    let manifest_dir = match env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };
    let fallback = manifest_dir.join("../web-ui/assets");

    println!(
        "cargo:rustc-env=CRYSTAL_FORGE_UI_DIST={}",
        fallback.display()
    );
}
