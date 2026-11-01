//! Embed Windows VERSIONINFO + application icon into the PE.
//!
//! - On Windows hosts (MSVC CI): uses `winresource` + `rc.exe`.
//! - On Linux cross (cargo-zigbuild gnu): compiles a `.rc` with MinGW `windres`
//!   (via `scripts/windres-wine.sh` or `$WINDRES`) and links the `.o` with
//!   `cargo:rustc-link-arg=` so zigbuild accepts it (it rejects winresource's `.a`).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let icon = manifest_dir.join("assets/ondalivre.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let target = env::var("TARGET").unwrap_or_default();
    let host = env::var("HOST").unwrap_or_default();
    let is_msvc = target.contains("msvc");
    let cross = host != target;

    if is_msvc && !cross {
        // Native Windows MSVC (GitHub windows-latest): winresource + rc.exe
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().unwrap());
        res.set("ProductName", "OndaLivre");
        res.set(
            "FileDescription",
            "OndaLivre - automacao de radio portatil (Fluent UI)",
        );
        res.set("CompanyName", "Rodrigo Miquilino");
        res.set("FileVersion", "1.0.1");
        res.set("ProductVersion", "1.0.1");
        res.set("OriginalFilename", "ondalivre.exe");
        res.set("InternalName", "ondalivre");
        res.set("LegalCopyright", "Copyright (c) Rodrigo Miquilino");
        res.set_version_info(winresource::VersionInfo::FILEVERSION, 0x0001_0000_0000_0000);
        res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, 0x0001_0000_0000_0000);
        res.compile().expect("winresource compile failed");
        return;
    }

    // GNU / cross: hand-roll .rc → windres → link .o
    let rc_path = out_dir.join("ondalivre.rc");
    let obj_path = out_dir.join("ondalivre_res.o");
    let icon_path = icon.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION 1,0,1,0
PRODUCTVERSION 1,0,1,0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "Rodrigo Miquilino"
      VALUE "FileDescription", "OndaLivre - automacao de radio portatil (Fluent UI)"
      VALUE "FileVersion", "1.0.1"
      VALUE "InternalName", "ondalivre"
      VALUE "LegalCopyright", "Copyright (c) Rodrigo Miquilino"
      VALUE "OriginalFilename", "ondalivre.exe"
      VALUE "ProductName", "OndaLivre"
      VALUE "ProductVersion", "1.0.1"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon_path
    );
    fs::write(&rc_path, rc).expect("write rc");

    let windres = resolve_windres(&manifest_dir);
    let status = Command::new(&windres)
        .args([
            "-i",
            rc_path.to_str().unwrap(),
            "-o",
            obj_path.to_str().unwrap(),
            "--target=pe-x86-64",
        ])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn windres ({windres}): {e}"));
    if !status.success() {
        panic!("windres failed with {status}");
    }
    println!("cargo:rustc-link-arg={}", obj_path.display());
}

fn resolve_windres(manifest_dir: &std::path::Path) -> String {
    if let Ok(p) = env::var("WINDRES") {
        return p;
    }
    let wrapper = manifest_dir.join("scripts/windres-wine.sh");
    if wrapper.exists() {
        return wrapper.display().to_string();
    }
    "windres".into()
}
