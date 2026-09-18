use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostContext {
    pub cwd: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostCommand {
    Init(HostContext),
    Validate(HostContext),
    Lock(HostContext),
    Apply(HostContext),
    Unapply(HostContext),
    Inspect(HostContext),
    Check(HostContext),
    CardBuild(Vec<String>),
}

#[derive(Debug)]
pub struct CliError {
    message: String,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

pub fn parse_host_command(arguments: &[String], cwd: &Path) -> Result<HostCommand, CliError> {
    let Some(command) = arguments.first() else {
        return Err(CliError::new("a command is required"));
    };
    match command.as_str() {
        "init" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Init),
        "validate" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Validate),
        "lock" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Lock),
        "apply" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Apply),
        "unapply" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Unapply),
        "inspect" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Inspect),
        "check" => parse_config_command(&arguments[1..], cwd).map(HostCommand::Check),
        "card" => parse_card_command(&arguments[1..]),
        _ => Err(CliError::new(format!("unknown command: {command}"))),
    }
}

fn parse_config_command(arguments: &[String], cwd: &Path) -> Result<HostContext, CliError> {
    let mut config = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--config" => {
                if config.is_some() {
                    return Err(CliError::new("--config may be specified only once"));
                }
                index += 1;
                let Some(value) = arguments.get(index) else {
                    return Err(CliError::new("--config requires a path"));
                };
                if value.starts_with('-') {
                    return Err(CliError::new("--config requires a path"));
                }
                config = Some(PathBuf::from(value));
            }
            option if option.starts_with('-') => {
                return Err(CliError::new(format!("unknown option: {option}")));
            }
            positional => {
                return Err(CliError::new(format!(
                    "unexpected positional argument: {positional}"
                )));
            }
        }
        index += 1;
    }
    let cwd = cwd.to_path_buf();
    let config_path = match config {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd.join(".dembly/config.toml"),
    };
    Ok(HostContext { cwd, config_path })
}

fn parse_card_command(arguments: &[String]) -> Result<HostCommand, CliError> {
    let Some(subcommand) = arguments.first() else {
        return Err(CliError::new("card requires a subcommand"));
    };
    if subcommand != "build" {
        return Err(CliError::new(format!("unknown card command: {subcommand}")));
    }
    Ok(HostCommand::CardBuild(arguments[1..].to_vec()))
}
