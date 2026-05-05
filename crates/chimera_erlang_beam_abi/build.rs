//! Build script for chimera_erlang_beam_abi.
//!
//! This build script integrates Zig build into the Cargo workflow.
//! It invokes `zig build-lib` to produce libbeamz.a, then configures
//! Cargo to link against it.
//!
//! The `zig` feature (enabled by default) controls whether to build
//! and link the Zig library. Disable with `--no-default-features`.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Check if zig feature is enabled
    let zig_feature = env::var("CARGO_FEATURE_ZIG").unwrap_or_default() == "1";

    if !zig_feature {
        println!("cargo::warning=Zig feature disabled - skipping Zig build");
        return;
    }

    println!("cargo::rerun-if-changed=build.rs");

    // Get the Zig source directory (at project root, not in crates/)
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let project_root = manifest_dir.parent().unwrap().parent().unwrap();
    let zig_dir = project_root.join("zig");

    // Check if the Zig source directory exists
    if !zig_dir.exists() {
        println!(
            "cargo::warning=Zig source directory not found at {} - skipping Zig build",
            zig_dir.display()
        );
        return;
    }

    println!(
        "cargo::rerun-if-changed={}",
        zig_dir.join("build.zig").display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        zig_dir.join("build.zig.zon").display()
    );
    println!("cargo::rerun-if-changed={}", zig_dir.join("src").display());
    println!("cargo::rerun-if-env-changed=CHIMERA_BEAM_EXTERNAL_BEAMZ_LIB");

    if let Ok(external_lib) = env::var("CHIMERA_BEAM_EXTERNAL_BEAMZ_LIB") {
        let external_path = PathBuf::from(external_lib);
        if external_path.exists() {
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            let dest_path = out_dir.join("libbeamz.a");
            if let Err(e) = std::fs::copy(&external_path, &dest_path) {
                println!(
                    "cargo::warning=Failed to copy external beamz archive to OUT_DIR: {}",
                    e
                );
            } else {
                println!("cargo::rustc-link-search=native={}", out_dir.display());
                println!("cargo::rustc-link-lib=static=beamz");
                return;
            }
        }
        println!(
            "cargo::warning=CHIMERA_BEAM_EXTERNAL_BEAMZ_LIB is unusable: {}",
            external_path.display()
        );
    }

    // Check if zig is available
    let zig_available = Command::new("zig")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !zig_available {
        println!("cargo::warning=zig not found in PATH - skipping Zig build");
        return;
    }

    // Build the Zig library using build-lib for direct .a output with PIC
    // This produces a position-independent static library
    let status = Command::new("zig")
        .current_dir(&zig_dir)
        .args([
            "build-lib",
            "src/root.zig",
            "-Doptimize=ReleaseFast",
            "-fPIC",
            "-femit-bin=libbeamz.a",
        ])
        .status();

    let build_success = status.map(|s| s.success()).unwrap_or(false);

    if !build_success {
        println!("cargo::warning=zig build-lib failed - ABI crate will not link properly");
        return;
    }

    // build-lib outputs libbeamz.a to the current directory (zig_dir)
    let lib_path = zig_dir.join("libbeamz.a");
    if lib_path.exists() {
        // Get OUT_DIR for copying the library
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let dest_path = out_dir.join("libbeamz.a");

        // Copy libbeamz.a to OUT_DIR so Cargo can link it
        if let Err(e) = std::fs::copy(&lib_path, &dest_path) {
            println!("cargo::warning=Failed to copy libbeamz.a to OUT_DIR: {}", e);
        }

        println!("cargo::rustc-link-search=native={}", out_dir.display());
        println!("cargo::rustc-link-lib=static=beamz");
    } else {
        println!(
            "cargo::warning=libbeamz.a not found at {}",
            lib_path.display()
        );
    }
}
