use std::{fs, path::Path, process::Stdio, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::timeout,
};

fn server_command(directory: &Path, master_key: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ai-gateway"));
    command
        .current_dir(directory)
        .env_remove("DATABASE_URL")
        .env_remove("GATEWAY_MASTER_KEY")
        .env("GATEWAY_UPDATE_SOURCE", "")
        .env("APP_HOST", "127.0.0.1")
        .env("PORT", "0")
        .env("RUST_LOG", "ai_gateway=info")
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(key) = master_key {
        command.env("GATEWAY_MASTER_KEY", key);
    }
    command
}

async fn start_server(directory: &Path, master_key: Option<&str>) -> (Child, String) {
    let mut child = server_command(directory, master_key).spawn().unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let url = timeout(Duration::from_secs(15), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some((_, port)) = line.split_once("127.0.0.1:") {
                let digits: String = port.chars().take_while(char::is_ascii_digit).collect();
                let port: u16 = digits.parse().unwrap();
                return format!("http://127.0.0.1:{port}");
            }
        }
        panic!("gateway exited before listening");
    })
    .await
    .unwrap();
    (child, url)
}

#[tokio::test]
async fn standalone_reopens_database_and_encrypted_secrets_without_environment_keys() {
    let directory = tempfile::tempdir().unwrap();
    let client = Client::new();
    let (mut first, url) = start_server(directory.path(), None).await;
    let account: Value = client
        .post(format!("{url}/api/auth/register"))
        .json(&json!({"email":"sqlite@example.test","password":"sqlite-test-password"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(account["user"]["id"].is_string());
    assert!(directory.path().join("ai-gateway.sqlite3").is_file());
    let key_file = directory.path().join("ai-gateway.key");
    let saved_key = fs::read_to_string(&key_file).unwrap();
    let token = account["token"].as_str().unwrap();
    let tenant = format!(
        "/api/admin/tenants/{}",
        account["tenants"][0]["id"].as_str().unwrap()
    );
    for (endpoint, payload) in [
        (
            "providers",
            json!({"code":"local","kind":"openai-compatible","base_url":"https://example.invalid/v1","api_key":"standalone-test","timeout_ms":1000}),
        ),
        (
            "model-routes",
            json!({"alias":"test-route","provider_code":"local","external_model":"test-model"}),
        ),
    ] {
        client
            .post(format!("{url}{tenant}/{endpoint}"))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }
    first.kill().await.unwrap();

    let (mut second, url) = start_server(directory.path(), None).await;
    let me: Value = client
        .get(format!("{url}/api/auth/me"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["user"]["id"], account["user"]["id"]);
    let readiness: Value = client
        .get(format!("{url}{tenant}/model-routes/test-route/readiness"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(readiness["status"], "ready");
    assert!(fs::read_to_string(&key_file).unwrap() == saved_key);
    second.kill().await.unwrap();

    fs::remove_file(&key_file).unwrap();
    let error = startup_error(directory.path(), None).await;
    assert!(error.contains("Restore ai-gateway.key"));
    assert!(!key_file.exists());
    let (mut recovered, url) = start_server(directory.path(), Some(saved_key.trim())).await;
    let recovered_route: Value = client
        .get(format!("{url}{tenant}/model-routes/test-route/readiness"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recovered_route["status"], "ready");
    assert!(!key_file.exists());
    recovered.kill().await.unwrap();
}

async fn startup_error(directory: &Path, master_key: Option<&str>) -> String {
    let output = timeout(
        Duration::from_secs(15),
        server_command(directory, master_key).output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!output.status.success());
    String::from_utf8(output.stderr).unwrap()
}

#[tokio::test]
async fn explicit_key_takes_priority_and_invalid_configuration_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let key_file = directory.path().join("ai-gateway.key");
    fs::write(&key_file, "invalid file").unwrap();
    let key = STANDARD.encode([3u8; 32]);
    let (mut server, _) = start_server(directory.path(), Some(&key)).await;
    server.kill().await.unwrap();
    assert_eq!(fs::read_to_string(&key_file).unwrap(), "invalid file");

    fs::write(&key_file, &key).unwrap();
    for invalid in ["", "invalid-env-secret"] {
        let error = startup_error(directory.path(), Some(invalid)).await;
        assert!(error.contains("GATEWAY_MASTER_KEY must be"));
        assert!(!error.contains("invalid-env-secret"));
        assert!(fs::read_to_string(&key_file).unwrap() == key);
    }
}
