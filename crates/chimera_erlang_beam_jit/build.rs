//! Build script for chimera_erlang_beam_jit
//!
//! Builds the C++ JIT compiler using CMake.

use std::env;
use std::path::PathBuf;

fn main() {
    let src_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let build_dir = PathBuf::from(&env::var("OUT_DIR").unwrap()).join("jit-build");

    // Create build directory
    std::fs::create_dir_all(&build_dir).expect("Failed to create build directory");

    // Run CMake configure
    let status = std::process::Command::new("cmake")
        .current_dir(&build_dir)
        .args(&["-DCMAKE_BUILD_TYPE=Release", src_dir.to_str().unwrap()])
        .status()
        .expect("Failed to run cmake");

    if !status.success() {
        panic!("cmake configure failed");
    }

    // Build
    let status = std::process::Command::new("cmake")
        .current_dir(&build_dir)
        .args(&["--build", ".", "--target", "chimera_jit"])
        .status()
        .expect("Failed to build");

    if !status.success() {
        panic!("cmake build failed");
    }

    println!("cargo:rerun-if-changed=CMakeLists.txt");
    println!("cargo:rerun-if-changed=src/");
}
