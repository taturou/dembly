use dembly_runtime::{
    ResolvedRuntimeUser, RuntimeBind, RuntimeCard, RuntimeCheck, RuntimeConfig, RuntimeExport,
    RuntimeHook,
};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub trait RuntimeSystem {
    fn effective_uid(&mut self) -> u32;
    fn read_passwd(&mut self) -> Result<String, String>;
    fn read_group(&mut self) -> Result<String, String>;
    fn mount_card(&mut self, card: &RuntimeCard) -> Result<(), String>;
    fn ensure_card_mounts(&mut self, cards: &[RuntimeCard]) -> Result<(), String>;
    fn mount_bind(&mut self, bind: &RuntimeBind, user: &ResolvedRuntimeUser) -> Result<(), String>;
    fn create_export(&mut self, export: &RuntimeExport) -> Result<(), String>;
    fn run_hook(
        &mut self,
        hook: &RuntimeHook,
        cards: &[RuntimeCard],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), String>;
    fn set_environment(&mut self, environment: &BTreeMap<String, String>) -> Result<(), String>;
    fn set_gid(&mut self, gid: u32) -> Result<(), String>;
    fn set_uid(&mut self, uid: u32) -> Result<(), String>;
    fn exec(&mut self, argv: &[String]) -> Result<(), String>;
    fn run_check(
        &mut self,
        check: &RuntimeCheck,
        card: &RuntimeCard,
        config: &RuntimeConfig,
        user: &ResolvedRuntimeUser,
    ) -> Result<(), String>;
}

pub fn run_runtime_command(
    arguments: &[String],
    system: &mut impl RuntimeSystem,
) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("init") if arguments.len() >= 2 => run_init(arguments, system),
        Some("check") if arguments.len() == 2 => run_check(&arguments[1], system),
        _ => Err("invalid internal runtime command".into()),
    }
}

fn run_init(arguments: &[String], system: &mut impl RuntimeSystem) -> Result<(), String> {
    if system.effective_uid() != 0 {
        return Err("initializer must run as root".into());
    }

    let plan = PathBuf::from(&arguments[1]);
    let config = dembly_runtime::load_runtime_config(&plan)?;
    let argv = if arguments.len() > 2 {
        arguments[2..].to_vec()
    } else {
        config.process_argv.clone()
    };
    let program = argv
        .first()
        .ok_or_else(|| "final process argv must not be empty".to_owned())?;
    let user = resolve_user(&config, system)?;

    for card in &config.cards {
        system.mount_card(card).map_err(|error| {
            format!(
                "cannot mount Card {} (source {}, target {}): {error}",
                card.name,
                card.image.display(),
                card.mount_target.display()
            )
        })?;
    }
    system
        .ensure_card_mounts(&config.cards)
        .map_err(|error| format!("cannot verify Card mount targets: {error}"))?;
    for bind in &config.binds {
        system.mount_bind(bind, &user).map_err(|error| {
            format!(
                "cannot mount Host Bind (source {}, target {}): {error}",
                bind.source.display(),
                bind.target
            )
        })?;
    }
    for export in &config.exports {
        system.create_export(export).map_err(|error| {
            format!(
                "cannot create export (source {}, target {}): {error}",
                export.source.display(),
                export.target.display()
            )
        })?;
    }
    system
        .set_environment(&config.environment)
        .map_err(|error| format!("cannot set runtime environment: {error}"))?;
    for hook in &config.hooks {
        system
            .run_hook(hook, &config.cards, &config.environment)
            .map_err(|error| {
                format!(
                    "cannot run post_mount hook {} for Card {}: {error}",
                    hook.exec.display(),
                    hook.card
                )
            })?;
    }
    system
        .set_gid(user.gid)
        .map_err(|error| format!("cannot set GID to {}: {error}", user.gid))?;
    system
        .set_uid(user.uid)
        .map_err(|error| format!("cannot set UID to {}: {error}", user.uid))?;
    system
        .exec(&argv)
        .map_err(|error| format!("cannot exec {program}: {error}"))
}

