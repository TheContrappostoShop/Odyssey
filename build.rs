// build.rs

use std::env;
use std::fs;
use std::path::Path;

const CONFIG_FILE: &str = "odyssey.yaml";
const API_HELPER_FILE: &str = "apiHelper.py";

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    fs::copy(CONFIG_FILE, Path::new(&out_dir).join("../../..").join(CONFIG_FILE)).unwrap();
    fs::copy(CONFIG_FILE, Path::new(&out_dir).join("../../..").join(API_HELPER_FILE)).unwrap();
}
