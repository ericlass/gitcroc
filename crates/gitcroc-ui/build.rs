//! GPUI links `libxkbcommon-x11` on Linux. Some machines have the runtime
//! library, `libxkbcommon-x11.so.0`, without the `-dev` package's
//! unversioned link; this script links against the runtime library there,
//! as the M6 spikes did. CI installs the `-dev` packages and does not need
//! it.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // A Linux build runs its build script on Linux, a Unix.
    #[cfg(unix)]
    link_xkbcommon_x11();
}

#[cfg(unix)]
fn link_xkbcommon_x11() {
    use std::path::{Path, PathBuf};

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let (Some(out), Ok(arch)) = (
        std::env::var_os("OUT_DIR"),
        std::env::var("CARGO_CFG_TARGET_ARCH"),
    ) else {
        return;
    };
    let dirs = [
        PathBuf::from(format!("/usr/lib/{arch}-linux-gnu")),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/usr/lib"),
    ];
    if dirs
        .iter()
        .any(|dir| dir.join("libxkbcommon-x11.so").exists())
    {
        return;
    }
    let Some(runtime) = dirs
        .iter()
        .map(|dir| dir.join("libxkbcommon-x11.so.0"))
        .find(|path| path.exists())
    else {
        return;
    };
    let out = Path::new(&out);
    let link = out.join("libxkbcommon-x11.so");
    if !link.exists() && std::os::unix::fs::symlink(&runtime, &link).is_err() {
        return;
    }
    println!("cargo::rustc-link-search=native={}", out.display());
}
