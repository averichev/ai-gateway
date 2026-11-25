use std::{env, net::SocketAddr, path::Path, sync::Arc};

use serde::Serialize;
use standalone_updater::{OperationStatus, UpdateError, UpdateMonitor, UpdateSource};

#[cfg(target_os = "linux")]
pub(crate) mod linux;

pub const PRODUCT_ID: &str = "cf9f6111-304c-40e4-8c78-6c84e600dd9e";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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

fn read_source(path: &Path) -> standalone_updater::Result<UpdateSource> {
    let source: UpdateSource =
        serde_json::from_slice(&std::fs::read(path)?).map_err(|_| UpdateError::Document)?;
    if source.product_id != PRODUCT_ID {
        return Err(UpdateError::Trust(
            "UUID источника не принадлежит AI Gateway",
        ));
    }
    Ok(source)
}

pub(crate) fn external_monitor(path: &Path) -> standalone_updater::Result<UpdateMonitor> {
    UpdateMonitor::new(read_source(path)?, VERSION, &protocol_target())
}

impl Updates {
    pub(crate) async fn launch(_address: SocketAddr) -> Result<Self, Box<dyn std::error::Error>> {
        let mode = env::var_os("GATEWAY_UPDATE_MODE").unwrap_or_else(|| "external".into());
        let source = env::var_os("GATEWAY_UPDATE_SOURCE").filter(|path| !path.is_empty());
        let internal = env::args()
            .nth(1)
            .is_some_and(|arg| arg.starts_with("--standalone-updater-"));
        match mode.to_str() {
            Some("external") if internal => {
                Err("служебная команда требует настроенного Linux runtime".into())
            }
            Some("external") => match source {
                Some(path) => {
                    let monitor = external_monitor(Path::new(&path))?;
                    Ok(Self::External(Arc::new(monitor)))
                }
                None => Ok(Self::Disabled),
            },
            #[cfg(target_os = "linux")]
            Some(mode @ ("supervisor" | "standalone")) => {
                let path = source.ok_or("для установки требуется GATEWAY_UPDATE_SOURCE")?;
                linux::launch(read_source(Path::new(&path))?, mode, _address).await
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
