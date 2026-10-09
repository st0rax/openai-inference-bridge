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
        let bind_text = env::var("OIB_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());
        let bind_addr = bind_text.parse().map_err(|_| ConfigError::InvalidBind)?;
        let token = env::var("OIB_API_TOKEN").map_err(|_| ConfigError::MissingToken)?;
        if token.len() < TOKEN_MIN_LEN || !token.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
            return Err(ConfigError::WeakToken);
        }
        if !bind_addr.ip().is_loopback() && env::var("OIB_ALLOW_REMOTE").as_deref() != Ok("1") {
            return Err(ConfigError::RemoteBindRequiresOptIn);
        }
        let data_dir = match env::var_os("OIB_DATA_DIR") {
            Some(p) => {
                let p = PathBuf::from(p);
                if !p.is_absolute() { return Err(ConfigError::InvalidDataDir); }
                p
            }
            None => default_data_dir()?,
        };
        Ok(Self { bind_addr, data_dir, token })
    }

    pub fn api_token(&self) -> &str { &self.token }

    pub fn profile_dir(&self, slug: &str) -> Result<PathBuf, ConfigError> {
        if slug.is_empty() || !slug.as_bytes()[0].is_ascii_lowercase()
            || !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err(ConfigError::InvalidBrainSlug);
        }
        Ok(self.data_dir.join("profiles").join(slug))
    }
}

fn default_data_dir() -> Result<PathBuf, ConfigError> {
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(root).join("OpenAIInferenceBridge"));
        }
        if let Some(home) = env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(home).join("AppData/Local/OpenAIInferenceBridge"));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home).join("Library/Application Support/OpenAIInferenceBridge"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
            let p = PathBuf::from(xdg);
            if p.is_absolute() { return Ok(p.join("openai-inference-bridge")); }
        }
        if let Some(home) = env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".local/share/openai-inference-bridge"));
        }
    }
    Err(ConfigError::NoHomeDirectory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bind_is_loopback() {
        assert_eq!(DEFAULT_BIND, "127.0.0.1:8788");
    }

    #[test]
    fn token_debug_is_redacted() {
        let c = Config { bind_addr: DEFAULT_BIND.parse().unwrap(), data_dir: PathBuf::from("/tmp/oib"), token: "example-secret-token-that-is-long-enough".into() };
        assert!(!format!("{c:?}").contains(c.api_token()));
        assert!(format!("{c:?}").contains("[REDACTED]"));
    }

    #[test]
    fn profile_path_is_scoped_and_rejects_traversal() {
        let c = Config { bind_addr: DEFAULT_BIND.parse().unwrap(), data_dir: PathBuf::from("/tmp/oib"), token: "example-secret-token-that-is-long-enough".into() };
        assert_eq!(c.profile_dir("chatgpt").unwrap(), PathBuf::from("/tmp/oib/profiles/chatgpt"));
        assert!(c.profile_dir("../shared").is_err());
        assert!(c.profile_dir("ChatGPT").is_err());
    }
}
