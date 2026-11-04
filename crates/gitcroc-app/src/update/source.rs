//! [`OwnSource`], the app's [`UpdateSource`] without Tauri (PLAT-4): one
//! `GET` of the manifest, then, to install, the package downloaded into
//! memory, verified with the project's key (SEC-7), and only then
//! installed. Nothing is written before the package is verified.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use gitcroc_core::{AppInfo, Package, UpdateFound, UpdateFuture, UpdateProgress, UpdateSource};

use super::install::Install;
use super::manifest::{self, Offer};
use super::verify;
use crate::platform::{Installation, PlatformInstallation};

/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the server may send nothing.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the manifest may take, from connecting to its last byte.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(60);
/// The largest manifest read.
const MANIFEST_LIMIT: usize = 1024 * 1024;
/// The largest package downloaded.
const PACKAGE_LIMIT: u64 = 1024 * 1024 * 1024;
/// How many redirects are followed, as reqwest's default.
const REDIRECTS: usize = 10;

/// Where the manifest is and which key verifies the packages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) url: String,
    pub(crate) public_key: String,
    /// Plain `http` is allowed from a loopback address: M6-28's update test
    /// and the tests' server.
    pub(crate) loopback_http: bool,
}

impl Endpoint {
    /// [`manifest::ENDPOINT`] and [`verify::PUBLIC_KEY`].
    pub(crate) fn release() -> Self {
        Self {
            url: manifest::ENDPOINT.to_owned(),
            public_key: verify::PUBLIC_KEY.to_owned(),
            loopback_http: false,
        }
    }

    /// This build's: the release endpoint and key, fixed (SEC-7).
    #[cfg(not(feature = "update-test"))]
    pub(crate) fn built_in() -> Self {
        Self::release()
    }

    /// In a build with the `update-test` feature, the endpoint and key that
    /// `GITCROC_UPDATE_TEST_ENDPOINT` and `GITCROC_UPDATE_TEST_KEY` named at
    /// build time, for M6-28's update test; the release ones without them.
    #[cfg(feature = "update-test")]
    pub(crate) fn built_in() -> Self {
        match (
            option_env!("GITCROC_UPDATE_TEST_ENDPOINT"),
            option_env!("GITCROC_UPDATE_TEST_KEY"),
        ) {
            (Some(url), Some(public_key)) => Self {
                url: url.to_owned(),
                public_key: public_key.to_owned(),
                loopback_http: true,
            },
            _ => Self::release(),
        }
    }
}

/// How the update is installed: where the running app came from, or, in
/// tests, an AppImage at a given path, replaced on every platform with the
/// step the Linux build uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Running(Package),
    #[cfg(test)]
    GivenAppImage(std::path::PathBuf),
}

/// The app's update source.
#[derive(Debug)]
pub struct OwnSource {
    endpoint: Endpoint,
    current_version: String,
    /// The manifest's key for this platform; `None` where nothing updates
    /// itself.
    platform: Option<&'static str>,
    target: Target,
    /// What the last check found.
    offer: Mutex<Option<Offer>>,
}

impl OwnSource {
    /// The running build's source: its version, platform and package, and
    /// the endpoint and key built in.
    pub fn new() -> Self {
        Self::with(
            Endpoint::built_in(),
            AppInfo::current().version,
            manifest::this_platform(),
            Target::Running(PlatformInstallation::package()),
        )
    }

    pub(crate) fn with(
        endpoint: Endpoint,
        current_version: String,
        platform: Option<&'static str>,
        target: Target,
    ) -> Self {
        Self {
            endpoint,
            current_version,
            platform,
            target,
            offer: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn client(&self) -> Result<reqwest::Client, String> {
        let loopback_http = self.endpoint.loopback_http;
        // A redirect may not leave https either.
        let redirects = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= REDIRECTS {
                attempt.error("too many redirects")
            } else if manifest::checked_url(attempt.url().as_str(), loopback_http).is_ok() {
                attempt.follow()
            } else {
                attempt.error("a redirect left https")
            }
        });
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .redirect(redirects)
            .build()
            .map_err(|error| chain(&error))
    }

