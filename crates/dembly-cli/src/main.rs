mod args;
mod commands;

use args::{parse_host_command, CliError, HostCommand};
use std::io::{self, BufRead, Write};
use std::os::unix::process::CommandExt;
use std::process::ExitCode;
use std::{fs, path::PathBuf};

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
  check     Run Card checks
  card      Build Card artifacts
  help      Print this message
";

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|argument| argument == "__runtime")
    {
        return runtime_command(&arguments[1..]);
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
        HostCommand::CardBuild(arguments) => card_build(&arguments),
        HostCommand::Validate(_)
        | HostCommand::Lock(_)
        | HostCommand::Apply(_)
        | HostCommand::Unapply(_)
        | HostCommand::Inspect(_)
        | HostCommand::Check(_) => Err(CliError::new("command is not implemented yet")),
    }
}

fn report_error(error: CliError) -> ExitCode {
    eprintln!("dembly: {error}");
    ExitCode::from(2)
}

fn card_build(arguments: &[String]) -> Result<(), CliError> {
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
        name = name.or_else(|| prompt("Card name", Some(default_name)));
        version = version.or_else(|| prompt("Version", None));
        let default_mount = name
            .as_ref()
            .map(|value| format!("/opt/dembly/cards/{value}"));
        mount_target = mount_target.or_else(|| prompt("Mount target", default_mount.as_deref()));
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
    println!("built {}", result.card_root.display());
    Ok(())
}

fn required_option(arguments: &[String], index: usize, option: &str) -> Result<String, CliError> {
    arguments
        .get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| CliError::new(format!("{option} requires a value")))
}

fn prompt(label: &str, default: Option<&str>) -> Option<String> {
    match default {
        Some(default) => print!("{label} [{default}]: "),
        None => print!("{label}: "),
    }
    io::stdout().flush().ok()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value).ok()?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        default.map(str::to_owned)
    } else {
        Some(value)
    }
}

fn runtime_command(arguments: &[String]) -> ExitCode {
    if arguments.len() == 2 && arguments[0] == "probe" {
        return runtime_probe(&arguments[1]);
    }
    if arguments.len() != 2 || arguments[0] != "init" {
        eprintln!("dembly: invalid internal runtime command");
        return ExitCode::from(2);
    }
    if unsafe { libc_geteuid() } != 0 {
        eprintln!("dembly runtime: initializer must run as root");
        return ExitCode::from(2);
    }
    let config = match dembly_runtime::load_runtime_config(&PathBuf::from(&arguments[1])) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("dembly runtime: {error}");
            return ExitCode::from(2);
        }
    };
    for card in &config.cards {
        if let Err(error) = dembly_runtime::mount_card(card) {
            eprintln!("dembly runtime: {error}");
            return ExitCode::from(2);
        }
    }
    if let Err(error) = dembly_runtime::ensure_mount_targets_exist(&config.cards) {
        eprintln!("dembly runtime: {error}");
        return ExitCode::from(2);
    }
    if let Err(error) = dembly_runtime::create_exports(&config.exports) {
        eprintln!("dembly runtime: {error}");
        return ExitCode::from(2);
    }
    if let Err(error) = dembly_runtime::run_hooks(&config.hooks, &config.cards, &config.environment)
    {
        eprintln!("dembly runtime: {error}");
        return ExitCode::from(2);
    }
    if let Err(error) = drop_privileges(config.runtime_user.uid, config.runtime_user.gid) {
        eprintln!("dembly runtime: {error}");
        return ExitCode::from(2);
    }
    let Some(program) = config.process_argv.first() else {
        eprintln!("dembly runtime: process argv must not be empty");
        return ExitCode::from(2);
    };
    let error = std::process::Command::new(program)
        .args(&config.process_argv[1..])
        .envs(&config.environment)
        .exec();
    eprintln!("dembly runtime: cannot exec {program}: {error}");
    ExitCode::from(2)
}

fn runtime_probe(configured: &str) -> ExitCode {
    let mut configured_parts = configured.split(':');
    let candidate = configured_parts.next().unwrap_or_default();
    let configured_group = configured_parts.next();
    if configured_parts.next().is_some() {
        eprintln!("dembly runtime probe: invalid configured user {configured}");
        return ExitCode::from(2);
    }
    let passwd = match fs::read_to_string("/etc/passwd") {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly runtime probe: cannot read /etc/passwd: {error}");
            return ExitCode::from(2);
        }
    };
    for line in passwd.lines() {
        let fields = line.split(':').collect::<Vec<_>>();
        if fields.len() < 7 || (fields[0] != candidate && fields[2] != candidate) {
            continue;
        }
        if fields[2].parse::<u32>().is_err() || fields[3].parse::<u32>().is_err() {
            continue;
        }
        let gid = match configured_group.filter(|value| !value.is_empty()) {
            None => fields[3].to_owned(),
            Some(value) if value.parse::<u32>().is_ok() => value.to_owned(),
            Some(value) => match group_id(value) {
                Some(value) => value,
                None => {
                    eprintln!("dembly runtime probe: cannot resolve configured group {value}");
                    return ExitCode::from(2);
                }
            },
        };
        println!("{}\t{}\t{}\t{}", fields[0], fields[2], gid, fields[5]);
        return ExitCode::SUCCESS;
    }
    eprintln!("dembly runtime probe: cannot resolve configured user {configured} in /etc/passwd");
    ExitCode::from(2)
}

fn group_id(name: &str) -> Option<String> {
    fs::read_to_string("/etc/group")
        .ok()?
        .lines()
        .find_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            (fields.len() >= 3 && fields[0] == name && fields[2].parse::<u32>().is_ok())
                .then(|| fields[2].to_owned())
        })
}

unsafe fn libc_geteuid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

fn drop_privileges(uid: u32, gid: u32) -> Result<(), String> {
    extern "C" {
        fn setgid(gid: u32) -> i32;
        fn setuid(uid: u32) -> i32;
    }
    unsafe {
        if setgid(gid) != 0 {
            return Err(format!("cannot set GID to {gid}"));
        }
        if setuid(uid) != 0 {
            return Err(format!("cannot set UID to {uid}"));
        }
    }
    Ok(())
}
