use std::os::unix::process::CommandExt;
use std::process::ExitCode;
use std::{
    io::{self, Write},
    path::PathBuf,
};

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

    if argument == "card"
        && arguments
            .get(1)
            .is_some_and(|subcommand| subcommand == "build")
    {
        return card_build(&arguments[2..]);
    }
    if argument == "validate" {
        return validate(&arguments[1..]);
    }
    if argument == "lock" {
        return lock(&arguments[1..]);
    }
    if argument == "inspect" {
        return inspect(&arguments[1..]);
    }
    if argument == "__runtime" {
        return runtime_command(&arguments[1..]);
    }

    eprintln!("dembly: command is not implemented yet");
    ExitCode::from(2)
}

fn inspect(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly inspect accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly inspect: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly inspect: {error}");
            return ExitCode::from(2);
        }
    };
    let deck = match dembly_core::load_deck(&deck_path) {
        Ok(deck) => deck,
        Err(error) => {
            eprintln!("dembly inspect: {error}");
            return ExitCode::from(2);
        }
    };
    println!("Deck: {}", deck.name);
    println!(
        "Deck root: {}",
        deck_path.parent().unwrap_or(&deck_path).display()
    );
    match &deck.base {
        dembly_core::Base::Image { image } => println!("Image Base: {image}"),
        dembly_core::Base::Compose { compose, service } => {
            println!("Compose Base: {compose} service={service}")
        }
    }
    let root = deck_path.parent().unwrap_or(&deck_path);
    for reference in deck.cards {
        match dembly_core::load_card(&root.join(reference.path)) {
            Ok(card) => println!(
                "Card: {} {} mount={}",
                card.name, card.version, card.mount.target
            ),
            Err(error) => {
                eprintln!("dembly inspect: {error}");
                return ExitCode::from(2);
            }
        }
    }
    ExitCode::SUCCESS
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

#[cfg(target_os = "linux")]
unsafe fn libc_geteuid() -> u32 {
    // SAFETY: geteuid has no arguments, no memory ownership contract, and is available on Linux.
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

fn drop_privileges(uid: u32, gid: u32) -> Result<(), String> {
    // SAFETY: setgid/setuid receive plain numeric IDs parsed from host-generated runtime.toml.
    // This runs only after the root-only mount phase and immediately before exec.
    extern "C" {
        fn setgid(gid: u32) -> i32;
        fn setuid(uid: u32) -> i32;
    }
    // SAFETY: both functions have no pointer arguments. A non-zero return is converted to an error.
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

fn validate(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly validate accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly validate: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly validate: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved = match dembly_core::resolve_deck(&deck_path, &bind_variables(&deck_path)) {
        Ok(deck) => deck,
        Err(error) => {
            eprintln!("dembly validate: {error}");
            return ExitCode::from(2);
        }
    };
    for card in &resolved.cards {
        if let Err(error) = dembly_core::verify_card_filesystem(&card.manifest_path, &card.document)
        {
            eprintln!("dembly validate: {error}");
            return ExitCode::from(2);
        }
    }
    for warning in &resolved.warnings {
        eprintln!("dembly validate: warning: {warning}");
    }
    println!("Deck {} is valid", resolved.document.name);
    ExitCode::SUCCESS
}

fn lock(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly lock accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly lock: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly lock: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved = match dembly_core::resolve_deck(&deck_path, &bind_variables(&deck_path)) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("dembly lock: {error}");
            return ExitCode::from(2);
        }
    };
    let reference = match &resolved.document.base {
        dembly_core::Base::Image { image } => image,
        dembly_core::Base::Compose { .. } => {
            eprintln!("dembly lock: Compose Base lock is not implemented yet");
            return ExitCode::from(2);
        }
    };
    let image_id = match dembly_docker::image_identity(reference) {
        Ok(identity) => identity,
        Err(error) => {
            eprintln!("dembly lock: {error}");
            return ExitCode::from(2);
        }
    };
    let mut cards = Vec::new();
    for card in &resolved.cards {
        if let Err(error) = dembly_core::verify_card_filesystem(&card.manifest_path, &card.document)
        {
            eprintln!("dembly lock: {error}");
            return ExitCode::from(2);
        }
        let manifest_sha256 = match dembly_core::sha256_file(&card.manifest_path) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("dembly lock: {error}");
                return ExitCode::from(2);
            }
        };
        let source = card
            .manifest_path
            .strip_prefix(&resolved.root)
            .unwrap_or(&card.manifest_path)
            .to_string_lossy()
            .into_owned();
        cards.push(dembly_core::LockedCard {
            name: card.document.name.clone(),
            version: card.document.version.clone(),
            source,
            manifest_sha256,
            filesystem_sha256: card.document.filesystem.sha256.clone(),
        });
    }
    let lock = dembly_core::DeckLock {
        schema_version: 1,
        base: dembly_core::LockBase::Image {
            reference: reference.clone(),
            resolved_image_id: image_id,
        },
        cards,
    };
    let path = resolved.root.join("deck.lock");
    match dembly_core::write_lock(&path, &lock) {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("dembly lock: {error}");
            ExitCode::from(2)
        }
    }
}

fn bind_variables(deck_path: &std::path::Path) -> dembly_core::BindVariables {
    let host_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let deck_root = deck_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."));
    dembly_core::BindVariables {
        host_home,
        deck_root,
        user: "root".into(),
        home: "/root".into(),
    }
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
            "--name" => {
                index += 1;
                name = option_value(arguments, index, "--name");
            }
            "--version" => {
                index += 1;
                version = option_value(arguments, index, "--version");
            }
            "--mount-target" => {
                index += 1;
                mount_target = option_value(arguments, index, "--mount-target");
            }
            "--path-prepend" => {
                index += 1;
                let Some(value) = option_value(arguments, index, "--path-prepend") else {
                    return ExitCode::from(2);
                };
                path_prepend.push(value);
            }
            option => {
                eprintln!("unknown card build option: {option}");
                return ExitCode::from(2);
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
    match dembly_card::build_card(&request) {
        Ok(result) => {
            println!("built {}", result.card_root.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("dembly card build: {error}");
            ExitCode::from(2)
        }
    }
}

fn option_value(arguments: &[String], index: usize, option: &str) -> Option<String> {
    let value = arguments.get(index).cloned();
    if value.is_none() {
        eprintln!("{option} requires a value");
    }
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
    if value.is_empty() {
        default.map(str::to_owned)
    } else {
        Some(value)
    }
}