fn run_check(plan: &str, system: &mut impl RuntimeSystem) -> Result<(), String> {
    let config = dembly_runtime::load_runtime_config(&PathBuf::from(plan))?;
    let user = resolve_user(&config, system)?;
    for check in &config.checks {
        if !config.cards.iter().any(|card| card.name == check.card) {
            return Err(format!(
                "cannot run check {}: unknown Card {}",
                check.exec.display(),
                check.card
            ));
        }
    }
    for card in &config.cards {
        for check in config.checks.iter().filter(|check| check.card == card.name) {
            system
                .run_check(check, card, &config, &user)
                .map_err(|error| {
                    format!(
                        "cannot run check {} for Card {}: {error}",
                        check.exec.display(),
                        check.card
                    )
                })?;
        }
    }
    Ok(())
}

fn resolve_user(
    config: &RuntimeConfig,
    system: &mut impl RuntimeSystem,
) -> Result<ResolvedRuntimeUser, String> {
    let spec = &config.runtime_user.spec;
    let passwd = system.read_passwd().map_err(|error| {
        format!("cannot resolve runtime user {spec}: read /etc/passwd: {error}")
    })?;
    let group = system
        .read_group()
        .map_err(|error| format!("cannot resolve runtime user {spec}: read /etc/group: {error}"))?;
    dembly_runtime::resolve_runtime_user(spec, &passwd, &group)
        .map_err(|error| format!("cannot resolve runtime user {spec}: {error}"))
}

pub struct NativeRuntimeSystem;

impl RuntimeSystem for NativeRuntimeSystem {
    fn effective_uid(&mut self) -> u32 {
        unsafe { geteuid() }
    }

    fn read_passwd(&mut self) -> Result<String, String> {
        fs::read_to_string("/etc/passwd").map_err(|error| error.to_string())
    }

    fn read_group(&mut self) -> Result<String, String> {
        fs::read_to_string("/etc/group").map_err(|error| error.to_string())
    }

    fn mount_card(&mut self, card: &RuntimeCard) -> Result<(), String> {
        dembly_runtime::mount_card(card)
    }

    fn ensure_card_mounts(&mut self, cards: &[RuntimeCard]) -> Result<(), String> {
        dembly_runtime::ensure_mount_targets_exist(cards)
    }

    fn mount_bind(&mut self, bind: &RuntimeBind, user: &ResolvedRuntimeUser) -> Result<(), String> {
        dembly_runtime::mount_bind(bind, user)
    }

    fn create_export(&mut self, export: &RuntimeExport) -> Result<(), String> {
        dembly_runtime::create_exports(std::slice::from_ref(export))
    }

    fn run_hook(
        &mut self,
        hook: &RuntimeHook,
        cards: &[RuntimeCard],
        environment: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        dembly_runtime::run_hooks(std::slice::from_ref(hook), cards, environment)
    }

    fn set_environment(&mut self, environment: &BTreeMap<String, String>) -> Result<(), String> {
        for (name, value) in environment {
            std::env::set_var(name, value);
        }
        Ok(())
    }

    fn set_gid(&mut self, gid: u32) -> Result<(), String> {
        if unsafe { setgid(gid) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }

    fn set_uid(&mut self, uid: u32) -> Result<(), String> {
        if unsafe { setuid(uid) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }

    fn exec(&mut self, argv: &[String]) -> Result<(), String> {
        let error = Command::new(&argv[0]).args(&argv[1..]).exec();
        Err(error.to_string())
    }

    fn run_check(
        &mut self,
        check: &RuntimeCheck,
        card: &RuntimeCard,
        config: &RuntimeConfig,
        user: &ResolvedRuntimeUser,
    ) -> Result<(), String> {
        let mut one_check = config.clone();
        one_check.cards = vec![card.clone()];
        one_check.checks = vec![check.clone()];
        dembly_runtime::run_checks(&one_check, user)
    }
}

extern "C" {
    fn geteuid() -> u32;
    fn setgid(gid: u32) -> i32;
    fn setuid(uid: u32) -> i32;
}
