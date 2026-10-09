//! Project-owned Brain registry and canonical OpenAI model IDs.

use std::collections::HashSet;
use std::env;
use std::fmt;
use std::path::PathBuf;

use crate::config::{Config, ConfigError};

pub const MODEL_ID_PREFIX: &str = "oib/";
const SUPPORTED_ADAPTERS: &[&str] = &["chatgpt-web"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brain {
    pub id: String,
    pub display_name: String,
    pub enabled: bool,
    pub start_url: Option<String>,
    pub adapter_kind: Option<String>,
    pub profile_dir: PathBuf,
}

impl Brain {
    pub fn model_id(&self) -> String {
        format!("{MODEL_ID_PREFIX}{}", self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainRegistry {
    brains: Vec<Brain>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidBrainId(String),
    DuplicateBrainId(String),
    MissingField { brain_id: String, field: String },
    InvalidEnabledValue { brain_id: String },
    InvalidStartUrl { brain_id: String },
    UnsupportedAdapter { brain_id: String, adapter: String },
    InvalidProfilePath { brain_id: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBrainId(id) => write!(f, "invalid Brain ID: {id}"),
            Self::DuplicateBrainId(id) => write!(f, "duplicate Brain ID: {id}"),
            Self::MissingField { brain_id, field } => {
                write!(f, "missing configuration field {field} for Brain {brain_id}")
            }
            Self::InvalidEnabledValue { brain_id } => {
                write!(f, "invalid enabled flag for Brain {brain_id}; expected 0 or 1")
            }
            Self::InvalidStartUrl { brain_id } => {
                write!(f, "invalid HTTP(S) start URL for Brain {brain_id}")
            }
            Self::UnsupportedAdapter { brain_id, adapter } => {
                write!(f, "unsupported adapter {adapter} for Brain {brain_id}")
            }
            Self::InvalidProfilePath { brain_id } => {
                write!(f, "could not derive isolated profile path for Brain {brain_id}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

impl BrainRegistry {
    pub fn empty() -> Self {
        Self { brains: Vec::new() }
    }

    pub fn from_env(config: &Config) -> Result<Self, RegistryError> {
        Self::from_lookup(config, |key| env::var(key).ok())
    }

    fn from_lookup(
        config: &Config,
        mut get: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, RegistryError> {
        let ids_text = get("OIB_BRAIN_IDS").unwrap_or_default();
        if ids_text.trim().is_empty() {
            return Ok(Self { brains: Vec::new() });
        }

        let mut seen = HashSet::new();
        let mut brains = Vec::new();

        for raw_id in ids_text.split(',') {
            let id = raw_id.trim();
            if !valid_brain_id(id) {
                return Err(RegistryError::InvalidBrainId(id.to_owned()));
            }
            if !seen.insert(id.to_owned()) {
                return Err(RegistryError::DuplicateBrainId(id.to_owned()));
            }

            let env_prefix = format!("OIB_BRAIN_{}", id.to_ascii_uppercase());
            let enabled_key = format!("{env_prefix}_ENABLED");
            let enabled_value = get(&enabled_key).ok_or_else(|| RegistryError::MissingField {
                brain_id: id.to_owned(),
                field: "ENABLED".to_owned(),
            })?;
            let enabled = match enabled_value.as_str() {
                "1" => true,
                "0" => false,
                _ => return Err(RegistryError::InvalidEnabledValue {
                    brain_id: id.to_owned(),
                }),
            };

            let profile_dir = config
                .profile_dir(id)
                .map_err(|_: ConfigError| RegistryError::InvalidProfilePath {
                    brain_id: id.to_owned(),
                })?;

            if !enabled {
                brains.push(Brain {
                    id: id.to_owned(),
                    display_name: id.to_owned(),
                    enabled: false,
                    start_url: None,
                    adapter_kind: None,
                    profile_dir,
                });
                continue;
            }

            let display_name = required_value(&mut get, &env_prefix, "LABEL", id)?;
            let start_url = required_value(&mut get, &env_prefix, "URL", id)?;
            if !valid_start_url(&start_url) {
                return Err(RegistryError::InvalidStartUrl {
                    brain_id: id.to_owned(),
                });
            }
            let adapter_kind = required_value(&mut get, &env_prefix, "ADAPTER", id)?;
            if !SUPPORTED_ADAPTERS.contains(&adapter_kind.as_str()) {
                return Err(RegistryError::UnsupportedAdapter {
                    brain_id: id.to_owned(),
                    adapter: adapter_kind,
                });
            }

            brains.push(Brain {
                id: id.to_owned(),
                display_name,
                enabled: true,
                start_url: Some(start_url),
                adapter_kind: Some(adapter_kind),
                profile_dir,
            });
        }

        Ok(Self { brains })
    }

    pub fn brains(&self) -> &[Brain] {
        &self.brains
    }

    pub fn resolve_model_id(&self, model_id: &str) -> Option<&Brain> {
        self.brains
            .iter()
            .find(|brain| brain.enabled && brain.model_id() == model_id)
    }

    /// Produce the OpenAI model-list response. IDs are validated ASCII slugs,
    /// so no user-provided string is interpolated into JSON without validation.
    pub fn models_json(&self) -> String {
        let data = self
            .brains
            .iter()
            .filter(|brain| brain.enabled)
            .map(|brain| {
                format!(
                    r#"{{"id":"{}","object":"model","created":0,"owned_by":"openai-inference-bridge"}}"#,
                    brain.model_id()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"object":"list","data":[{data}]}}"#)
    }
}

fn required_value(
    get: &mut impl FnMut(&str) -> Option<String>,
    prefix: &str,
    suffix: &str,
    brain_id: &str,
) -> Result<String, RegistryError> {
    let field = format!("{prefix}_{suffix}");
    get(&field)
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .ok_or_else(|| RegistryError::MissingField {
            brain_id: brain_id.to_owned(),
            field: suffix.to_owned(),
        })
}

fn valid_brain_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 63
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_start_url(url: &str) -> bool {
    if url.is_empty()
        || url.contains('#')
        || url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }

    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest
        .split(|character| matches!(character, '/' | '?'))
        .next()
        .unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }

    let (host, port, is_ipv6) = if authority.starts_with('[') {
        let Some(close) = authority.find(']') else {
            return false;
        };
        let host = &authority[1..close];
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        let suffix = &authority[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else if let Some(port) = suffix.strip_prefix(':') {
            Some(port)
        } else {
            return false;
        };
        (host, port, true)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.contains(':') {
            return false;
        }
        (host, Some(port), false)
    } else {
        (authority, None, false)
    };

    if let Some(port) = port {
        let Ok(port_number) = port.parse::<u16>() else {
            return false;
        };
        if port_number == 0 {
            return false;
        }
    }

    if is_ipv6 || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }

    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "registry-test-token-that-is-long-enough";

    fn config() -> Config {
        let data_dir = env::temp_dir().to_string_lossy().into_owned();
        Config::from_lookup(|key| match key {
            "OIB_API_TOKEN" => Some(TOKEN.to_owned()),
            "OIB_DATA_DIR" => Some(data_dir.clone()),
            _ => None,
        })
        .unwrap()
    }

    fn registry(values: &[(&str, &str)]) -> Result<BrainRegistry, RegistryError> {
        BrainRegistry::from_lookup(&config(), |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    fn enabled_chatgpt() -> [(&'static str, &'static str); 5] {
        [
            ("OIB_BRAIN_IDS", "chatgpt"),
            ("OIB_BRAIN_CHATGPT_ENABLED", "1"),
            ("OIB_BRAIN_CHATGPT_LABEL", "ChatGPT"),
            ("OIB_BRAIN_CHATGPT_URL", "https://chatgpt.com/"),
            ("OIB_BRAIN_CHATGPT_ADAPTER", "chatgpt-web"),
        ]
    }

    #[test]
    fn absent_brain_configuration_produces_empty_registry() {
        let registry = registry(&[]).unwrap();
        assert!(registry.brains().is_empty());
        assert_eq!(registry.models_json(), r#"{"object":"list","data":[]}"#);
    }

    #[test]
    fn enabled_brain_uses_canonical_model_id_and_is_resolvable() {
        let registry = registry(&enabled_chatgpt()).unwrap();
        assert_eq!(registry.resolve_model_id("oib/chatgpt").unwrap().display_name, "ChatGPT");
        assert!(registry.resolve_model_id("chatgpt").is_none());
        assert!(registry.resolve_model_id("oib/unknown").is_none());
        assert!(registry.models_json().contains(r#""id":"oib/chatgpt""#));
        assert!(!registry.models_json().contains("chatgpt.com"));
    }

    #[test]
    fn disabled_brain_is_not_listed_or_resolvable() {
        let registry = registry(&[
            ("OIB_BRAIN_IDS", "chatgpt"),
            ("OIB_BRAIN_CHATGPT_ENABLED", "0"),
        ])
        .unwrap();
        assert!(registry.resolve_model_id("oib/chatgpt").is_none());
        assert_eq!(registry.models_json(), r#"{"object":"list","data":[]}"#);
    }

    #[test]
    fn invalid_and_duplicate_ids_are_rejected() {
        assert!(matches!(
            registry(&[("OIB_BRAIN_IDS", "ChatGPT")]),
            Err(RegistryError::InvalidBrainId(_))
        ));
        assert!(matches!(
            registry(&[("OIB_BRAIN_IDS", "chatgpt,chatgpt")]),
            Err(RegistryError::DuplicateBrainId(_))
        ));
        assert!(matches!(
            registry(&[("OIB_BRAIN_IDS", "../shared")]),
            Err(RegistryError::InvalidBrainId(_))
        ));
    }

    #[test]
    fn enabled_brain_requires_complete_valid_configuration() {
        let values = [
            ("OIB_BRAIN_IDS", "chatgpt"),
            ("OIB_BRAIN_CHATGPT_ENABLED", "1"),
            ("OIB_BRAIN_CHATGPT_LABEL", "ChatGPT"),
            ("OIB_BRAIN_CHATGPT_URL", "file:///etc/passwd"),
            ("OIB_BRAIN_CHATGPT_ADAPTER", "chatgpt-web"),
        ];
        assert!(matches!(
            registry(&values),
            Err(RegistryError::InvalidStartUrl { .. })
        ));

        let values = [
            ("OIB_BRAIN_IDS", "chatgpt"),
            ("OIB_BRAIN_CHATGPT_ENABLED", "1"),
            ("OIB_BRAIN_CHATGPT_LABEL", "ChatGPT"),
            ("OIB_BRAIN_CHATGPT_URL", "https://chatgpt.com/"),
            ("OIB_BRAIN_CHATGPT_ADAPTER", "unknown"),
        ];
        assert!(matches!(
            registry(&values),
            Err(RegistryError::UnsupportedAdapter { .. })
        ));
    }

    #[test]
    fn start_url_validation_rejects_unsafe_or_malformed_authorities() {
        assert!(valid_start_url("https://chatgpt.com/"));
        assert!(valid_start_url("http://localhost:3000/"));
        assert!(valid_start_url("https://[::1]:8443/"));
        assert!(!valid_start_url("file:///etc/passwd"));
        assert!(!valid_start_url("https://user@host.example/"));
        assert!(!valid_start_url("https://host.example:70000/"));
        assert!(!valid_start_url("https://host.example/#fragment"));
        assert!(!valid_start_url("https://bad..host.example/"));
    }

    #[test]
    fn enabled_brain_requires_enabled_flag() {
        assert!(matches!(
            registry(&[("OIB_BRAIN_IDS", "chatgpt")]),
            Err(RegistryError::MissingField { .. })
        ));
    }

    #[test]
    fn ids_may_start_with_a_digit_but_must_remain_canonical() {
        let values = [
            ("OIB_BRAIN_IDS", "1brain"),
            ("OIB_BRAIN_1BRAIN_ENABLED", "0"),
        ];
        let registry = registry(&values).unwrap();
        assert_eq!(registry.brains()[0].id, "1brain");
    }
}
