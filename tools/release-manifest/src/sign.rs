//! Signing the update packages with the project's updater key, and checking
//! the signatures (SEC-7, M6-28b), in the format `tauri signer` writes and
//! both 1.0.0-rc.1's Tauri updater and the app's `OwnSource` read: a `.sig`
//! is base64 of a minisign signature box whose trusted comment is
//! `timestamp:<s>\tfile:<name>\tversion:<v>`. cargo-packager's signer leaves
//! out `version:` (M6-03), so it is not used.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::Error;

/// The project's updater public key, base64 of the minisign public key
/// file: the `pubkey` of 1.0.0-rc.1's `tauri.conf.json`. A copy of
/// `gitcroc_app::update::verify::PUBLIC_KEY`, which `gitcroc-app`'s tests
/// check it equals, since this tool cannot depend on the app.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdDOUE5NTE3NjI5QTdDOTkKUldTWmZKcGlGNVdhZkFycG4veXhKemFOZW1zVlJubDFnTzNRZTY5YnNReEJTWnFPWVNpU0hwc1oK";

/// The environment variable holding the secret key: base64 of a minisign
/// secret key box, as `tauri signer generate` writes it (M5-12).
pub const KEY_VAR: &str = "TAURI_SIGNING_PRIVATE_KEY";

/// The environment variable holding the secret key's password; unset is
/// the empty password.
pub const PASSWORD_VAR: &str = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD";

/// The untrusted comment `tauri signer` writes.
const UNTRUSTED_COMMENT: &str = "signature from tauri secret key";

/// The secret key in `key`, base64 of a minisign secret key box, decrypted
/// with `password`.
pub fn secret_key(key: &str, password: &str) -> Result<minisign::SecretKey, Error> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::NoKey);
    }
    let text =
        base64_text(key).map_err(|reason| Error::BadKey(format!("the secret key {reason}")))?;
    let key_box = minisign::SecretKeyBox::from_string(&text)
        .map_err(|error| Error::BadKey(format!("the secret key is not valid: {error}")))?;
    // Always Some: None would ask for the password on the terminal.
    key_box
        .into_secret_key(Some(password.to_owned()))
        .map_err(|error| Error::BadKey(format!("the secret key does not open: {error}")))
}

/// A new key pair for a test, such as M6-28c's update test, never the
/// project's: the secret half as [`KEY_VAR`] takes it, encrypted with the
/// empty password, and the public half, both base64 of their boxes.
pub fn keygen() -> Result<(String, String), Error> {
    let failed = |error: minisign::PError| Error::Sign(format!("no key was made: {error}"));
    let pair =
        minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).map_err(failed)?;
    let secret = pair.sk.to_box(None).map_err(failed)?.into_string();
    let public = pair.pk.to_box().map_err(failed)?.into_string();
    Ok((STANDARD.encode(secret), STANDARD.encode(public)))
}

/// Writes [`keygen`]'s secret half to `path`, which must not exist yet, and
/// returns the public half.
pub fn keygen_file(path: &Path) -> Result<String, Error> {
    let (secret, public) = keygen()?;
    let io = |source| Error::Io {
        path: path.to_owned(),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    std::io::Write::write_all(&mut file, secret.as_bytes()).map_err(io)?;
    Ok(public)
}

/// The trusted comment of a signature of `file` for `version`, made at
/// `timestamp` seconds since the epoch.
pub fn trusted_comment(timestamp: u64, file: &str, version: &str) -> String {
    format!("timestamp:{timestamp}\tfile:{file}\tversion:{version}")
}

/// A `.sig`'s content: base64 of a minisign signature of `package`, named
/// `file`, for `version`.
pub fn signature(
    key: &minisign::SecretKey,
    package: impl std::io::Read,
    file: &str,
    version: &str,
) -> Result<String, Error> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let trusted = trusted_comment(timestamp, file, version);
    let signature = minisign::sign(None, key, package, Some(&trusted), Some(UNTRUSTED_COMMENT))
        .map_err(|error| Error::Sign(format!("{file}: {error}")))?;
    Ok(STANDARD.encode(signature.into_string()))
}

