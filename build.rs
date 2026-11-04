use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/windows/backleaf.ico");
    println!("cargo:rerun-if-changed=assets/windows/backleaf.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("backleaf-icon.res");
    let status = Command::new("rc.exe")
        .arg("/nologo")
        .arg(format!("/fo{}", output.display()))
        .arg("assets/windows/backleaf.rc")
        .status()
        .expect("Windows SDK rc.exe must be on PATH (use a VS developer shell)");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=backleaf={}", output.display());
}
