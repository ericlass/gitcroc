//! PLAT-4: update signatures made without Tauri's bundler verify the way
//! tauri-plugin-updater 2.14 verifies them, with minisign-verify.
//!
//! With `SPIKE_UPDATE_DIR` set (to `sign_update`'s out-dir), it verifies
//! the real packages listed in its `latest.json`; `SPIKE_UPDATE_FILES` names
//! them, separated by the platform's path separator.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use gpui_ci_spike::update;
use minisign_verify::{PublicKey, Signature};

/// As `verify_signature` and `signed_version` in tauri-plugin-updater.
fn verify_like_tauri(
    data: &[u8],
    release_signature: &str,
    pub_key: &str,
    version: &str,
) -> Result<(), String> {
    let decode = |text: &str| {
        String::from_utf8(STANDARD.decode(text.trim()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    };
    let public_key = PublicKey::decode(&decode(pub_key)?).map_err(|e| e.to_string())?;
    let signature = Signature::decode(&decode(release_signature)?).map_err(|e| e.to_string())?;
    public_key
        .verify(data, &signature, true)
        .map_err(|e| e.to_string())?;
    let signed = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .ok_or("no signed version")?;
    if signed == version {
        Ok(())
    } else {
        Err(format!("signed {signed}, announced {version}"))
    }
}

#[test]
fn plat_4_a_signed_package_verifies_and_a_changed_one_does_not() {
    let dir = std::env::temp_dir().join(format!("gpui-ci-spike-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gitcroc_1.0.0_amd64.AppImage");
    std::fs::write(&file, b"not really an AppImage").unwrap();

    let (secret, public) = update::generate().unwrap();
    let sig = update::sign(&secret, &file, "1.0.0").unwrap();
    let signature = std::fs::read_to_string(&sig).unwrap();

    verify_like_tauri(b"not really an AppImage", &signature, &public, "1.0.0").unwrap();
    assert!(verify_like_tauri(b"tampered", &signature, &public, "1.0.0").is_err());
    assert!(verify_like_tauri(b"not really an AppImage", &signature, &public, "1.0.1").is_err());
    let (_, other_key) = update::generate().unwrap();
    assert!(verify_like_tauri(b"not really an AppImage", &signature, &other_key, "1.0.0").is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn plat_4_the_built_packages_verify() {
    let (Some(dir), Some(files)) = (
        std::env::var_os("SPIKE_UPDATE_DIR"),
        std::env::var_os("SPIKE_UPDATE_FILES"),
    ) else {
        eprintln!("SPIKE_UPDATE_DIR and SPIKE_UPDATE_FILES not set; nothing to verify");
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let public = std::fs::read_to_string(dir.join("updater.pub")).unwrap();
    let manifest = std::fs::read_to_string(dir.join("latest.json")).unwrap();
    let version = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("\"version\": \""))
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_string();
    let mut checked = 0;
    for file in std::env::split_paths(&files) {
        let data = std::fs::read(&file).unwrap();
        let mut sig_path = file.as_os_str().to_owned();
        sig_path.push(".sig");
        let signature = std::fs::read_to_string(&sig_path).unwrap();
        assert!(
            manifest.contains(signature.trim()),
            "latest.json holds the signature of {}",
            file.display()
        );
        verify_like_tauri(&data, &signature, &public, &version)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        println!("verified {} ({} bytes)", file.display(), data.len());
        checked += 1;
    }
    assert!(checked > 0, "no packages listed");
}
