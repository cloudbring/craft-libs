//! Records the compiler and profile that built the harness, for the machine fingerprint.
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc)
        .arg("-V")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());
    println!("cargo:rustc-env=CRAFT_BENCH_RUSTC={version}");
    println!("cargo:rustc-env=CRAFT_BENCH_PROFILE={}", std::env::var("PROFILE").unwrap_or_default());
    println!("cargo:rustc-env=CRAFT_BENCH_OPT_LEVEL={}", std::env::var("OPT_LEVEL").unwrap_or_default());
    println!("cargo:rerun-if-changed=build.rs");
}
