//! The [`Keychain`] implementations: the OS keychain through the `keyring`
//! crate, the same code on every platform, and one in memory for the tests
//! and the `e2e` build.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use super::Keychain;

/// The service of every entry gitcroc stores.
pub const KEYCHAIN_SERVICE: &str = "gitcroc";

/// The account an AI provider's key is stored under.
pub fn keychain_account(provider_id: &str) -> String {
    format!("ai-provider-{provider_id}")
}

/// Why the keychain could not be used. It never holds a secret (SEC-1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the keychain cannot be used: {0}")]
pub struct KeychainError(pub String);

/// Keychain Services on macOS, the Credential Manager on Windows, the
/// Secret Service on Linux. Without one, every call fails; nothing is ever
/// written to a file instead.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsKeychain;

impl OsKeychain {
    fn entry(account: &str) -> Result<keyring::Entry, KeychainError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|error| failed(&error))
    }
}

/// The error's text, without the data some variants carry.
fn failed(error: &keyring::Error) -> KeychainError {
    KeychainError(match error {
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            "the stored key is not text".to_owned()
        }
        keyring::Error::NoDefaultStore => "no keychain was found".to_owned(),
        other => other.to_string(),
    })
}

impl Keychain for OsKeychain {
    fn read(&self, account: &str) -> Result<Option<String>, KeychainError> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(failed(&error)),
        }
    }

    fn write(&self, account: &str, secret: &str) -> Result<(), KeychainError> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|error| failed(&error))
    }

    fn remove(&self, account: &str) -> Result<(), KeychainError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(failed(&error)),
        }
    }
}

/// A keychain in memory. [`MemoryKeychain::unreachable`] makes one that
/// fails every call, as a machine without a keychain does.
#[derive(Debug, Default)]
pub struct MemoryKeychain {
    secrets: Mutex<HashMap<String, String>>,
    unreachable: bool,
}

impl MemoryKeychain {
    pub fn unreachable() -> Self {
        Self {
            unreachable: true,
            ..Self::default()
        }
    }

    /// Every account and its secret.
    pub fn entries(&self) -> HashMap<String, String> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        // Each call changes the map in one step, so a panic cannot leave it
        // half-changed.
        self.secrets.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reach(&self) -> Result<(), KeychainError> {
        if self.unreachable {
            Err(KeychainError("no keychain was found".to_owned()))
        } else {
            Ok(())
        }
    }
}

impl Keychain for MemoryKeychain {
    fn read(&self, account: &str) -> Result<Option<String>, KeychainError> {
        self.reach()?;
        Ok(self.lock().get(account).cloned())
    }

    fn write(&self, account: &str, secret: &str) -> Result<(), KeychainError> {
        self.reach()?;
        self.lock().insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn remove(&self, account: &str) -> Result<(), KeychainError> {
        self.reach()?;
        self.lock().remove(account);
        Ok(())
    }
}
