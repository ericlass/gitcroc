//! `gitcroc-cli ai providers`: the AI providers of `settings.toml` as JSON,
//! for tests and the prompt eval (M4-05, M4-11). A key is read only from
//! the environment variable `--key-env` names, never from an argument or
//! the keychain, and is never printed (SEC-1). `ai eval` is in [`crate::eval`].

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcroc_core::{AiSettings, PlatformConfigDir, SettingsStore};

use crate::workspace::GivenConfigDir;

pub const PROVIDERS_USAGE: &str = "ai providers [--config-dir <dir>] [--key-env <NAME>]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The platform config dir when `None`.
    pub config_dir: Option<PathBuf>,
    /// The environment variable holding the key.
    pub key_env: Option<String>,
}

/// Parses the arguments after `ai`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let [command, rest @ ..] = args else {
        return None;
    };
    if command != "providers" {
        return None;
    }
    let mut options = Options {
        config_dir: None,
        key_env: None,
    };
    let mut rest = rest.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next()?;
        match flag.to_str()? {
            "--config-dir" if options.config_dir.is_none() => {
                options.config_dir = Some(PathBuf::from(value));
            }
            "--key-env" if options.key_env.is_none() => {
                let name = value.to_str()?;
                is_variable_name(name).then_some(())?;
                options.key_env = Some(name.to_owned());
            }
            _ => return None,
        }
    }
    Some(options)
}

/// A portable environment variable name: letters, digits and `_`, not
/// starting with a digit. Anything else may be a key typed by mistake.
pub(crate) fn is_variable_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Prints the AI settings and providers. An API provider's `has_key` says
/// whether the named variable holds a key; a CLI provider has none.
pub fn run(options: &Options) -> ExitCode {
    let settings = match &options.config_dir {
        Some(dir) => SettingsStore::load(&GivenConfigDir(dir.clone())),
        None => SettingsStore::load(&PlatformConfigDir),
    };
    if let Some(warning) = settings.view().warning {
        eprintln!("warning: the settings are read-only: {warning:?}");
    }
    let has_key = options
        .key_env
        .as_ref()
        .and_then(std::env::var_os)
        .is_some_and(|key| !key.is_empty());
    match listing(settings.ai(), has_key).and_then(|json| serde_json::to_string_pretty(&json)) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write the providers as JSON: {error}");
            ExitCode::from(2)
        }
    }
}

fn listing(ai: AiSettings, has_key: bool) -> Result<serde_json::Value, serde_json::Error> {
    let providers = ai
        .providers
        .iter()
        .map(|entry| {
            let mut value = serde_json::to_value(entry)?;
            if let Some(fields) = value.as_object_mut() {
                fields.insert(
                    "has_key".to_owned(),
                    (has_key && !entry.kind.is_cli()).into(),
                );
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    Ok(serde_json::json!({
        "enabled": ai.enabled,
        "default": ai.default,
        "exclude": ai.exclude,
        "debug_log": ai.debug_log,
        "providers": providers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn ai_providers_arguments() {
        assert_eq!(
            parse_args(&args(&[
                "providers",
                "--key-env",
                "MY_KEY",
                "--config-dir",
                "c"
            ])),
            Some(Options {
                config_dir: Some(PathBuf::from("c")),
                key_env: Some("MY_KEY".to_owned()),
            })
        );
        assert_eq!(
            parse_args(&args(&["providers"])),
            Some(Options {
                config_dir: None,
                key_env: None,
            })
        );
        for bad in [
            &[][..],
            &["models"],
            &["providers", "--key-env"],
            &["providers", "--key-env", "1KEY"],
            &["providers", "--key-env", "sk-live-abc"],
            &["providers", "--key-env", "A", "--key-env", "B"],
            &["providers", "--key", "sk"],
        ] {
            assert_eq!(parse_args(&args(bad)), None, "{bad:?}");
        }
    }
}
