use std::os::unix::process::CommandExt;
use std::process::ExitCode;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
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
    if argument == "up" {
        return up(&arguments[1..]);
    }
    if argument == "down" {
        return down(&arguments[1..]);
    }
    if argument == "exec" {
        return exec_command(&arguments[1..]);
    }
    if argument == "run" {
        return run_command(&arguments[1..]);
    }
    if argument == "check" {
        return check_command(&arguments[1..]);
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
    let base = match &resolved.document.base {
        dembly_core::Base::Image { image } => match dembly_docker::image_identity(image) {
            Ok(identity) => dembly_core::LockBase::Image {
                reference: image.clone(),
                resolved_image_id: identity,
            },
            Err(error) => {
                eprintln!("dembly lock: {error}");
                return ExitCode::from(2);
            }
        },
        dembly_core::Base::Compose { compose, service } => {
            let compose_path = resolved.root.join(compose);
            let image = match dembly_docker::compose_service_image(&compose_path, service) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("dembly lock: {error}");
                    return ExitCode::from(2);
                }
            };
            let image_id = match dembly_docker::image_identity(&image) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("dembly lock: {error}");
                    return ExitCode::from(2);
                }
            };
            let compose_sha256 = match dembly_core::sha256_file(&compose_path) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("dembly lock: {error}");
                    return ExitCode::from(2);
                }
            };
            dembly_core::LockBase::Compose {
                compose: compose.clone(),
                service: service.clone(),
                compose_sha256,
                resolved_image_id: image_id,
            }
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
        base,
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

fn up(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly up accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly up: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved = match dembly_core::resolve_deck(&deck_path, &bind_variables(&deck_path)) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    };
    let image = match &resolved.document.base {
        dembly_core::Base::Image { image } => image,
        dembly_core::Base::Compose { .. } => {
            eprintln!("dembly up: Compose Base lifecycle is not implemented yet");
            return ExitCode::from(2);
        }
    };
    let image_config = match dembly_docker::inspect_image(image) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = enforce_image_lock(&resolved, &image_config.id) {
        eprintln!("dembly up: {error}");
        return ExitCode::from(2);
    }
    if let Err(error) = verify_cards(&resolved) {
        eprintln!("dembly up: {error}");
        return ExitCode::from(2);
    }
    let process_argv = image_config
        .entrypoint
        .iter()
        .chain(&image_config.command)
        .cloned()
        .collect::<Vec<_>>();
    if process_argv.is_empty() {
        eprintln!("dembly up: image has no original Entrypoint or Cmd");
        return ExitCode::from(2);
    }
    let user = match runtime_user(&image_config.user) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    };
    let state = match runtime_state(&resolved.root, &resolved.document.name) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = create_volumes(&resolved) {
        eprintln!("dembly up: {error}");
        return ExitCode::from(2);
    }
    let environment = planned_environment(&resolved, &image_config.environment);
    let runtime_config = state.join("runtime.toml");
    if let Err(error) = fs::write(
        &runtime_config,
        render_runtime_config(&resolved, &user, &environment, &process_argv),
    ) {
        eprintln!(
            "dembly up: cannot write {}: {error}",
            runtime_config.display()
        );
        return ExitCode::from(2);
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("dembly up: cannot resolve current executable: {error}");
            return ExitCode::from(2);
        }
    };
    let name = format!("dembly-{}", resolved.document.name);
    let labels = managed_labels(&resolved.document.name);
    let cards = resolved
        .cards
        .iter()
        .map(|card| dembly_docker::CardFileBind {
            name: card.document.name.clone(),
            source: card
                .manifest_path
                .parent()
                .unwrap()
                .join(&card.document.filesystem.file),
        })
        .collect();
    let mut extra_mounts = resolved
        .volumes
        .iter()
        .map(|volume| {
            (
                volume.source.clone(),
                volume.target.to_string_lossy().into_owned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    extra_mounts.extend(resolved.binds.iter().map(|bind| {
        (
            bind.source.clone(),
            bind.target.to_string_lossy().into_owned(),
            bind.mode == "ro",
        )
    }));
    let plan = dembly_docker::ImageRuntimePlan {
        image: image.clone(),
        container_name: name.clone(),
        executable,
        runtime_config,
        cards,
        labels,
        extra_mounts,
    };
    match dembly_docker::run_docker(&dembly_docker::image_create_command(&plan)) {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("dembly up: Docker create failed with status {status}");
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("dembly up: {error}");
            return ExitCode::from(2);
        }
    }
    match dembly_docker::docker_status(&["start", &name]) {
        Ok(status) if status.success() => {
            println!("started {name}");
            ExitCode::SUCCESS
        }
        Ok(status) => {
            eprintln!("dembly up: Docker start failed with status {status}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("dembly up: {error}");
            ExitCode::from(2)
        }
    }
}

fn down(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly down accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly down: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly down: {error}");
            return ExitCode::from(2);
        }
    };
    let deck = match dembly_core::load_deck(&deck_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly down: {error}");
            return ExitCode::from(2);
        }
    };
    if !matches!(deck.base, dembly_core::Base::Image { .. }) {
        eprintln!("dembly down: Compose Base lifecycle is not implemented yet");
        return ExitCode::from(2);
    }
    let name = format!("dembly-{}", deck.name);
    let managed = dembly_docker::container_label(&name, "io.dembly.managed");
    let owner = dembly_docker::container_label(&name, "io.dembly.deck");
    if managed.as_deref() != Ok("true") || owner.as_deref() != Ok(deck.name.as_str()) {
        eprintln!("dembly down: Runtime ownership label verification failed: {name}");
        return ExitCode::from(2);
    }
    match dembly_docker::docker_status(&["rm", "-f", &name]) {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("dembly down: Docker remove failed with status {status}");
            return ExitCode::from(2);
        }
        Err(error) => {
            eprintln!("dembly down: {error}");
            return ExitCode::from(2);
        }
    }
    let root = deck_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    if let Ok(state) = runtime_state(root, &deck.name) {
        if let Err(error) = fs::remove_dir_all(&state) {
            eprintln!(
                "dembly down: cannot remove Runtime metadata {}: {error}",
                state.display()
            );
            return ExitCode::from(2);
        }
    }
    println!("stopped {name}");
    ExitCode::SUCCESS
}

