//! Signs update packages with a throwaway key, as Tauri's bundler does.
//!
//!   cargo run --example sign_update -- <out-dir> <version> <file>...
//!
//! Writes `<file>.sig` next to each file, and `updater.pub` (the base64
//! public key, as `plugins.updater.pubkey` holds it) and `latest.json` into
//! `<out-dir>`. The secret key never touches the disk.

use std::path::PathBuf;

use gpui_ci_spike::update;

/// The `latest.json` platforms a package serves, as tools/release-manifest
/// names them; the macOS archive holds a universal binary.
fn platforms(name: &str) -> &'static [&'static str] {
    if name.ends_with(".exe") {
        &["windows-x86_64"]
    } else if name.ends_with(".app.tar.gz") {
        &["darwin-x86_64", "darwin-aarch64"]
    } else {
        &["linux-x86_64"]
    }
}

fn main() -> update::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(
        args.next()
            .ok_or("usage: sign_update <out-dir> <version> <file>...")?,
    );
    let version = args.next().ok_or("no version")?;
    let files: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if files.is_empty() {
        return Err("no files to sign".into());
    }
    let (secret, public) = update::generate()?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("updater.pub"), &public).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for file in &files {
        let sig = update::sign(&secret, file, &version)?;
        let signature = std::fs::read_to_string(&sig).map_err(|e| e.to_string())?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        println!("signed {}", file.display());
        for platform in platforms(&name) {
            entries.push(format!(
                "    \"{platform}\": {{\"signature\": \"{signature}\", \"url\": \"https://github.com/ericlass/gitcroc/releases/download/v{version}/{name}\"}}"
            ));
        }
    }
    let manifest = format!(
        "{{\n  \"version\": \"{version}\",\n  \"notes\": \"spike\",\n  \"pub_date\": \"2026-10-26T00:00:00Z\",\n  \"platforms\": {{\n{}\n  }}\n}}\n",
        entries.join(",\n")
    );
    std::fs::write(out.join("latest.json"), manifest).map_err(|e| e.to_string())?;
    Ok(())
}
