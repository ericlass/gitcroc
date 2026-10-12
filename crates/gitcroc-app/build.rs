use std::error::Error;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Tauri links comctl32 v6, which Windows only loads for an executable
    // whose manifest asks for it. tauri-build embeds that manifest in the
    // `gitcroc` binary alone, so test and example binaries that link Tauri
    // die with STATUS_ENTRYPOINT_NOT_FOUND. Embed it in every binary instead.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV")?;
    let mut windows = tauri_build::WindowsAttributes::new();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;
    Ok(())
}