fn check_command(arguments: &[String]) -> ExitCode {
    if arguments.len() > 1 {
        eprintln!("dembly check accepts at most one deck.toml path");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly check: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments.first().map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly check: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved = match dembly_core::resolve_deck(&deck_path, &bind_variables(&deck_path)) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly check: {error}");
            return ExitCode::from(2);
        }
    };
    let image = match &resolved.document.base {
        dembly_core::Base::Image { image } => image,
        dembly_core::Base::Compose { .. } => {
            eprintln!("dembly check: Compose Base lifecycle is not implemented yet");
            return ExitCode::from(2);
        }
    };
    let image_config = match dembly_docker::inspect_image(image) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly check: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) =
        enforce_image_lock(&resolved, &image_config.id).and_then(|_| verify_cards(&resolved))
    {
        eprintln!("dembly check: {error}");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for card in &resolved.cards {
        let Some(check) = &card.document.check else {
            continue;
        };
        let mut run_arguments = vec![
            deck_path.to_string_lossy().into_owned(),
            "--".into(),
            format!("{}/{}", card.document.mount.target, check.exec),
        ];
        run_arguments.extend(check.args.iter().cloned());
        println!("check {}", card.document.name);
        if run_command(&run_arguments) != ExitCode::SUCCESS {
            failed = true;
        }
    }
    if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_command(arguments: &[String]) -> ExitCode {
    let Some(separator) = arguments.iter().position(|argument| argument == "--") else {
        eprintln!("dembly run requires -- <command...>");
        return ExitCode::from(2);
    };
    if separator > 1 || separator + 1 == arguments.len() {
        eprintln!("dembly run usage: dembly run [deck.toml] -- <command...>");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments
        .first()
        .filter(|_| separator == 1)
        .map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved = match dembly_core::resolve_deck(&deck_path, &bind_variables(&deck_path)) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: {error}");
            return ExitCode::from(2);
        }
    };
    let image = match &resolved.document.base {
        dembly_core::Base::Image { image } => image,
        dembly_core::Base::Compose { .. } => {
            eprintln!("dembly run: Compose Base lifecycle is not implemented yet");
            return ExitCode::from(2);
        }
    };
    let image_config = match dembly_docker::inspect_image(image) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) =
        enforce_image_lock(&resolved, &image_config.id).and_then(|_| verify_cards(&resolved))
    {
        eprintln!("dembly run: {error}");
        return ExitCode::from(2);
    }
    let user = match runtime_user(&image_config.user) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = create_volumes(&resolved) {
        eprintln!("dembly run: {error}");
        return ExitCode::from(2);
    }
    let persistent_state = match runtime_state(&resolved.root, &resolved.document.name) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: {error}");
            return ExitCode::from(2);
        }
    };
    let state = persistent_state.join(format!("run-{}", std::process::id()));
    if let Err(error) = fs::create_dir_all(&state)
        .and_then(|_| fs::set_permissions(&state, fs::Permissions::from_mode(0o700)))
    {
        eprintln!("dembly run: cannot create temporary Runtime metadata: {error}");
        return ExitCode::from(2);
    }
    let runtime_config = state.join("runtime.toml");
    let environment = planned_environment(&resolved, &image_config.environment);
    if let Err(error) = fs::write(
        &runtime_config,
        render_runtime_config(&resolved, &user, &environment, &arguments[separator + 1..]),
    ) {
        eprintln!("dembly run: cannot write runtime metadata: {error}");
        let _ = fs::remove_dir_all(&state);
        return ExitCode::from(2);
    }
    let executable = match std::env::current_exe() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly run: cannot resolve current executable: {error}");
            let _ = fs::remove_dir_all(&state);
            return ExitCode::from(2);
        }
    };
    let name = format!(
        "dembly-{}-run-{}",
        resolved.document.name,
        std::process::id()
    );
    let cards = resolved
        .cards
        .iter()
        .map(|card| dembly_docker::CardFileBind {
            name: card.document.name.clone(),
            source: card
                .manifest_path
                .parent()
                .unwrap()
                .join(&card.document.filesystem.file),
        })
        .collect();
    let mut extra_mounts = resolved
        .volumes
        .iter()
        .map(|volume| {
            (
                volume.source.clone(),
                volume.target.to_string_lossy().into_owned(),
                false,
            )
        })
        .collect::<Vec<_>>();
    extra_mounts.extend(resolved.binds.iter().map(|bind| {
        (
            bind.source.clone(),
            bind.target.to_string_lossy().into_owned(),
            bind.mode == "ro",
        )
    }));
    let plan = dembly_docker::ImageRuntimePlan {
        image: image.clone(),
        container_name: name.clone(),
        executable,
        runtime_config,
        cards,
        labels: managed_labels(&resolved.document.name),
        extra_mounts,
    };
    let result = match dembly_docker::run_docker(&dembly_docker::image_create_command(&plan)) {
        Ok(status) if status.success() => {
            match dembly_docker::docker_status(&["start", "-a", &name]) {
                Ok(status) => status.code().unwrap_or(2),
                Err(error) => {
                    eprintln!("dembly run: {error}");
                    2
                }
            }
        }
        Ok(status) => {
            eprintln!("dembly run: Docker create failed with status {status}");
            2
        }
        Err(error) => {
            eprintln!("dembly run: {error}");
            2
        }
    };
    if let Err(error) = dembly_docker::docker_status(&["rm", "-f", &name]) {
        eprintln!("dembly run: temporary Runtime cleanup failed: {error}");
    }
    if let Err(error) = fs::remove_dir_all(&state) {
        eprintln!("dembly run: temporary metadata cleanup failed: {error}");
    }
    ExitCode::from(result as u8)
}

