//! The release manifest, `latest.json`, as `tools/release-manifest` writes it
//! and 1.0.0-rc.1's Tauri updater reads it (PLAT-4): the newest version, its
//! notes and date, and for each platform that updates itself the package's
//! address and signature.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::Deserialize;

/// Where the manifest is: the latest release's `latest.json`, as in
/// 1.0.0-rc.1's `tauri.conf.json`.
pub const ENDPOINT: &str =
    "https://github.com/ericlass/gitcroc/releases/latest/download/latest.json";

/// `latest.json`. Every field is required.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub platforms: BTreeMap<String, Platform>,
}

/// One platform's package.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Platform {
    /// The package's address; only `https` is accepted ([`checked_url`]).
    pub url: String,
    /// The content of the package's `.sig`: base64 of a minisign signature.
    pub signature: String,
}

/// A newer version on offer for this platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub notes: String,
    pub date: String,
    pub url: String,
    pub signature: String,
}

/// Reads `latest.json`.
pub fn read(json: &[u8]) -> Result<Manifest, String> {
    serde_json::from_slice(json)
        .map_err(|error| format!("the update manifest could not be read: {error}"))
}

/// The manifest's key for an OS and architecture, as
/// `std::env::consts` names them; `None` where no package updates itself.
pub fn platform_key(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "x86_64") => Some("darwin-x86_64"),
        ("macos", "aarch64") => Some("darwin-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

/// The running build's key.
pub fn this_platform() -> Option<&'static str> {
    platform_key(std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether `offered` is a newer semver than `current`, so 1.0.0-rc.1 <
/// 1.0.0-rc.2 < 1.0.0.
pub fn is_newer(current: &str, offered: &str) -> Result<bool, String> {
    let parse = |version: &str| {
        semver::Version::parse(version)
            .map_err(|error| format!("{version:?} is not a version: {error}"))
    };
    Ok(parse(offered)? > parse(current)?)
}

/// What the manifest offers the build at `current` on `platform`: `None`
/// when its version is not newer, or it has no package for the platform.
pub fn offer(
    manifest: Manifest,
    current: &str,
    platform: Option<&str>,
) -> Result<Option<Offer>, String> {
    if !is_newer(current, &manifest.version)? {
        return Ok(None);
    }
    let Some(package) = platform.and_then(|key| manifest.platforms.get(key)) else {
        return Ok(None);
    };
    Ok(Some(Offer {
        url: package.url.clone(),
        signature: package.signature.clone(),
        version: manifest.version,
        notes: manifest.notes,
        date: manifest.pub_date,
    }))
}

/// `url`, if it is `https`. M6-28's update test also serves plain `http`
/// from a loopback address, which `loopback_http` allows.
pub fn checked_url(url: &str, loopback_http: bool) -> Result<reqwest::Url, String> {
    let parsed =
        reqwest::Url::parse(url).map_err(|error| format!("{url:?} is not an address: {error}"))?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if loopback_http && is_loopback(&parsed) => Ok(parsed),
        _ => Err(format!("the update's address is not https: {url}")),
    }
}

fn is_loopback(url: &reqwest::Url) -> bool {
    match url.host_str() {
        Some("localhost") => true,
        Some(host) => host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback()),
        None => false,
    }
}
