use std::process::ExitCode;
use std::{io::{self, Write}, path::PathBuf};

const PUBLIC_COMMANDS: &str = "\
Dembly development environment orchestrator

Usage: dembly <COMMAND>

Commands:
  validate  Validate a Deck without creating a Runtime
  lock      Generate or update deck.lock
  up        Start a persistent Runtime
  down      Stop a Dembly-managed Runtime
  run       Run a command in a temporary Runtime
  exec      Execute a command in a running Runtime
  inspect   Display the resolved Deck plan
  check     Run selected Card checks
  card      Build Card artifacts
  help      Print this message or the help of the given subcommand
";

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(argument) = arguments.first() else {
        print!("{PUBLIC_COMMANDS}");
        return ExitCode::SUCCESS;
    };

    if argument == "--help" || argument == "-h" || argument == "help" {
        print!("{PUBLIC_COMMANDS}");
        return ExitCode::SUCCESS;
    }

    if argument == "card" && arguments.get(1).is_some_and(|subcommand| subcommand == "build") {
        return card_build(&arguments[2..]);
    }

    eprintln!("dembly: command is not implemented yet");
    ExitCode::from(2)
}

fn card_build(arguments: &[String]) -> ExitCode {
    if arguments.len() < 2 {
        eprintln!("dembly card build requires <tool-root> <cards-root>");
        return ExitCode::from(2);
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
            "--name" => { index += 1; name = option_value(arguments, index, "--name"); },
            "--version" => { index += 1; version = option_value(arguments, index, "--version"); },
            "--mount-target" => { index += 1; mount_target = option_value(arguments, index, "--mount-target"); },
            "--path-prepend" => {
                index += 1;
                let Some(value) = option_value(arguments, index, "--path-prepend") else { return ExitCode::from(2); };
                path_prepend.push(value);
            }
            option => { eprintln!("unknown card build option: {option}"); return ExitCode::from(2); }
        }
        index += 1;
    }
    if !non_interactive {
        let default_name = tool_root.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        name = name.or_else(|| prompt("Card name", Some(default_name)));
        version = version.or_else(|| prompt("Version", None));
        let default_mount = name.as_ref().map(|value| format!("/opt/dembly/cards/{value}"));
        mount_target = mount_target.or_else(|| prompt("Mount target", default_mount.as_deref()));
    }
    let request = dembly_card::CardBuildRequest { tool_root, cards_root, name, version, mount_target, path_prepend, non_interactive };
    match dembly_card::build_card(&request) {
        Ok(result) => { println!("built {}", result.card_root.display()); ExitCode::SUCCESS }
        Err(error) => { eprintln!("dembly card build: {error}"); ExitCode::from(2) }
    }
}

fn option_value(arguments: &[String], index: usize, option: &str) -> Option<String> {
    let value = arguments.get(index).cloned();
    if value.is_none() { eprintln!("{option} requires a value"); }
    value
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
    if value.is_empty() { default.map(str::to_owned) } else { Some(value) }
}
