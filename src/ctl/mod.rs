pub mod cli;
pub mod config;
pub mod docker;
pub mod env_file;
pub mod linux;
pub mod output;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::Parser;
use thiserror::Error;
use tokio::time::sleep;

use self::{
    cli::{Cli, Command, DockerCommand, EnvCommand, ImageCommand},
    config::{CtlConfig, DEFAULT_CONFIG_PATH},
    docker::DockerClient,
    env_file::{EnvFile, EnvValueStatus},
    linux::LinuxPlatform,
    output::{
        CheckState, print_banner, print_check_table, print_error, print_info,
        print_key_value_table, print_section, print_success, print_warn,
    },
};

pub type Result<T> = std::result::Result<T, CtlError>;

#[derive(Debug, Error)]
pub enum CtlError {
    #[error("{0}")]
    Parse(String),
    #[error("{0}")]
    Validation(String),
    #[error("неподдерживаемая платформа: {0}")]
    UnsupportedOs(String),
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),
    #[error("не удалось разобрать конфиг: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("не удалось записать конфиг: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("HTTP-ошибка: {0}")]
    Http(#[from] reqwest::Error),
    #[error("ошибка декодирования UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("конфиг `{0}` не найден: {1}")]
    MissingConfig(PathBuf, std::io::Error),
    #[error("env-файл `{0}` не найден: {1}")]
    MissingEnv(PathBuf, std::io::Error),
    #[error("команда `{command}` завершилась с ошибкой (status {status:?}): {details}")]
    CommandFailed {
        command: String,
        status: Option<i32>,
        stdout: String,
        stderr: String,
        details: String,
    },
}

pub async fn run_from_env() -> Result<()> {
    let cli = Cli::parse();
    run(cli).await
}

pub async fn run(cli: Cli) -> Result<()> {
    print_banner();

    match cli.command {
        Command::Doctor => doctor(&cli.config).await,
        Command::Init(args) => init(&cli.config, args.app_dir, args.image, args.force),
        Command::Docker { command } => match command {
            DockerCommand::Ensure => docker_ensure(),
        },
        Command::Env { command } => match command {
            EnvCommand::Show => env_show(&cli.config),
            EnvCommand::Set { key, value } => env_set(&cli.config, &key, &value),
            EnvCommand::Validate => env_validate(&cli.config),
        },
        Command::Image { command } => match command {
            ImageCommand::Show => image_show(&cli.config),
            ImageCommand::Set { reference } => image_set(&cli.config, &reference),
        },
        Command::Apply => apply(&cli.config).await,
        Command::Restart => restart(&cli.config).await,
        Command::Status => status(&cli.config).await,
        Command::Logs(args) => logs(&cli.config, args.follow, args.tail),
    }
}

pub fn print_fatal(error: &dyn std::fmt::Display) {
    print_error(&error.to_string());
}

fn init(
    config_path: &Path,
    app_dir: Option<PathBuf>,
    image: Option<String>,
    force: bool,
) -> Result<()> {
    let app_dir = app_dir.unwrap_or_else(|| {
        config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/opt/ai-gateway"))
    });

    fs::create_dir_all(&app_dir)?;

    let config = CtlConfig::with_app_dir(app_dir.clone(), image);

    if force || !config_path.exists() {
        config.save(config_path)?;
        print_success(&format!("Конфиг записан в {}", config_path.display()));
    } else {
        print_warn(&format!(
            "Конфиг уже существует по пути {}, оставляю текущий файл",
            config_path.display()
        ));
    }

    if force || !config.env_file.exists() {
        let env = EnvFile::from_template()?;
        env.save(&config.env_file)?;
        print_success(&format!(
            "Шаблон env записан в {}",
            config.env_file.display()
        ));
    } else {
        print_warn(&format!(
            "Env-файл уже существует по пути {}, оставляю текущий файл",
            config.env_file.display()
        ));
    }

    print_info("Дальше:");
    print_info(&format!("1. Отредактируй {}", config.env_file.display()));
    print_info("2. Запусти `agctl env validate`");
    print_info("3. Запусти `agctl apply`");

    Ok(())
}