/// Signs the file at `path` for `version` and writes `<path>.sig`, which it
/// returns.
pub fn sign_file(key: &minisign::SecretKey, path: &Path, version: &str) -> Result<PathBuf, Error> {
    crate::check_version(version)?;
    let package = fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    let signature = signature(key, package, &file_name(path), version)?;
    let sig = sig_path(path);
    fs::write(&sig, signature).map_err(|source| Error::Io {
        path: sig.clone(),
        source,
    })?;
    Ok(sig)
}

/// Checks that `signature`, a `.sig`'s content, signs `package` with
/// `public_key`, base64 of a minisign public key file, and names `version`,
/// as `OwnSource` checks it: with `minisign-verify`, then the trusted
/// comment's `version:`.
pub fn verify(
    package: &[u8],
    signature: &str,
    version: &str,
    public_key: &str,
) -> Result<(), String> {
    let key_text = base64_text(public_key).map_err(|reason| format!("the public key {reason}"))?;
    let key = minisign_verify::PublicKey::decode(&key_text)
        .map_err(|error| format!("the public key is not valid: {error}"))?;
    let text = base64_text(signature).map_err(|reason| format!("the signature {reason}"))?;
    let signature = minisign_verify::Signature::decode(&text)
        .map_err(|error| format!("the signature is not valid: {error}"))?;
    key.verify(package, &signature, true)
        .map_err(|error| format!("the signature does not match: {error}"))?;
    let signed = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .ok_or_else(|| "the signature names no version".to_owned())?;
    if signed.trim_start_matches('v') == version.trim_start_matches('v') {
        Ok(())
    } else {
        Err(format!("it was signed for version {signed}, not {version}"))
    }
}

/// Checks the file at `path` against `<path>.sig`, as [`verify`] does.
pub fn verify_file(path: &Path, version: &str, public_key: &str) -> Result<(), Error> {
    let package = fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    let sig = sig_path(path);
    let signature = match fs::read_to_string(&sig) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NoSignature(file_name(path)));
        }
        Err(source) => return Err(Error::Io { path: sig, source }),
    };
    verify(&package, &signature, version, public_key).map_err(|reason| Error::BadSignature {
        file: file_name(path),
        reason,
    })
}

