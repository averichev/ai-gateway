use std::{collections::BTreeMap, fs, path::Path};

use super::{CtlError, Result};

pub const REQUIRED_ENV_KEYS: &[&str] = &[
    "APP_HOST",
    "APP_PORT",
    "DATABASE_URL",
    "GATEWAY_MASTER_KEY",
    "ADMIN_DIST_DIR",
    "POSTGRES_USER",
    "POSTGRES_PASSWORD",
    "POSTGRES_DB",
    "DEFAULT_PROVIDER_CODE",
    "DEFAULT_PROVIDER_KIND",
    "DEFAULT_PROVIDER_BASE_URL",
    "DEFAULT_PROVIDER_TIMEOUT_MS",
    "DEFAULT_MODEL_ALIAS",
    "DEFAULT_EXTERNAL_MODEL",
];

#[derive(Debug, Clone, Default)]
pub struct EnvFile {
    values: BTreeMap<String, String>,
}

impl EnvFile {
    pub fn from_template() -> Result<Self> {
        Self::parse(TEMPLATE_ENV)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|error| CtlError::MissingEnv(path.to_path_buf(), error))?;

        Self::parse(&content)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut lines = Vec::with_capacity(self.values.len());

        for (key, value) in &self.values {
            lines.push(format!("{key}={value}"));
        }

        fs::write(path, format!("{}\n", lines.join("\n")))?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.values.iter()
    }

    pub fn validation_rows(&self) -> Vec<EnvValidationRow> {
        REQUIRED_ENV_KEYS
            .iter()
            .map(|key| {
                let value = self.get(key).unwrap_or_default();
                let status = if value.is_empty() {
                    EnvValueStatus::Missing
                } else if is_placeholder_value(value) {
                    EnvValueStatus::Placeholder
                } else {
                    EnvValueStatus::Present
                };

                EnvValidationRow {
                    key: (*key).to_owned(),
                    status,
                    safe_value: match status {
                        EnvValueStatus::Missing => "-".to_owned(),
                        EnvValueStatus::Placeholder => "шаблонное значение".to_owned(),
                        EnvValueStatus::Present => redact_value(key, value),
                    },
                }
            })
            .collect()
    }

    pub fn missing_required_keys(&self) -> Vec<String> {
        self.validation_rows()
            .into_iter()
            .filter(|row| row.status == EnvValueStatus::Missing)
            .map(|row| row.key)
            .collect()
    }

    pub fn placeholder_required_keys(&self) -> Vec<String> {
        self.validation_rows()
            .into_iter()
            .filter(|row| row.status == EnvValueStatus::Placeholder)
            .map(|row| row.key)
            .collect()
    }

    pub fn safe_rows(&self) -> Vec<(String, String)> {
        self.iter()
            .map(|(key, value)| (key.clone(), redact_value(key, value)))
            .collect()
    }

    fn parse(content: &str) -> Result<Self> {
        let mut values = BTreeMap::new();

        for (index, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(CtlError::Parse(format!(
                    "некорректная строка .env {}: `{raw_line}`",
                    index + 1
                )));
            };

            values.insert(key.trim().to_owned(), value.trim().to_owned());
        }

        Ok(Self { values })
    }
}

#[derive(Debug, Clone)]
pub struct EnvValidationRow {
    pub key: String,
    pub status: EnvValueStatus,
    pub safe_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvValueStatus {
    Present,
    Missing,
    Placeholder,
}

pub fn redact_value(key: &str, value: &str) -> String {
    if is_secret_key(key) {
        let length = value.chars().count();

        if length <= 6 {
            return "******".to_owned();
        }

        let head = value.chars().take(3).collect::<String>();
        let tail = value
            .chars()
            .rev()
            .take(2)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();

        format!("{head}***{tail}")
    } else {
        value.to_owned()
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    if key.ends_with("_ENV") {
        return false;
    }

    ["KEY", "PASSWORD", "SECRET", "TOKEN"]
        .iter()
        .any(|marker| key.contains(marker))
}

fn is_placeholder_value(value: &str) -> bool {
    matches!(
        value.trim(),
        "replace-me" | "your-key" | "<replace-me>" | "<your-key>"
    )
}

const TEMPLATE_ENV: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.env.example"));
