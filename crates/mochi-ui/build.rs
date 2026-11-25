use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("assets");
    let ico = assets.join("mochi.ico").canonicalize().unwrap();
    println!("cargo:rerun-if-changed={}", ico.display());

    let code = format!(
        "pub static MOCHI_ICO: &[u8] = include_bytes!({:?});\n",
        ico.to_string_lossy()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("assets_gen.rs"), code).expect("write assets_gen.rs");
}