    async fn check_manifest(&self) -> Result<Option<Offer>, String> {
        let url = manifest::checked_url(&self.endpoint.url, self.endpoint.loopback_http)?;
        let mut response = self
            .client()?
            .get(url)
            .timeout(MANIFEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| chain(&error))?;
        if !response.status().is_success() {
            return Err(format!(
                "the update manifest could not be read: {}",
                response.status()
            ));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| chain(&error))? {
            if body.len() + chunk.len() > MANIFEST_LIMIT {
                return Err("the update manifest is too large".to_owned());
            }
            body.extend_from_slice(&chunk);
        }
        manifest::offer(manifest::read(&body)?, &self.current_version, self.platform)
    }

    /// The package at `url`, whole, telling `progress` of each chunk.
    async fn download(&self, url: &str, progress: &UpdateProgress) -> Result<Vec<u8>, String> {
        let url = manifest::checked_url(url, self.endpoint.loopback_http)?;
        let mut response = self
            .client()?
            .get(url)
            .send()
            .await
            .map_err(|error| chain(&error))?;
        if !response.status().is_success() {
            return Err(format!(
                "the update could not be downloaded: {}",
                response.status()
            ));
        }
        let total = response.content_length();
        if total.is_some_and(|total| total > PACKAGE_LIMIT) {
            return Err("the update is too large".to_owned());
        }
        let mut package = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| chain(&error))? {
            let length = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            let so_far = u64::try_from(package.len()).unwrap_or(u64::MAX);
            if so_far.saturating_add(length) > PACKAGE_LIMIT {
                return Err("the update is too large".to_owned());
            }
            package.extend_from_slice(&chunk);
            progress(length, total);
        }
        Ok(package)
    }

    async fn download_and_install(&self, progress: UpdateProgress) -> Result<(), String> {
        let offer = lock(&self.offer)
            .clone()
            .ok_or_else(|| "no update was found".to_owned())?;
        let install = match &self.target {
            Target::Running(package) => Some(Install::running(*package)?),
            #[cfg(test)]
            Target::GivenAppImage(_) => None,
        };
        let package = self.download(&offer.url, &progress).await?;
        // Nothing is written before this.
        verify::verify(
            &package,
            &offer.signature,
            &self.endpoint.public_key,
            &offer.version,
        )?;
        let target = self.target.clone();
        tokio::task::spawn_blocking(move || match (install, target) {
            (Some(install), _) => put_in_place(&install, &package),
            #[cfg(test)]
            (None, Target::GivenAppImage(file)) => {
                super::install::replace_appimage(&package, &file)
            }
            (None, _) => Err("the update has nowhere to go".to_owned()),
        })
        .await
        .map_err(|error| format!("the update was not installed: {error}"))?
    }
}

impl Default for OwnSource {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateSource for OwnSource {
    fn check(&self) -> UpdateFuture<'_, Option<UpdateFound>> {
        Box::pin(async move {
            let offer = self.check_manifest().await;
            let mut kept = lock(&self.offer);
            *kept = offer.as_ref().ok().cloned().flatten();
            offer.map(|offer| {
                offer.map(|offer| UpdateFound {
                    version: offer.version,
                    date: Some(offer.date),
                    notes: offer.notes,
                })
            })
        })
    }

    fn install(&self, progress: UpdateProgress) -> UpdateFuture<'_, ()> {
        Box::pin(self.download_and_install(progress))
    }
}

/// Installs the verified `package`. The Windows installer replaces the app
/// itself and restarts it, so the app exits once it has started.
fn put_in_place(install: &Install, package: &[u8]) -> Result<(), String> {
    PlatformInstallation::install_update(install, package)?;
    if *install == Install::WindowsInstaller {
        std::process::exit(0)
    }
    Ok(())
}

/// An error and its sources, joined, since reqwest's own message rarely
/// says why ("error sending request").
fn chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// The offer is set whole, so a panic leaves none half-set.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
