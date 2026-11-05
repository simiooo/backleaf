use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/windows/backleaf.ico");
    println!("cargo:rerun-if-changed=assets/windows/backleaf.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=src/native/ole_bridge.cpp");
    cc::Build::new()
        .cpp(true)
        .file("src/native/ole_bridge.cpp")
        .flag("/std:c++20")
        .flag("/EHsc")
        .flag("/utf-8")
        .compile("backleaf_ole");
    for library in ["ole32", "oleaut32", "uuid", "shell32", "user32"] {
        println!("cargo:rustc-link-lib={library}");
    }
    println!("cargo:rerun-if-changed=src/native/archive_helper.cpp");
    println!("cargo:rerun-if-changed=assets/archive-engine/7z2605-src.tar.xz");
    let archive_out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let sdk = archive_out.join("7zip-sdk");
    std::fs::create_dir_all(&sdk).expect("archive SDK directory");
    let source = std::fs::canonicalize("assets/archive-engine/7z2605-src.tar.xz")
        .expect("bundled 7-Zip source");
    let extracted = Command::new("tar.exe")
        .args(["-xf"])
        .arg(source)
        .arg("-C")
        .arg(&sdk)
        .status()
        .expect("Windows tar.exe for bundled source");
    assert!(extracted.success(), "Bundled archive SDK extraction failed");
    let target = env::var("TARGET").unwrap();
    let compiler = cc::windows_registry::find_tool(&target, "cl.exe")
        .expect("MSVC C++ compiler for archive helper");
    let helper = archive_out.join("BackleafArchive.exe");
    let compiled = compiler
        .to_command()
        .args(["/nologo", "/EHsc", "/std:c++20", "/O2", "/utf-8", "/MT"])
        .arg(format!("/I{}", sdk.display()))
        .arg(format!("/Fe{}", helper.display()))
        .arg(format!(
            "/Fo{}",
            archive_out.join("archive-helper.obj").display()
        ))
        .arg("src/native/archive_helper.cpp")
        .args(["/link", "ole32.lib", "oleaut32.lib", "uuid.lib"])
        .status()
        .expect("archive helper compile");
    assert!(
        compiled.success(),
        "Native archive helper compilation failed"
    );
    println!("cargo:rerun-if-changed=src/native/shell_helper.cpp");
    println!("cargo:rerun-if-changed=src/native/shell_common.hpp");
    let shell = compiler
        .to_command()
        .args(["/nologo", "/EHsc", "/std:c++20", "/O2", "/utf-8", "/MT"])
        .arg(format!(
            "/Fe{}",
            archive_out.join("BackleafShell.exe").display()
        ))
        .arg(format!(
            "/Fo{}",
            archive_out.join("shell-helper.obj").display()
        ))
        .arg("src/native/shell_helper.cpp")
        .args([
            "/link",
            "ole32.lib",
            "oleaut32.lib",
            "uuid.lib",
            "shell32.lib",
            "user32.lib",
        ])
        .status()
        .expect("Shell helper compile");
    assert!(shell.success(), "Native Shell helper compilation failed");
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
