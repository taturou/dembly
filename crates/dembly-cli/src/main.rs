mod commands;

use dembly_cli::{parse_host_command, CliError, HostCommand};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const PUBLIC_COMMANDS: &str = "\
Dembly development environment compiler

Usage: dembly <COMMAND>

Commands:
  init      Create a Dembly configuration interactively
  validate  Validate a Dembly configuration
  lock      Record the resolved configuration in Compose metadata
  apply     Apply Dembly metadata to Compose
  unapply   Remove Dembly metadata from Compose
  inspect   Display the resolved configuration
  check     Verify static Host integrity
  card      Build Card artifacts
  help      Print this message
";

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "__runtime")
    {
        let mut system = dembly_cli::runtime::NativeRuntimeSystem;
        return match dembly_cli::runtime::run_runtime_command(&arguments[1..], &mut system) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("dembly __runtime: {error}");
                ExitCode::from(2)
            }
        };
    }
    if arguments.is_empty()
        || matches!(
            arguments.first().map(String::as_str),
            Some("--help" | "-h" | "help")
        )
    {
        print!("{PUBLIC_COMMANDS}");
        return ExitCode::SUCCESS;
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "--version")
    {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            return report_error(CliError::new(format!(
                "cannot determine current directory: {error}"
            )))
        }
    };
    let command = match parse_host_command(&arguments, &cwd) {
        Ok(command) => command,
        Err(error) => return report_error(error),
    };
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    match dispatch(command, &mut input, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report_error(error),
    }
}

fn dispatch(
    command: HostCommand,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), CliError> {
    match command {
        HostCommand::Init(context) => commands::init::run(&context, input, output),
        HostCommand::Validate(context) => commands::validate::run(&context, output),
        HostCommand::Lock(context) => commands::lock::run(&context, output),
        HostCommand::Apply(context) => commands::apply::run(&context, output),
        HostCommand::Unapply(context) => commands::unapply::run(&context, output),
        HostCommand::Inspect(context) => commands::inspect::run(&context, output),
        HostCommand::Check(context) => commands::check::run(&context, output),
        HostCommand::CardBuild(arguments) => card_build(&arguments, input, output),
    }
}

fn report_error(error: CliError) -> ExitCode {
    eprintln!("dembly: {error}");
    ExitCode::from(2)
}

fn card_build(
    arguments: &[String],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), CliError> {
    if arguments.len() < 2 {
        return Err(CliError::new(
            "card build requires <tool-root> <cards-root>",
        ));
    }
    let tool_root = PathBuf::from(&arguments[0]);
    let cards_root = PathBuf::from(&arguments[1]);
    let mut name = None;
    let mut version = None;
    let mut mount_target = None;
    let mut path_prepend = Vec::new();
    let mut non_interactive = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--non-interactive" => non_interactive = true,
            "--name" => {
                index += 1;
                name = Some(required_option(arguments, index, "--name")?);
            }
            "--version" => {
                index += 1;
                version = Some(required_option(arguments, index, "--version")?);
            }
            "--mount-target" => {
                index += 1;
                mount_target = Some(required_option(arguments, index, "--mount-target")?);
            }
            "--path-prepend" => {
                index += 1;
                path_prepend.push(required_option(arguments, index, "--path-prepend")?);
            }
            option => {
                return Err(CliError::new(format!(
                    "unknown card build option: {option}"
                )))
            }
        }
        index += 1;
    }
    if !non_interactive {
        let default_name = tool_root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        name = name.or_else(|| prompt(input, output, "Card name", Some(default_name)));
        version = version.or_else(|| prompt(input, output, "Version", None));
        let default_mount = name
            .as_ref()
            .map(|value| format!("/opt/dembly/cards/{value}"));
        mount_target = mount_target
            .or_else(|| prompt(input, output, "Mount target", default_mount.as_deref()));
    }
    let request = dembly_card::CardBuildRequest {
        tool_root,
        cards_root,
        name,
        version,
        mount_target,
        path_prepend,
        non_interactive,
    };
    let result = dembly_card::build_card(&request)
        .map_err(|error| CliError::new(format!("card build: {error}")))?;
    writeln!(output, "built {}", result.card_root.display())
        .map_err(|error| CliError::new(format!("cannot write card build result: {error}")))?;
    writeln!(output, "sha256: {}", result.sha256)
        .map_err(|error| CliError::new(format!("cannot write card build result: {error}")))?;
    Ok(())
}

fn required_option(arguments: &[String], index: usize, option: &str) -> Result<String, CliError> {
    arguments
        .get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| CliError::new(format!("{option} requires a value")))
}

fn prompt(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
    default: Option<&str>,
) -> Option<String> {
    match default {
        Some(default) => write!(output, "{label} [{default}]: ").ok()?,
        None => write!(output, "{label}: ").ok()?,
    }
    output.flush().ok()?;
    let mut value = String::new();
    input.read_line(&mut value).ok()?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        default.map(str::to_owned)
    } else {
        Some(value)
    }
}
