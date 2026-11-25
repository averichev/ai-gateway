use std::{env, ffi::OsStr, net::SocketAddr, path::Path, sync::Arc};

use serde::Serialize;
use standalone_updater::{OperationStatus, UpdateError, UpdateMonitor, UpdateSource};

#[cfg(target_os = "linux")]
pub(crate) mod linux;

pub const PRODUCT_ID: &str = include_str!("updates/product-id.txt").trim_ascii_end();
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Default)]
pub enum Updates {
    #[default]
    Disabled,
    External(Arc<UpdateMonitor>),
    #[cfg(target_os = "linux")]
    Managed(standalone_updater::runtime::UpdateManager),
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Summary {
    #[cfg(target_os = "linux")]
    Managed(standalone_updater::runtime::UpdateSummary),
    ReadOnly {
        current_version: &'static str,
        target: String,
        installation_mode: &'static str,
        installation_supported: bool,
        installation_reason: &'static str,
        status: OperationStatus,
    },
}

fn protocol_target() -> String {
    let os = match env::consts::OS {
        "macos" => "darwin",
        os => os,
    };
    format!("{os}-{}", env::consts::ARCH)
}

#[derive(Clone, Copy)]
pub(crate) enum UpdateLocation<'a> {
    Manifest(&'a str),
    LegacyFile(&'a Path),
}

impl<'a> UpdateLocation<'a> {
    pub(crate) fn new(value: &'a OsStr) -> Self {
        let path = Path::new(value);
        match value.to_str() {
            Some(url) if !path.is_absolute() && url.contains("://") => Self::Manifest(url),
            _ => Self::LegacyFile(path),
        }
    }

    pub(crate) fn legacy_file(self) -> Option<&'a Path> {
        match self {
            Self::LegacyFile(path) => Some(path),
            Self::Manifest(_) => None,
        }
    }

    fn source(self) -> standalone_updater::Result<UpdateSource> {
        match self {
            Self::Manifest(url) => {
                let origin = reqwest::Url::parse(url)
                    .map_err(|_| UpdateError::Trust("некорректный URL манифеста"))?
                    .origin()
                    .ascii_serialization();
                let public_key = serde_json::from_str(include_str!("updates/public-key.json"))
                    .expect("AI Gateway embeds a public Ed25519 JWK");
                Ok(UpdateSource::from_manifest(
                    url,
                    PRODUCT_ID,
                    public_key,
                    vec![origin],
                    MAX_ARTIFACT_BYTES,
                ))
            }
            Self::LegacyFile(path) => {
                let source: UpdateSource = serde_json::from_slice(&std::fs::read(path)?)
                    .map_err(|_| UpdateError::Document)?;
                if source.product_id != PRODUCT_ID {
                    return Err(UpdateError::Trust(
                        "UUID источника не принадлежит AI Gateway",
                    ));
                }
                Ok(source)
            }
        }
    }

    pub(crate) fn monitor(self) -> standalone_updater::Result<UpdateMonitor> {
        UpdateMonitor::new(self.source()?, VERSION, &protocol_target())
    }
}

impl Updates {
    pub(crate) async fn launch(_address: SocketAddr) -> Result<Self, Box<dyn std::error::Error>> {
        let mode = env::var_os("GATEWAY_UPDATE_MODE").unwrap_or_else(|| "external".into());
        let source = env::var_os("GATEWAY_UPDATE_SOURCE").filter(|value| !value.is_empty());
        let source = source.as_deref().map(UpdateLocation::new);
        let internal = env::args()
            .nth(1)
            .is_some_and(|arg| arg.starts_with("--standalone-updater-"));
        match mode.to_str() {
            Some("external") if internal => {
                Err("служебная команда требует настроенного Linux runtime".into())
            }
            Some("external") => match source {
                Some(location) => Ok(Self::External(Arc::new(location.monitor()?))),
                None => Ok(Self::Disabled),
            },
            #[cfg(target_os = "linux")]
            Some(mode @ ("supervisor" | "standalone")) => {
                let location = source.ok_or("для установки требуется GATEWAY_UPDATE_SOURCE")?;
                linux::launch(location.source()?, mode, _address).await
            }
            _ => Err("недопустимый GATEWAY_UPDATE_MODE для этой платформы".into()),
        }
    }

    pub fn summary(&self) -> standalone_updater::Result<Summary> {
        let (mode, reason, status) = match self {
            Self::Disabled => (
                "disabled",
                "Источник обновлений не настроен.",
                OperationStatus::default(),
            ),
            Self::External(monitor) => (
                "external",
                "Версию устанавливает система развёртывания.",
                monitor.status()?,
            ),
            #[cfg(target_os = "linux")]
            Self::Managed(manager) => return manager.summary().map(Summary::Managed),
        };
        Ok(Summary::ReadOnly {
            current_version: VERSION,
            target: protocol_target(),
            installation_mode: mode,
            installation_supported: false,
            installation_reason: reason,
            status,
        })
    }

    pub fn check(&self) -> standalone_updater::Result<OperationStatus> {
        match self {
            Self::Disabled => Err(UpdateError::Unsupported("источник обновлений не настроен")),
            Self::External(monitor) => monitor.check(),
            #[cfg(target_os = "linux")]
            Self::Managed(manager) => manager.check(),
        }
    }

    pub(crate) async fn shutdown_requested(&self) {
        #[cfg(target_os = "linux")]
        if let Self::Managed(manager) = self {
            return manager.shutdown_requested().await;
        }
        std::future::pending().await
    }

    pub(crate) fn finish_shutdown(&self) -> standalone_updater::Result<()> {
        #[cfg(target_os = "linux")]
        if let Self::Managed(manager) = self {
            return manager.finish_shutdown();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use standalone_updater::UpdateClient;

    #[tokio::test]
    async fn manifest_url_pins_product_and_download_origin_including_port() {
        let url = "https://updates.example.org:8443/channel/manifest.json";
        let location = UpdateLocation::new(OsStr::new(url));
        let source = location.source().unwrap();
        assert_eq!(source.manifest_url, url);
        assert!(source.descriptor_url.is_empty());
        assert_eq!(source.product_id, PRODUCT_ID);
        assert_eq!(source.max_artifact_bytes, 256 * 1024 * 1024);

        let client = UpdateClient::new(source, VERSION, "linux-aarch64").unwrap();
        assert!(
            client
                .artifact_url("https://updates.example.org:8443/file")
                .is_ok()
        );
        assert!(
            client
                .artifact_url("https://updates.example.org/file")
                .is_err()
        );
        assert!(
            client
                .artifact_url("https://other.example.org:8443/file")
                .is_err()
        );
    }
}