fn env_show(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let env = EnvFile::load(&config.env_file)?;
    print_key_value_table("Текущее окружение", &env.safe_rows());
    Ok(())
}

fn env_set(config_path: &Path, key: &str, value: &str) -> Result<()> {
    let config = load_config(config_path)?;
    let mut env = EnvFile::load(&config.env_file)?;
    env.set(key.to_owned(), value.to_owned());
    env.save(&config.env_file)?;
    print_success(&format!(
        "Переменная `{key}` обновлена в {}",
        config.env_file.display()
    ));
    Ok(())
}

fn env_validate(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let env = EnvFile::load(&config.env_file)?;
    let rows = env
        .validation_rows()
        .into_iter()
        .map(|row| {
            (
                row.key,
                match row.status {
                    EnvValueStatus::Present => CheckState::Ok,
                    EnvValueStatus::Missing | EnvValueStatus::Placeholder => CheckState::Fail,
                },
                row.safe_value,
            )
        })
        .collect::<Vec<_>>();

    print_check_table("Проверка окружения", &rows);

    let missing = env.missing_required_keys();
    let placeholders = env.placeholder_required_keys();

    if missing.is_empty() && placeholders.is_empty() {
        print_success("Env-файл выглядит корректно");
        Ok(())
    } else {
        let mut problems = Vec::new();
        if !missing.is_empty() {
            problems.push(format!("отсутствуют: {}", missing.join(", ")));
        }
        if !placeholders.is_empty() {
            problems.push(format!("шаблонные значения: {}", placeholders.join(", ")));
        }

        Err(CtlError::Validation(format!(
            "проверка окружения не пройдена: {}",
            problems.join("; ")
        )))
    }
}

fn image_show(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    print_key_value_table(
        "Текущий image",
        &[
            ("Image".to_owned(), config.image),
            ("Контейнер".to_owned(), config.container_name),
            ("Порт".to_owned(), config.published_port.to_string()),
        ],
    );
    Ok(())
}

fn image_set(config_path: &Path, reference: &str) -> Result<()> {
    let mut config = load_config(config_path)?;
    config.image = reference.to_owned();
    config.save(config_path)?;
    print_success(&format!("Image обновлён: `{reference}`"));
    Ok(())
}

fn logs(config_path: &Path, follow: bool, tail: usize) -> Result<()> {
    let config = load_config(config_path)?;
    let docker = DockerClient::new();
    docker.stream_logs(&config.container_name, follow, tail)
}

fn docker_ensure() -> Result<()> {
    let docker = DockerClient::new();

    if docker.is_installed() {
        let version = docker.version()?;
        let daemon = docker.daemon_ready()?;
        print_success(&format!("Docker уже установлен: {version}"));
        print_info(&format!("Docker daemon доступен: {daemon}"));
        return Ok(());
    }

    let platform = LinuxPlatform::detect().ok_or_else(|| {
        CtlError::UnsupportedOs(
            "не удалось определить Linux-платформу по /etc/os-release".to_owned(),
        )
    })?;

    print_info(&format!(
        "Docker не установлен, пробую установить на {}",
        platform.pretty_name
    ));

    docker.ensure_installed(&platform)?;
    let version = docker.version()?;
    print_success(&format!("Docker успешно установлен: {version}"));
    Ok(())
}

