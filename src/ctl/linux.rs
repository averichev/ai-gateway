use std::{collections::HashMap, fs};

#[derive(Debug, Clone)]
pub struct LinuxPlatform {
    pub id: String,
    pub pretty_name: String,
    pub version_id: Option<String>,
}

impl LinuxPlatform {
    pub fn detect() -> Option<Self> {
        let content = fs::read_to_string("/etc/os-release").ok()?;
        let values = parse_os_release(&content);

        let id = values.get("ID")?.to_owned();
        let pretty_name = values
            .get("PRETTY_NAME")
            .cloned()
            .unwrap_or_else(|| id.clone());

        Some(Self {
            id,
            pretty_name,
            version_id: values.get("VERSION_ID").cloned(),
        })
    }

    pub fn is_supported_for_docker_install(&self) -> bool {
        matches!(self.id.as_str(), "ubuntu" | "debian")
    }
}

fn parse_os_release(content: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.to_owned(), value.trim_matches('"').to_owned());
        }
    }

    values
}
