use std::{env, fmt, net::SocketAddr, path::PathBuf};

pub const DEFAULT_BIND: &str = "127.0.0.1:8788";
const TOKEN_MIN_LEN: usize = 32;

#[derive(Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub data_dir: PathBuf,
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingToken,
    WeakToken,
    InvalidBind,
    RemoteBindRequiresOptIn,
    InvalidDataDir,
    NoHomeDirectory,
    InvalidBrainSlug,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "configuration error: {self:?}")
    }
}

impl std::error::Error for ConfigError {}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("data_dir", &self.data_dir)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    pub(crate) fn from_lookup(
        mut get: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let bind_text = get("OIB_BIND").unwrap_or_else(|| DEFAULT_BIND.to_owned());
        let bind_addr: SocketAddr = bind_text.parse().map_err(|_| ConfigError::InvalidBind)?;

        let token = get("OIB_API_TOKEN").ok_or(ConfigError::MissingToken)?;
        if token.len() < TOKEN_MIN_LEN || !token.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err(ConfigError::WeakToken);
        }

        if !bind_addr.ip().is_loopback() && get("OIB_ALLOW_REMOTE").as_deref() != Some("1") {
            return Err(ConfigError::RemoteBindRequiresOptIn);
        }

        let data_dir = match get("OIB_DATA_DIR") {
            Some(value) => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err(ConfigError::InvalidDataDir);
                }
                path
            }
            None => default_data_dir(&mut get)?,
        };

        Ok(Self {
            bind_addr,
            data_dir,
            token,
        })
    }

    pub fn api_token(&self) -> &str {
        &self.token
    }

    pub fn profile_dir(&self, slug: &str) -> Result<PathBuf, ConfigError> {
        if slug.is_empty()
            || !(slug.as_bytes()[0].is_ascii_lowercase() || slug.as_bytes()[0].is_ascii_digit())
            || !slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(ConfigError::InvalidBrainSlug);
        }
        Ok(self.data_dir.join("profiles").join(slug))
    }
}

fn default_data_dir(get: &mut impl FnMut(&str) -> Option<String>) -> Result<PathBuf, ConfigError> {
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = get("LOCALAPPDATA").filter(|value| !value.is_empty()) {
            let path = PathBuf::from(root).join("OpenAIInferenceBridge");
            if path.is_absolute() {
                return Ok(path);
            }
            return Err(ConfigError::InvalidDataDir);
        }
        if let Some(home) = get("USERPROFILE").filter(|value| !value.is_empty()) {
            let path = PathBuf::from(home).join("AppData/Local/OpenAIInferenceBridge");
            if path.is_absolute() {
                return Ok(path);
            }
            return Err(ConfigError::InvalidDataDir);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = get("HOME").filter(|value| !value.is_empty()) {
            let path =
                PathBuf::from(home).join("Library/Application Support/OpenAIInferenceBridge");
            if path.is_absolute() {
                return Ok(path);
            }
            return Err(ConfigError::InvalidDataDir);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = get("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            let path = PathBuf::from(xdg);
            if path.is_absolute() {
                return Ok(path.join("openai-inference-bridge"));
            }
        }
        if let Some(home) = get("HOME").filter(|value| !value.is_empty()) {
            let path = PathBuf::from(home).join(".local/share/openai-inference-bridge");
            if path.is_absolute() {
                return Ok(path);
            }
            return Err(ConfigError::InvalidDataDir);
        }
    }

    Err(ConfigError::NoHomeDirectory)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "example-secret-token-that-is-long-enough";

    fn config(values: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::from_lookup(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    fn temp_data_dir() -> String {
        env::temp_dir()
            .join("oib-test")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn defaults_to_loopback_and_expected_port() {
        let data_dir = temp_data_dir();
        let config = config(&[("OIB_API_TOKEN", TOKEN), ("OIB_DATA_DIR", &data_dir)]).unwrap();
        assert_eq!(config.bind_addr.to_string(), DEFAULT_BIND);
    }

    #[test]
    fn token_is_required_and_must_be_strong_ascii() {
        let data_dir = temp_data_dir();
        assert_eq!(
            config(&[("OIB_DATA_DIR", &data_dir)]).unwrap_err(),
            ConfigError::MissingToken
        );
        assert_eq!(
            config(&[("OIB_API_TOKEN", "short"), ("OIB_DATA_DIR", &data_dir)]).unwrap_err(),
            ConfigError::WeakToken
        );
        assert_eq!(
            config(&[
                ("OIB_API_TOKEN", "token-with-newline\nnot-valid"),
                ("OIB_DATA_DIR", &data_dir),
            ])
            .unwrap_err(),
            ConfigError::WeakToken
        );
    }

    #[test]
    fn non_loopback_requires_explicit_opt_in() {
        let data_dir = temp_data_dir();
        let values = [
            ("OIB_API_TOKEN", TOKEN),
            ("OIB_BIND", "0.0.0.0:8788"),
            ("OIB_DATA_DIR", &data_dir),
        ];
        assert_eq!(
            config(&values).unwrap_err(),
            ConfigError::RemoteBindRequiresOptIn
        );

        let values = [
            ("OIB_API_TOKEN", TOKEN),
            ("OIB_BIND", "0.0.0.0:8788"),
            ("OIB_ALLOW_REMOTE", "1"),
            ("OIB_DATA_DIR", &data_dir),
        ];
        assert!(config(&values).is_ok());
    }

    #[test]
    fn data_directory_must_be_absolute() {
        assert_eq!(
            config(&[("OIB_API_TOKEN", TOKEN), ("OIB_DATA_DIR", "relative/path")]).unwrap_err(),
            ConfigError::InvalidDataDir
        );
    }

    #[test]
    fn token_is_redacted_from_debug() {
        let data_dir = temp_data_dir();
        let config = config(&[("OIB_API_TOKEN", TOKEN), ("OIB_DATA_DIR", &data_dir)]).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains(TOKEN));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn profile_path_is_scoped_and_slug_is_validated() {
        let data_dir = temp_data_dir();
        let config = config(&[("OIB_API_TOKEN", TOKEN), ("OIB_DATA_DIR", &data_dir)]).unwrap();
        assert_eq!(
            config.profile_dir("chatgpt").unwrap(),
            env::temp_dir().join("oib-test/profiles/chatgpt")
        );
        assert_eq!(
            config.profile_dir("1brain").unwrap(),
            env::temp_dir().join("oib-test/profiles/1brain")
        );
        assert!(config.profile_dir("../shared").is_err());
        assert!(config.profile_dir("ChatGPT").is_err());
    }
}