fn sig_path(path: &Path) -> PathBuf {
    let mut sig = path.as_os_str().to_owned();
    sig.push(".sig");
    PathBuf::from(sig)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn base64_text(encoded: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("is not base64: {error}"))?;
    String::from_utf8(bytes).map_err(|_| "is not text".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key pair as `tauri signer generate` makes it: an encrypted
    /// minisign key pair, each half written as base64 of its box.
    fn tauri_keys(password: &str) -> (String, String) {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(password.to_owned())).unwrap();
        let secret = STANDARD.encode(pair.sk.to_box(None).unwrap().into_string());
        let public = STANDARD.encode(pair.pk.to_box().unwrap().into_string());
        (secret, public)
    }

    #[test]
    fn sec_7_sign_reads_tauris_secret_key_format() {
        let (secret, public) = tauri_keys("correct horse");
        let key = secret_key(&secret, "correct horse").unwrap();
        let sig = signature(
            &key,
            &b"package"[..],
            "gitcroc_1.0.0_x64-setup.exe",
            "1.0.0",
        )
        .unwrap();
        verify(b"package", &sig, "1.0.0", &public).unwrap();

        let wrong = secret_key(&secret, "wrong horse").unwrap_err();
        assert!(matches!(wrong, Error::BadKey(_)), "{wrong:?}");
        let none = secret_key(&secret, "").unwrap_err();
        assert!(matches!(none, Error::BadKey(_)), "{none:?}");
        assert!(matches!(secret_key("", "x"), Err(Error::NoKey)));
        assert!(matches!(
            secret_key("not base64!", ""),
            Err(Error::BadKey(_))
        ));
    }

    /// `tauri signer generate --ci` encrypts with the empty password.
    #[test]
    fn sec_7_sign_reads_a_key_without_password() {
        let (secret, public) = tauri_keys("");
        let key = secret_key(&secret, "").unwrap();
        let sig = signature(&key, &b"package"[..], "gitcroc.app.tar.gz", "1.0.0-rc.2").unwrap();
        verify(b"package", &sig, "1.0.0-rc.2", &public).unwrap();
    }

    #[test]
    fn sec_7_signature_is_tauris_format() {
        let (secret, _) = tauri_keys("");
        let key = secret_key(&secret, "").unwrap();
        let sig = signature(&key, &b"package"[..], "gitcroc.app.tar.gz", "1.0.0").unwrap();
        let text = base64_text(&sig).unwrap();
        let decoded = minisign_verify::Signature::decode(&text).unwrap();
        assert_eq!(
            decoded.untrusted_comment(),
            "untrusted comment: signature from tauri secret key"
        );
        let fields: Vec<&str> = decoded.trusted_comment().split('\t').collect();
        assert_eq!(fields.len(), 3, "{fields:?}");
        let timestamp = fields[0].strip_prefix("timestamp:").unwrap();
        assert!(timestamp.parse::<u64>().unwrap() > 1_700_000_000);
        assert_eq!(&fields[1..], ["file:gitcroc.app.tar.gz", "version:1.0.0"]);
    }

    #[test]
    fn sec_7_verify_refuses_another_version_file_or_key() {
        let (secret, public) = tauri_keys("");
        let (_, other_public) = tauri_keys("");
        let key = secret_key(&secret, "").unwrap();
        let sig = signature(&key, &b"package"[..], "gitcroc.AppImage", "1.0.0").unwrap();
        verify(b"package", &sig, "1.0.0", &public).unwrap();
        verify(b"package", &sig, "v1.0.0", &public).unwrap();
        assert!(verify(b"package", &sig, "1.0.1", &public).is_err());
        assert!(verify(b"packagf", &sig, "1.0.0", &public).is_err());
        assert!(verify(b"package", &sig, "1.0.0", &other_public).is_err());
        assert!(verify(b"package", "garbage", "1.0.0", &public).is_err());
    }

    #[test]
    fn sec_7_sign_file_writes_the_sig_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let (secret, public) = tauri_keys("");
        let key = secret_key(&secret, "").unwrap();
        let package = dir.path().join("gitcroc_1.0.0_amd64.AppImage");
        fs::write(&package, b"\x7fELF").unwrap();
        let sig = sign_file(&key, &package, "1.0.0").unwrap();
        assert_eq!(sig, dir.path().join("gitcroc_1.0.0_amd64.AppImage.sig"));
        verify_file(&package, "1.0.0", &public).unwrap();
        assert!(matches!(
            verify_file(&package, "1.0.1", &public),
            Err(Error::BadSignature { .. })
        ));
        fs::remove_file(&sig).unwrap();
        assert!(matches!(
            verify_file(&package, "1.0.0", &public),
            Err(Error::NoSignature(_))
        ));
        assert!(matches!(
            sign_file(&key, &dir.path().join("missing"), "1.0.0"),
            Err(Error::Io { .. })
        ));
    }

    #[test]
    fn sec_7_keygen_signs_with_the_empty_password() {
        let (secret, public) = keygen().unwrap();
        let key = secret_key(&secret, "").unwrap();
        let sig = signature(&key, &b"package"[..], "gitcroc.AppImage", "1.0.1").unwrap();
        verify(b"package", &sig, "1.0.1", &public).unwrap();
        assert!(verify(b"package", &sig, "1.0.1", PUBLIC_KEY).is_err());
        assert_ne!(keygen().unwrap().1, public);
    }

    #[test]
    fn sec_7_the_public_key_is_a_minisign_key() {
        let text = base64_text(PUBLIC_KEY).unwrap();
        minisign_verify::PublicKey::decode(&text).unwrap();
    }
}
