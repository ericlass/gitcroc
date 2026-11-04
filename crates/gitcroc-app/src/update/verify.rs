//! Checking an update package before anything is done with it (SEC-7), as
//! Tauri's updater 2.14 does: the `.sig` is base64 of a minisign signature,
//! the key is base64 of a minisign public key file, and the version the
//! package was signed for must be the one the manifest offers.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

/// The project's updater key: the `pubkey` of 1.0.0-rc.1's
/// `tauri.conf.json`, so that release's packages and the ones after it are
/// signed with the same secret key.
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdDOUE5NTE3NjI5QTdDOTkKUldTWmZKcGlGNVdhZkFycG4veXhKemFOZW1zVlJubDFnTzNRZTY5YnNReEJTWnFPWVNpU0hwc1oK";

/// The minisign public key in `public_key`, base64 of the key file.
pub fn public_key(public_key: &str) -> Result<PublicKey, String> {
    let text = base64_text(public_key).map_err(|error| format!("the updater key {error}"))?;
    PublicKey::decode(&text).map_err(|error| format!("the updater key is not valid: {error}"))
}

/// Checks that `signature`, a `.sig`'s content, signs `package` with
/// `key`, and that it was signed for `version`.
pub fn verify(package: &[u8], signature: &str, key: &str, version: &str) -> Result<(), String> {
    let key = public_key(key)?;
    let text = base64_text(signature).map_err(|error| format!("the signature {error}"))?;
    let signature =
        Signature::decode(&text).map_err(|error| format!("the signature is not valid: {error}"))?;
    // Legacy signatures are accepted, as Tauri's updater accepts them.
    key.verify(package, &signature, true)
        .map_err(|error| format!("the update's signature does not match: {error}"))?;
    // The trusted comment is only trusted once the signature checked out.
    signed_for(signature.trusted_comment(), version)
}

/// Checks that the trusted comment, `key:value` pairs separated by tabs as
/// `tauri signer` writes them, names `version`, so an older signed package
/// cannot be offered as a newer version.
pub fn signed_for(trusted_comment: &str, version: &str) -> Result<(), String> {
    let Some(signed) = trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
    else {
        return Err("the update's signature names no version".to_owned());
    };
    let same = match (
        semver::Version::parse(signed.trim_start_matches('v')),
        semver::Version::parse(version.trim_start_matches('v')),
    ) {
        (Ok(signed), Ok(offered)) => signed == offered,
        _ => signed == version,
    };
    if same {
        Ok(())
    } else {
        Err(format!(
            "the update was signed for version {signed}, not {version}"
        ))
    }
}

fn base64_text(encoded: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|error| format!("is not base64: {error}"))?;
    String::from_utf8(bytes).map_err(|_| "is not text".to_owned())
}
