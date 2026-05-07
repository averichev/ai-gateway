use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "agctl",
    version,
    about = "Утилита управления production-сервером AI Gateway",
    long_about = "agctl запускается прямо на VPS и управляет Docker, образом приложения, .env и проверками состояния ai-gateway.",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "/opt/ai-gateway/ctl.toml",
        help = "Путь к конфигурации agctl"
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum Command {
    /// Проверить готовность сервера, Docker, env и image
    Doctor,
    /// Создать конфиг agctl и шаблон .env
    Init(InitArgs),
    /// Операции с Docker на сервере
    Docker {
        #[command(subcommand)]
        command: DockerCommand,
    },
    /// Управление переменными окружения приложения
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    /// Управление образом приложения
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
    /// Применить текущий image и конфиг к серверу
    Apply,
    /// Перезапустить текущий контейнер
    Restart,
    /// Показать состояние контейнера и healthcheck
    Status,
    /// Показать логи контейнера
    Logs(LogsArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(
        long,
        value_name = "DIR",
        help = "Рабочая директория приложения на сервере"
    )]
    pub app_dir: Option<PathBuf>,
    #[arg(long, value_name = "IMAGE[:TAG]", help = "Docker image по умолчанию")]
    pub image: Option<String>,
    #[arg(long, help = "Перезаписать существующие ctl.toml и .env")]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum DockerCommand {
    /// Проверить Docker и установить его на Debian/Ubuntu при необходимости
    Ensure,
}

#[derive(Debug, Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum EnvCommand {
    /// Показать текущий .env в безопасном виде
    Show,
    /// Установить или обновить одну переменную окружения
    Set {
        #[arg(value_name = "KEY", help = "Имя переменной")]
        key: String,
        #[arg(value_name = "VALUE", help = "Значение переменной")]
        value: String,
    },
    /// Проверить обязательные переменные окружения
    Validate,
}

#[derive(Debug, Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum ImageCommand {
    /// Показать текущий image
    Show,
    /// Обновить image в конфиге
    Set {
        #[arg(value_name = "IMAGE[:TAG]", help = "Новый Docker image")]
        reference: String,
    },
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(short = 'f', long, help = "Следить за логами в реальном времени")]
    pub follow: bool,
    #[arg(long, default_value_t = 200, help = "Сколько последних строк показать")]
    pub tail: usize,
}