fn exec_command(arguments: &[String]) -> ExitCode {
    let Some(separator) = arguments.iter().position(|argument| argument == "--") else {
        eprintln!("dembly exec requires -- <command...>");
        return ExitCode::from(2);
    };
    if separator > 1 || separator + 1 == arguments.len() {
        eprintln!("dembly exec usage: dembly exec [deck.toml] -- <command...>");
        return ExitCode::from(2);
    }
    let current_directory = match std::env::current_dir() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly exec: cannot determine current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let explicit = arguments
        .first()
        .filter(|_| separator == 1)
        .map(PathBuf::from);
    let deck_path = match dembly_core::discover_deck(explicit.as_deref(), &current_directory) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly exec: {error}");
            return ExitCode::from(2);
        }
    };
    let deck = match dembly_core::load_deck(&deck_path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly exec: {error}");
            return ExitCode::from(2);
        }
    };
    let name = format!("dembly-{}", deck.name);
    if dembly_docker::container_label(&name, "io.dembly.managed").as_deref() != Ok("true")
        || dembly_docker::container_label(&name, "io.dembly.deck").as_deref()
            != Ok(deck.name.as_str())
    {
        eprintln!("dembly exec: Runtime is not running: {name}");
        return ExitCode::from(2);
    }
    let root = deck_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let state = match runtime_state(root, &deck.name) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly exec: {error}");
            return ExitCode::from(2);
        }
    };
    let config = match dembly_runtime::load_runtime_config(&state.join("runtime.toml")) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("dembly exec: cannot read runtime metadata: {error}");
            return ExitCode::from(2);
        }
    };
    let user = format!("{}:{}", config.runtime_user.uid, config.runtime_user.gid);
    match dembly_docker::exec_in_container(&name, &user, &arguments[separator + 1..]) {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(2) as u8),
        Err(error) => {
            eprintln!("dembly exec: {error}");
            ExitCode::from(2)
        }
    }
}

