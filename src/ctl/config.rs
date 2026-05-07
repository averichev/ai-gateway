use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{CtlError, Result};

pub const DEFAULT_CONFIG_PATH: &str = "/opt/ai-gateway/ctl.toml";
pub const DEFAULT_APP_DIR: &str = "/opt/ai-gateway";
pub const DEFAULT_IMAGE: &str = "ghcr.io/your-org/ai-gateway:latest";
pub const DEFAULT_CONTAINER_NAME: &str = "ai-gateway";
pub const DEFAULT_VOLUME_NAME: &str = "ai-gateway-pgdata";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtlConfig {
    pub app_dir: PathBuf,
    pub env_file: PathBuf,
    pub image: String,
    pub container_name: String,
    pub published_port: u16,
    pub volume_name: String,
}

impl Default for CtlConfig {
    fn default() -> Self {
        Self::with_app_dir(PathBuf::from(DEFAULT_APP_DIR), None)
    }
}

impl CtlConfig {
    pub fn with_app_dir(app_dir: PathBuf, image: Option<String>) -> Self {
        Self {
            env_file: app_dir.join(".env"),
            app_dir,
            image: image.unwrap_or_else(|| DEFAULT_IMAGE.to_owned()),
            container_name: DEFAULT_CONTAINER_NAME.to_owned(),
            published_port: 8080,
            volume_name: DEFAULT_VOLUME_NAME.to_owned(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|error| CtlError::MissingConfig(path.to_path_buf(), error))?;

        Ok(toml::from_str(&content)?)
    }

    pub fn load_if_exists(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        Self::load(path).map(Some)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn healthcheck_url(&self) -> String {
        format!("http://127.0.0.1:{}/health", self.published_port)
    }
}
