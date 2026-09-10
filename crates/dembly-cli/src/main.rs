use std::process::ExitCode;
use std::{io::{self, Write}, path::PathBuf};
use std::collections::BTreeSet;
use std::os::unix::process::CommandExt;

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
    if argument == "validate" {
        return validate(&arguments[1..]);
    }
    if argument == "__runtime" {
        return runtime_command(&arguments[1..]);
    }

    eprintln!("dembly: command is not implemented yet");
    ExitCode::from(2)
}

fn runtime_command(arguments: &[String]) -> ExitCode {
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
        Err(error) => { eprintln!("dembly runtime: {error}"); return ExitCode::from(2); }
    };
    for card in &config.cards {
        if let Err(error) = dembly_runtime::mount_card(card) {
            eprintln!("dembly runtime: {error}");
            return ExitCode::from(2);
        }
    }
    let Some(program) = config.process_argv.first() else {
        eprintln!("dembly runtime: process argv must not be empty");
        return ExitCode::from(2);
    };
    let error = std::process::Command::new(program).args(&config.process_argv[1..]).exec();
    eprintln!("dembly runtime: cannot exec {program}: {error}");
    ExitCode::from(2)
}

#[cfg(target_os = "linux")]
unsafe fn libc_geteuid() -> u32 {
    // SAFETY: geteuid has no arguments, no memory ownership contract, and is available on Linux.
    extern "C" { fn geteuid() -> u32; }
    unsafe { geteuid() }
}

fn validate(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly validate accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => { eprintln!("dembly validate: cannot determine current directory: {error}"); return ExitCode::from(2); }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(path) => path,
        Err(error) => { eprintln!("dembly validate: {error}"); return ExitCode::from(2); }
    };
    let deck = match dembly_core::load_deck(&deck_path) {
        Ok(deck) => deck,
        Err(error) => { eprintln!("dembly validate: {error}"); return ExitCode::from(2); }
    };
    let Some(deck_root) = deck_path.parent() else {
        eprintln!("dembly validate: deck.toml has no parent directory");
        return ExitCode::from(2);
    };
    let mut card_names = BTreeSet::new();
    let mut mount_targets = BTreeSet::new();
    for reference in deck.cards {
        let card_path = deck_root.join(reference.path);
        let card = match dembly_core::load_card(&card_path) {
            Ok(card) => card,
            Err(error) => { eprintln!("dembly validate: {error}"); return ExitCode::from(2); }
        };
        if card.filesystem.file_type != "squashfs" {
            eprintln!("dembly validate: Card {} filesystem.type must be squashfs", card.name);
            return ExitCode::from(2);
        }
        if !valid_card_name(&card.name) {
            eprintln!("dembly validate: invalid Card name: {}", card.name);
            return ExitCode::from(2);
        }
        if !card_names.insert(card.name.clone()) {
            eprintln!("dembly validate: duplicate Card name: {}", card.name);
            return ExitCode::from(2);
        }
        if !PathBuf::from(&card.mount.target).is_absolute() || card.mount.target.split('/').any(|part| part == "..") {
            eprintln!("dembly validate: invalid Card mount target: {}", card.mount.target);
            return ExitCode::from(2);
        }
        if !mount_targets.insert(card.mount.target.clone()) {
            eprintln!("dembly validate: duplicate Card mount target: {}", card.mount.target);
            return ExitCode::from(2);
        }
        if let Err(error) = dembly_core::verify_card_filesystem(&card_path, &card) {
            eprintln!("dembly validate: {error}");
            return ExitCode::from(2);
        }
    }
    println!("Deck {} is valid", deck.name);
    ExitCode::SUCCESS
}

fn valid_card_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
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
