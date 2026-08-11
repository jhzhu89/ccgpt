use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt,
    fs::File,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use crate::models::ModelTargets;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub backend: Backend,
    pub debug_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Copilot,
    Direct {
        api_key: String,
        base_url: String,
        targets: ModelTargets,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let environment = process_environment()?;
        let home = dirs::home_dir().ok_or_else(|| ConfigError::new("home directory not found"))?;
        Self::load_from(&home.join(".ccgptrc"), environment)
    }

    pub fn load_from<I, K, V>(path: &Path, environment: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let rc = match read_rc(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(ConfigError::new(format!(
                    "failed to read {}: {error}",
                    path.display()
                )));
            }
        };
        Self::from_sources(&rc, environment)
    }

    pub fn from_sources<I, K, V>(rc: &str, environment: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut values = parse_rc(rc)?;
        values.extend(
            environment
                .into_iter()
                .map(|(name, value)| (name.into(), value.into())),
        );

        let port = value(&values, "CCGPT_PORT")
            .unwrap_or("8000")
            .parse::<u16>()
            .map_err(|_| ConfigError::new("CCGPT_PORT must be a number from 0 to 65535"))?;
        let api_key = owned_value(&values, "CCGPT_API_KEY");
        let base_url = owned_value(&values, "CCGPT_BASE_URL");
        let debug_file = owned_value(&values, "CCGPT_DEBUG_FILE").map(PathBuf::from);

        let backend = match api_key {
            None => Backend::Copilot,
            Some(api_key) => {
                let base_url = validate_base_url(base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
                let defaults = ModelTargets::default();
                Backend::Direct {
                    api_key,
                    base_url,
                    targets: ModelTargets {
                        high: owned_value(&values, "CCGPT_MODEL_HIGH").unwrap_or(defaults.high),
                        balanced: owned_value(&values, "CCGPT_MODEL_BALANCED")
                            .unwrap_or(defaults.balanced),
                        fast: owned_value(&values, "CCGPT_MODEL_FAST").unwrap_or(defaults.fast),
                    },
                }
            }
        };

        Ok(Self {
            port,
            backend,
            debug_file,
        })
    }
}

fn process_environment() -> Result<Vec<(String, String)>, ConfigError> {
    let mut values = Vec::new();
    for name in [
        "CCGPT_PORT",
        "CCGPT_API_KEY",
        "CCGPT_BASE_URL",
        "CCGPT_MODEL_HIGH",
        "CCGPT_MODEL_BALANCED",
        "CCGPT_MODEL_FAST",
        "CCGPT_DEBUG_FILE",
    ] {
        if let Some(value) = env::var_os(name) {
            let value = value
                .into_string()
                .map_err(|_| ConfigError::new(format!("{name} must be valid Unicode")))?;
            values.push((name.to_owned(), value));
        }
    }
    Ok(values)
}

fn read_rc(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn parse_rc(contents: &str) -> Result<HashMap<String, String>, ConfigError> {
    dotenvy::from_read_iter(contents.as_bytes())
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| ConfigError::new(format!("invalid .ccgptrc: {error}")))
}

fn value<'a>(values: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    values
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn owned_value(values: &HashMap<String, String>, name: &str) -> Option<String> {
    value(values, name).map(str::to_owned)
}

fn validate_base_url(value: &str) -> Result<String, ConfigError> {
    let value = value.trim().trim_end_matches('/');
    if value
        .split_once("://")
        .is_none_or(|(_, rest)| rest.starts_with('/'))
    {
        return Err(invalid_base_url(value));
    }
    let parsed = reqwest::Url::parse(value).map_err(|_| invalid_base_url(value))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid_base_url(value));
    }
    Ok(value.to_owned())
}

fn invalid_base_url(value: &str) -> ConfigError {
    ConfigError::new(format!("invalid CCGPT_BASE_URL: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_copilot_on_port_8000() {
        let config = Config::from_sources("", Vec::<(String, String)>::new()).unwrap();

        assert_eq!(config.port, 8000);
        assert_eq!(config.backend, Backend::Copilot);
        assert_eq!(config.debug_file, None);
    }

    #[test]
    fn ignores_generic_openai_environment_variables() {
        let config = Config::from_sources(
            "",
            [
                ("OPENAI_API_KEY", "generic"),
                ("OPENAI_BASE_URL", "https://other"),
            ],
        )
        .unwrap();

        assert_eq!(config.backend, Backend::Copilot);
    }

    #[test]
    fn loads_direct_backend_and_model_defaults() {
        let config =
            Config::from_sources("CCGPT_API_KEY=key", Vec::<(String, String)>::new()).unwrap();

        assert_eq!(
            config.backend,
            Backend::Direct {
                api_key: "key".to_owned(),
                base_url: DEFAULT_BASE_URL.to_owned(),
                targets: ModelTargets::default(),
            }
        );
    }

    #[test]
    fn process_environment_overrides_rc_values() {
        let config = Config::from_sources(
            "CCGPT_API_KEY=rc-key\nCCGPT_PORT=9000\nCCGPT_MODEL_HIGH=rc-sol\nCCGPT_DEBUG_FILE=rc.jsonl",
            [
                ("CCGPT_API_KEY", "env-key"),
                ("CCGPT_PORT", "7000"),
                ("CCGPT_MODEL_HIGH", "env-sol"),
                ("CCGPT_DEBUG_FILE", "env.jsonl"),
            ],
        )
        .unwrap();

        assert_eq!(config.port, 7000);
        let Backend::Direct {
            api_key, targets, ..
        } = config.backend
        else {
            panic!("expected direct backend");
        };
        assert_eq!(api_key, "env-key");
        assert_eq!(targets.high, "env-sol");
        assert_eq!(config.debug_file, Some(PathBuf::from("env.jsonl")));
    }

    #[test]
    fn supports_simple_export_and_quoted_rc_values() {
        let config = Config::from_sources(
            "export CCGPT_API_KEY='key'\nCCGPT_BASE_URL=\"http://127.0.0.1:8080/v1/\"",
            Vec::<(String, String)>::new(),
        )
        .unwrap();

        let Backend::Direct {
            api_key, base_url, ..
        } = config.backend
        else {
            panic!("expected direct backend");
        };
        assert_eq!(api_key, "key");
        assert_eq!(base_url, "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn base_url_without_api_key_still_uses_copilot() {
        let config = Config::from_sources(
            "CCGPT_BASE_URL=https://example.com/v1",
            Vec::<(String, String)>::new(),
        )
        .unwrap();

        assert_eq!(config.backend, Backend::Copilot);
    }

    #[test]
    fn rejects_invalid_base_urls() {
        for base_url in [
            "example.com/v1",
            "ftp://example.com/v1",
            "https:///v1",
            "https://example.com:bad/v1",
            "https://user@example.com/v1",
            "https://example.com/v1?query=true",
        ] {
            let error =
                Config::from_sources("", [("CCGPT_API_KEY", "key"), ("CCGPT_BASE_URL", base_url)])
                    .unwrap_err();
            assert!(error.to_string().starts_with("invalid CCGPT_BASE_URL"));
        }
    }

    #[test]
    fn rejects_invalid_port() {
        let error =
            Config::from_sources("CCGPT_PORT=70000", Vec::<(String, String)>::new()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "CCGPT_PORT must be a number from 0 to 65535"
        );
    }

    #[cfg(unix)]
    #[test]
    fn secures_config_file_before_reading() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".ccgptrc");
        std::fs::write(&path, "CCGPT_PORT=9000").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        Config::load_from(&path, Vec::<(String, String)>::new()).unwrap();

        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
