// build.rs

use std::env;
use std::fs;
use std::path::Path;

const CONFIG_FILE: &str = "odyssey.yaml";

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    fs::copy(CONFIG_FILE, Path::new(&out_dir).join("../../..").join(CONFIG_FILE)).unwrap();
    println!("{}", out_dir.into_string().unwrap());
}