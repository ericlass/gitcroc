//! Update package signatures in the format of Tauri's bundler, without it
//! (PLAT-4): a `.sig` file is the base64 of a minisign signature box whose
//! trusted comment is `timestamp:<s>\tfile:<name>\tversion:<v>`, and the
//! public key in the updater config is the base64 of the public key box.
//! cargo-packager's own signer writes the same box but leaves out `version:`.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

pub type Result<T> = std::result::Result<T, String>;

/// A throwaway key pair, without a password: (secret key box, public key as
/// the updater config holds it).
pub fn generate() -> Result<(String, String)> {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
    let secret = pair.sk.to_box(None).map_err(|e| e.to_string())?.to_string();
    let public = pair.pk.to_box().map_err(|e| e.to_string())?.to_string();
    Ok((secret, STANDARD.encode(public)))
}

/// Signs `path` and writes `<path>.sig`, returning its path.
pub fn sign(secret_box: &str, path: &Path, version: &str) -> Result<PathBuf> {
    let secret = minisign::SecretKeyBox::from_string(secret_box)
        .and_then(|sk| sk.into_unencrypted_secret_key())
        .map_err(|e| e.to_string())?;
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let trusted = format!("timestamp:{timestamp}\tfile:{name}\tversion:{version}");
    let signature = minisign::sign(
        None,
        &secret,
        Cursor::new(data),
        Some(&trusted),
        Some("signature from gitcroc spike"),
    )
    .map_err(|e| e.to_string())?;
    let mut sig_path = path.as_os_str().to_owned();
    sig_path.push(".sig");
    let sig_path = PathBuf::from(sig_path);
    std::fs::write(&sig_path, STANDARD.encode(signature.to_string())).map_err(|e| e.to_string())?;
    Ok(sig_path)
}