async fn doctor(config_path: &Path) -> Result<()> {
    let docker = DockerClient::new();
    let platform = LinuxPlatform::detect();
    let config = CtlConfig::load_if_exists(config_path)?;

    let mut rows = Vec::new();

    if let Some(platform) = platform.clone() {
        rows.push((
            "Linux-платформа".to_owned(),
            CheckState::Info,
            match platform.version_id {
                Some(version_id) => format!("{} ({})", platform.pretty_name, version_id),
                None => platform.pretty_name,
            },
        ));
    } else {
        rows.push((
            "Linux-платформа".to_owned(),
            CheckState::Warn,
            "Не удалось прочитать /etc/os-release".to_owned(),
        ));
    }

    if docker.is_installed() {
        rows.push(("Docker CLI".to_owned(), CheckState::Ok, docker.version()?));
        match docker.daemon_ready() {
            Ok(version) => rows.push((
                "Docker daemon".to_owned(),
                CheckState::Ok,
                format!("Версия сервера {version}"),
            )),
            Err(error) => rows.push((
                "Docker daemon".to_owned(),
                CheckState::Fail,
                error.to_string(),
            )),
        }
    } else {
        rows.push((
            "Docker CLI".to_owned(),
            CheckState::Fail,
            "Docker не установлен".to_owned(),
        ));
    }

    let config = if let Some(config) = config {
        rows.push((
            "Конфиг agctl".to_owned(),
            CheckState::Ok,
            config_path.display().to_string(),
        ));
        Some(config)
    } else {
        rows.push((
            "Конфиг agctl".to_owned(),
            CheckState::Fail,
            format!("Не найден конфиг {}", config_path.display()),
        ));
        None
    };

    if let Some(config) = &config {
        if config.env_file.exists() {
            rows.push((
                "Env-файл".to_owned(),
                CheckState::Ok,
                config.env_file.display().to_string(),
            ));

            let env = EnvFile::load(&config.env_file)?;
            let missing = env.missing_required_keys();
            let placeholders = env.placeholder_required_keys();
            if missing.is_empty() && placeholders.is_empty() {
                rows.push((
                    "Обязательные env-ключи".to_owned(),
                    CheckState::Ok,
                    "Все обязательные переменные заполнены".to_owned(),
                ));
            } else {
                let mut problems = Vec::new();
                if !missing.is_empty() {
                    problems.push(format!("отсутствуют: {}", missing.join(", ")));
                }
                if !placeholders.is_empty() {
                    problems.push(format!("шаблонные значения: {}", placeholders.join(", ")));
                }
                rows.push((
                    "Обязательные env-ключи".to_owned(),
                    CheckState::Fail,
                    problems.join("; "),
                ));
            }
        } else {
            rows.push((
                "Env-файл".to_owned(),
                CheckState::Fail,
                format!("Не найден env-файл {}", config.env_file.display()),
            ));
        }

        if docker.is_installed() {
            match docker.inspect_remote_image(&config.image) {
                Ok(()) => rows.push((
                    "Текущий image".to_owned(),
                    CheckState::Ok,
                    config.image.clone(),
                )),
                Err(error) => rows.push((
                    "Текущий image".to_owned(),
                    CheckState::Warn,
                    format!("Не удалось проверить {}: {}", config.image, error),
                )),
            }
        } else {
            rows.push((
                "Текущий image".to_owned(),
                CheckState::Warn,
                config.image.clone(),
            ));
        }
    }

    print_check_table("Готовность сервера", &rows);

    let has_failure = rows
        .iter()
        .any(|(_, state, _)| matches!(state, CheckState::Fail));

    if has_failure {
        Err(CtlError::Validation(
            "doctor нашёл блокирующие проблемы".to_owned(),
        ))
    } else {
        print_success("Сервер готов к применению конфигурации");
        Ok(())
    }
}