#[derive(Clone)]
struct RuntimeUser {
    name: String,
    uid: u32,
    gid: u32,
    home: String,
}

fn runtime_user(configured: &str) -> Result<RuntimeUser, String> {
    if configured.is_empty() || configured == "root" || configured == "0" || configured == "0:0" {
        return Ok(RuntimeUser {
            name: "root".into(),
            uid: 0,
            gid: 0,
            home: "/root".into(),
        });
    }
    let mut values = configured.split(':');
    let uid = values.next().unwrap_or_default().parse::<u32>().map_err(|_| format!("cannot resolve named image user {configured}; only root or numeric UID:GID is currently supported"))?;
    let gid = values
        .next()
        .map(str::parse)
        .transpose()
        .map_err(|_| format!("invalid image GID in {configured}"))?
        .unwrap_or(uid);
    if values.next().is_some() {
        return Err(format!("invalid image user: {configured}"));
    }
    Ok(RuntimeUser {
        name: uid.to_string(),
        uid,
        gid,
        home: "/".into(),
    })
}

fn verify_cards(resolved: &dembly_core::ResolvedDeck) -> Result<(), String> {
    for card in &resolved.cards {
        dembly_core::verify_card_filesystem(&card.manifest_path, &card.document)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
fn create_volumes(resolved: &dembly_core::ResolvedDeck) -> Result<(), String> {
    for volume in &resolved.volumes {
        if volume.source.is_symlink() {
            return Err(format!(
                "Volume physical path is a symlink: {}",
                volume.source.display()
            ));
        }
        fs::create_dir_all(&volume.source).map_err(|error| {
            format!("cannot create Volume {}: {error}", volume.source.display())
        })?;
    }
    Ok(())
}
fn managed_labels(deck: &str) -> Vec<(String, String)> {
    vec![
        ("io.dembly.managed".into(), "true".into()),
        ("io.dembly.deck".into(), deck.into()),
        ("io.dembly.schema".into(), "1".into()),
    ]
}
fn runtime_state(deck_root: &std::path::Path, deck_name: &str) -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let state = base
        .join("dembly")
        .join(current_uid().to_string())
        .join(deck_name);
    if state.starts_with(deck_root.join("volumes")) {
        return Err("Runtime metadata must not be stored below Deck volumes".into());
    }
    fs::create_dir_all(&state)
        .map_err(|error| format!("cannot create Runtime state directory: {error}"))?;
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("cannot protect Runtime state directory: {error}"))?;
    Ok(state)
}

