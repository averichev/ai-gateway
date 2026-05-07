use std::{
    ffi::OsStr,
    process::{Command, Stdio},
};

use crate::ctl::{CtlError, Result, config::CtlConfig, linux::LinuxPlatform};

#[derive(Debug, Clone)]
pub struct DockerClient;

impl DockerClient {
    pub fn new() -> Self {
        Self
    }

    pub fn is_installed(&self) -> bool {
        Command::new("docker")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub fn version(&self) -> Result<String> {
        let output = run_command("docker", ["--version"])?;
        Ok(output.stdout.trim().to_owned())
    }

    pub fn daemon_ready(&self) -> Result<String> {
        let output = run_command("docker", ["info", "--format", "{{.ServerVersion}}"])?;
        Ok(output.stdout.trim().to_owned())
    }

    pub fn ensure_installed(&self, platform: &LinuxPlatform) -> Result<()> {
        if self.is_installed() {
            let _ = self.daemon_ready()?;
            return Ok(());
        }

        if !platform.is_supported_for_docker_install() {
            return Err(CtlError::UnsupportedOs(format!(
                "автоматическая установка Docker поддерживается только на Debian/Ubuntu, обнаружено: {}",
                platform.pretty_name
            )));
        }

        let sudo = sudo_prefix()?;
        let install_script = format!(
            "{sudo}apt-get update && {sudo}apt-get install -y ca-certificates curl && curl -fsSL https://get.docker.com | {sudo}sh"
        );
        run_shell(&install_script)?;

        let start_script = format!("{sudo}systemctl enable --now docker");
        let _ = run_shell(&start_script);

        let _ = self.daemon_ready()?;
        Ok(())
    }

    pub fn inspect_remote_image(&self, image: &str) -> Result<()> {
        let _ = run_command("docker", ["manifest", "inspect", image])?;
        Ok(())
    }

    pub fn pull_image(&self, image: &str) -> Result<()> {
        let _ = run_command("docker", ["pull", image])?;
        Ok(())
    }

    pub fn container_status(&self, name: &str) -> Result<Option<String>> {
        inspect_field(name, "{{.State.Status}}")
    }

    pub fn container_image(&self, name: &str) -> Result<Option<String>> {
        inspect_field(name, "{{.Config.Image}}")
    }

    pub fn published_port(&self, name: &str, container_port: u16) -> Result<Option<String>> {
        let port_spec = format!("{container_port}/tcp");
        match run_command("docker", ["port", name, &port_spec]) {
            Ok(output) => Ok(Some(output.stdout.trim().to_owned())),
            Err(CtlError::CommandFailed { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn remove_container_if_exists(&self, name: &str) -> Result<bool> {
        match run_command("docker", ["rm", "-f", name]) {
            Ok(_) => Ok(true),
            Err(CtlError::CommandFailed { stderr, stdout, .. })
                if stderr.contains("No such container") || stdout.contains("No such container") =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    pub fn run_gateway_container(&self, config: &CtlConfig) -> Result<()> {
        let port_mapping = format!("{}:8080", config.published_port);
        let volume_mapping = format!("{}:/var/lib/postgresql/data", config.volume_name);

        let args = vec![
            "run".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            config.container_name.clone(),
            "--restart".to_owned(),
            "unless-stopped".to_owned(),
            "--env-file".to_owned(),
            config.env_file.display().to_string(),
            "-p".to_owned(),
            port_mapping,
            "-v".to_owned(),
            volume_mapping,
            config.image.clone(),
        ];

        let _ = run_command_owned("docker", args)?;
        Ok(())
    }

    pub fn restart_container(&self, name: &str) -> Result<()> {
        let _ = run_command("docker", ["restart", name])?;
        Ok(())
    }

    pub fn stream_logs(&self, name: &str, follow: bool, tail: usize) -> Result<()> {
        let mut args = vec!["logs".to_owned(), "--tail".to_owned(), tail.to_string()];

        if follow {
            args.push("-f".to_owned());
        }

        args.push(name.to_owned());

        let status = Command::new("docker").args(args).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(CtlError::CommandFailed {
                command: "docker logs".to_owned(),
                status: status.code(),
                stdout: String::new(),
                stderr: "не удалось получить логи docker".to_owned(),
                details: "не удалось получить логи docker".to_owned(),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn run_shell(script: &str) -> Result<CommandOutput> {
    run_command_owned("sh", vec!["-lc".to_owned(), script.to_owned()])
}

pub fn run_command<I, S>(program: &str, args: I) -> Result<CommandOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program).args(args).output()?;
    build_output(program, output)
}

pub fn run_command_owned(program: &str, args: Vec<String>) -> Result<CommandOutput> {
    let output = Command::new(program).args(args).output()?;
    build_output(program, output)
}

fn build_output(program: &str, output: std::process::Output) -> Result<CommandOutput> {
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    if output.status.success() {
        Ok(CommandOutput { stdout, stderr })
    } else {
        let details = if !stderr.trim().is_empty() {
            stderr.trim().to_owned()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_owned()
        } else {
            "команда завершилась с ошибкой".to_owned()
        };

        Err(CtlError::CommandFailed {
            command: program.to_owned(),
            status: output.status.code(),
            stdout,
            stderr,
            details,
        })
    }
}

fn inspect_field(name: &str, format: &str) -> Result<Option<String>> {
    match run_command("docker", ["inspect", "-f", format, name]) {
        Ok(output) => Ok(Some(output.stdout.trim().to_owned())),
        Err(CtlError::CommandFailed { stderr, stdout, .. })
            if stderr.contains("No such object") || stdout.contains("No such object") =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn sudo_prefix() -> Result<&'static str> {
    let output = run_command("id", ["-u"])?;

    if output.stdout.trim() == "0" {
        Ok("")
    } else {
        Ok("sudo ")
    }
}