async fn apply(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let docker = DockerClient::new();

    if !docker.is_installed() {
        return Err(CtlError::Validation(
            "Docker не установлен, сначала запусти `agctl docker ensure`".to_owned(),
        ));
    }

    let _ = docker.daemon_ready()?;
    let env = EnvFile::load(&config.env_file)?;
    let missing = env.missing_required_keys();
    let placeholders = env.placeholder_required_keys();
    if !missing.is_empty() || !placeholders.is_empty() {
        let mut problems = Vec::new();
        if !missing.is_empty() {
            problems.push(format!("отсутствуют env-ключи: {}", missing.join(", ")));
        }
        if !placeholders.is_empty() {
            problems.push(format!(
                "остались шаблонные значения: {}",
                placeholders.join(", ")
            ));
        }
        return Err(CtlError::Validation(format!(
            "не удалось применить конфигурацию: {}",
            problems.join("; ")
        )));
    }

    print_section("Применение конфигурации");
    print_info(&format!("Скачиваю image {}", config.image));
    docker.pull_image(&config.image)?;

    if docker.remove_container_if_exists(&config.container_name)? {
        print_info(&format!(
            "Удалён предыдущий контейнер {}",
            config.container_name
        ));
    }

    print_info(&format!(
        "Запускаю контейнер {} на порту {}",
        config.container_name, config.published_port
    ));
    docker.run_gateway_container(&config)?;

    wait_for_healthcheck(&config).await?;
    print_success("Конфигурация применена, healthcheck пройден");
    Ok(())
}

async fn restart(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let docker = DockerClient::new();
    docker.restart_container(&config.container_name)?;
    wait_for_healthcheck(&config).await?;
    print_success("Контейнер перезапущен, healthcheck пройден");
    Ok(())
}

async fn status(config_path: &Path) -> Result<()> {
    let config = load_config(config_path)?;
    let docker = DockerClient::new();

    let mut rows = vec![
        ("Конфиг".to_owned(), config_path.display().to_string()),
        (
            "Рабочая директория".to_owned(),
            config.app_dir.display().to_string(),
        ),
        ("Env-файл".to_owned(), config.env_file.display().to_string()),
        ("Image".to_owned(), config.image.clone()),
        ("Контейнер".to_owned(), config.container_name.clone()),
        (
            "Опубликованный порт".to_owned(),
            config.published_port.to_string(),
        ),
        ("Volume".to_owned(), config.volume_name.clone()),
    ];

    let container_status = docker.container_status(&config.container_name)?;
    rows.push((
        "Статус контейнера".to_owned(),
        container_status
            .clone()
            .unwrap_or_else(|| "не создан".to_owned()),
    ));

    if let Some(image) = docker.container_image(&config.container_name)? {
        rows.push(("Запущенный image".to_owned(), image));
    }

    if let Some(mapping) = docker.published_port(&config.container_name, 8080)? {
        rows.push(("Проброс порта".to_owned(), mapping));
    }

    let health = match check_health(&config).await {
        Ok(status) => format!("ok ({status})"),
        Err(error) => format!("ошибка ({error})"),
    };
    rows.push(("Healthcheck".to_owned(), health));

    print_key_value_table("Текущее состояние", &rows);
    Ok(())
}

fn load_config(config_path: &Path) -> Result<CtlConfig> {
    if config_path == Path::new(DEFAULT_CONFIG_PATH) && !config_path.exists() {
        return Err(CtlError::Validation(format!(
            "конфиг отсутствует, запусти `agctl init`, чтобы создать {}",
            config_path.display()
        )));
    }

    CtlConfig::load(config_path)
}

async fn wait_for_healthcheck(config: &CtlConfig) -> Result<()> {
    let started = Instant::now();
    let timeout = Duration::from_secs(30);

    loop {
        if let Ok(status) = check_health(config).await {
            print_info(&format!("Health endpoint ответил: {status}"));
            return Ok(());
        }

        if started.elapsed() >= timeout {
            return Err(CtlError::Validation(format!(
                "healthcheck не прошёл за {} сек. по адресу {}",
                timeout.as_secs(),
                config.healthcheck_url()
            )));
        }

        sleep(Duration::from_secs(1)).await;
    }
}

async fn check_health(config: &CtlConfig) -> Result<String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(config.healthcheck_url())
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        Ok(body)
    } else {
        Err(CtlError::Validation(format!(
            "healthcheck вернул HTTP {}",
            status.as_u16()
        )))
    }
}
