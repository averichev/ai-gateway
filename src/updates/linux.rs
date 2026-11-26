use std::{env, net::SocketAddr};

use sha2::{Digest, Sha256};
use standalone_updater::{
    UpdateSource,
    runtime::{
        self, BuildInfo, DataPolicy, InstallationMode, Launch, RECOVER_ARGUMENT, Readiness,
        RuntimeConfig, SERVER_ARGUMENT, WORKER_ARGUMENT,
    },
};

use super::{PRODUCT_ID, Updates, VERSION};
use crate::repositories::database::{POSTGRES_MIGRATOR, SQLITE_MIGRATOR};

pub(crate) fn build_info() -> BuildInfo {
    let mut hash = Sha256::new();
    // Bump the format marker when changing persisted formats outside migrations.
    hash.update(b"ai-gateway:provider-secrets-aes256gcm-v1\0");
    hash.update(b"ai-gateway:master-key-file-base64-v1\0");
    // Keep the UI delivery contract across binary replacements.
    hash.update([u8::from(cfg!(feature = "embedded-admin"))]);
    for (backend, migrations) in [
        ("postgres", &POSTGRES_MIGRATOR),
        ("sqlite", &SQLITE_MIGRATOR),
    ] {
        hash.update(backend.as_bytes());
        for migration in migrations
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
        {
            hash.update(migration.version.to_be_bytes());
            hash.update(migration.checksum.as_ref());
        }
    }
    BuildInfo {
        version: VERSION.into(),
        rust_target: env!("GATEWAY_BUILD_TARGET").into(),
        product_id: PRODUCT_ID.into(),
        self_update_protocol: 1,
        data_compatibility: hex::encode(hash.finalize()),
    }
}

pub(super) async fn launch(
    source: UpdateSource,
    mode: &str,
    address: SocketAddr,
) -> Result<Updates, Box<dyn std::error::Error>> {
    let mut arguments: Vec<_> = env::args().skip(1).collect();
    if matches!(
        arguments.first().map(String::as_str),
        Some(SERVER_ARGUMENT | WORKER_ARGUMENT | RECOVER_ARGUMENT)
    ) {
        arguments.remove(0);
    }
    let config = RuntimeConfig {
        build: build_info(),
        source,
        mode: if mode == "supervisor" {
            InstallationMode::Supervisor
        } else {
            InstallationMode::Standalone
        },
        executable: env::current_exe()?,
        updates_directory: env::var_os("GATEWAY_UPDATE_DIR")
            .unwrap_or_else(|| "/var/lib/ai-gateway/updater".into())
            .into(),
        working_directory: env::current_dir()?,
        arguments,
        readiness: Readiness::http(address, "/ready"),
        data_policy: DataPolicy::RequireExact,
    };
    match runtime::launch(config).await? {
        Launch::Serve(manager) => Ok(Updates::Managed(manager)),
        Launch::Exit(code) => std::process::exit(code),
    }
}