fn current_uid() -> u32 {
    // SAFETY: getuid has no arguments or ownership contract and is available on Linux.
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    // SAFETY: getuid has no arguments and cannot fail.
    unsafe { getuid() }
}
fn planned_environment(
    resolved: &dembly_core::ResolvedDeck,
    base: &[String],
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for entry in base {
        if let Some((key, value)) = entry.split_once('=') {
            values.insert(key.into(), value.into());
        }
    }
    let cards = resolved
        .cards
        .iter()
        .map(|card| {
            dembly_core::CardEnvironment::new(
                &card.document.mount.target,
                card.document.environment.clone(),
                card.document.environment_path_prepend.clone(),
            )
        })
        .collect::<Vec<_>>();
    dembly_core::plan_environment(
        values,
        resolved.document.environment.clone(),
        &cards,
        &resolved.document.environment_path_prepend,
    )
    .unwrap_or_default()
}
fn render_runtime_config(
    resolved: &dembly_core::ResolvedDeck,
    user: &RuntimeUser,
    environment: &BTreeMap<String, String>,
    argv: &[String],
) -> String {
    let mut output = format!("schema_version = 1\ndeck_name = \"{}\"\n[runtime_user]\nname = \"{}\"\nuid = {}\ngid = {}\nhome = \"{}\"\n", toml(&resolved.document.name), toml(&user.name), user.uid, user.gid, toml(&user.home));
    for card in &resolved.cards {
        output.push_str(&format!("[[cards]]\nname = \"{}\"\nimage = \"/run/dembly/cards/{}.squashfs\"\nmount_target = \"{}\"\n", toml(&card.document.name), toml(&card.document.name), toml(&card.document.mount.target)));
        for export in &card.document.exports {
            output.push_str(&format!(
                "[[exports]]\nsource = \"{}\"\ntarget = \"{}\"\n",
                toml(&format!("{}/{}", card.document.mount.target, export.source)),
                toml(&export.target)
            ));
        }
        for hook in &card.document.post_mount_hooks {
            output.push_str(&format!(
                "[[hooks]]\ncard = \"{}\"\nexec = \"{}\"\nargs = {}\n",
                toml(&card.document.name),
                toml(&format!("{}/{}", card.document.mount.target, hook.exec)),
                toml_array(&hook.args)
            ));
        }
    }
    output.push_str("[environment]\n");
    for (key, value) in environment {
        output.push_str(&format!("{} = \"{}\"\n", key, toml(value)));
    }
    output.push_str(&format!("[process]\nargv = {}\n", toml_array(argv)));
    output
}
fn toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", toml(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn enforce_image_lock(resolved: &dembly_core::ResolvedDeck, image_id: &str) -> Result<(), String> {
    let path = resolved.root.join("deck.lock");
    let lock = dembly_core::read_lock(&path).map_err(|_| {
        format!(
            "missing or invalid deck.lock; run dembly lock ({})",
            path.display()
        )
    })?;
    let dembly_core::LockBase::Image {
        reference,
        resolved_image_id,
    } = lock.base
    else {
        return Err("Deck lock Base kind does not match Image Base Deck".into());
    };
    let dembly_core::Base::Image { image } = &resolved.document.base else {
        return Err("Deck lock Base kind does not match Deck".into());
    };
    if &reference != image || resolved_image_id != image_id {
        return Err("stale deck.lock Base image identity; run dembly lock".into());
    }
    if lock.cards.len() != resolved.cards.len() {
        return Err("stale deck.lock Card set; run dembly lock".into());
    }
    for (locked, card) in lock.cards.iter().zip(&resolved.cards) {
        let manifest =
            dembly_core::sha256_file(&card.manifest_path).map_err(|error| error.to_string())?;
        if locked.name != card.document.name
            || locked.version != card.document.version
            || locked.manifest_sha256 != manifest
            || locked.filesystem_sha256 != card.document.filesystem.sha256
        {
            return Err("stale deck.lock Card identity; run dembly lock".into());
        }
    }
    Ok(())
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
