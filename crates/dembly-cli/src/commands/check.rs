use crate::commands::validate::write_warnings;
use dembly_cli::{CliError, HostContext, HostPlan};
use dembly_core::sha256_file;
use dembly_docker::{CardLock, DemblyLock};
use dembly_runtime::load_runtime_config;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let plan = HostPlan::resolve(&context.config_path)?;
    write_warnings(&plan);
    require_current_lock(&plan)?;
    let state = plan
        .managed_compose
        .state()
        .map_err(|error| CliError::new(error.to_string()))?
        .ok_or_else(|| CliError::new("Dembly applied state is missing; run dembly apply"))?;
    static_applied_state(&plan)?;
    validate_runtime_artifacts(&plan, &state.lock_digest)?;

    writeln!(output, "host integrity: valid")
        .and_then(|()| writeln!(output, "Card checks are not run by Host check."))
        .and_then(|()| {
            writeln!(
                output,
                "Run Card checks with: docker compose run {} __runtime check /run/dembly/runtime/{}.toml",
                plan.deck.document.compose.service, plan.deck.document.compose.service
            )
        })
        .map_err(|error| CliError::new(format!("cannot write check result: {error}")))
}

pub(crate) fn static_applied_state(plan: &HostPlan) -> Result<(), CliError> {
    plan.managed_compose
        .validate_applied_state(&plan.deck.document.compose.service)
        .map_err(|error| CliError::new(error.to_string()))
}

pub(crate) fn lock_status(plan: &HostPlan) -> &'static str {
    match current_lock(plan) {
        Ok(Some(_)) => "valid",
        Ok(None) => "missing",
        Err(_) => "stale",
    }
}

fn require_current_lock(plan: &HostPlan) -> Result<DemblyLock, CliError> {
    current_lock(plan)?.ok_or_else(|| CliError::new("Dembly Lock is missing; run dembly lock"))
}

fn current_lock(plan: &HostPlan) -> Result<Option<DemblyLock>, CliError> {
    let lock = plan
        .managed_compose
        .lock()
        .map_err(|error| CliError::new(error.to_string()))?;
    let Some(lock) = lock else {
        return Ok(None);
    };
    let expected = resolved_lock(plan)?;
    if lock == expected {
        Ok(Some(lock))
    } else {
        Err(CliError::new("Dembly Lock is stale; run dembly lock"))
    }
}

fn resolved_lock(plan: &HostPlan) -> Result<DemblyLock, CliError> {
    let cards = plan
        .deck
        .cards
        .iter()
        .map(|card| {
            Ok(CardLock {
                name: card.document.name.clone(),
                version: card.document.version.clone(),
                manifest_sha256: sha256_file(&card.manifest_path)
                    .map_err(|error| CliError::new(error.to_string()))?,
                filesystem_sha256: card.document.filesystem.sha256.clone(),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(DemblyLock {
        compose_path: plan.deck.compose_path.display().to_string(),
        service: plan.deck.document.compose.service.clone(),
        image: plan.image.id.clone(),
        cards,
    })
}

fn validate_runtime_artifacts(plan: &HostPlan, lock_digest: &str) -> Result<(), CliError> {
    let runtime_root = plan.deck.root.join("runtime");
    let service = &plan.deck.document.compose.service;
    let runtime_path = runtime_root.join(format!("{service}.toml"));
    let runtime = load_runtime_config(&runtime_path).map_err(CliError::new)?;
    if runtime.lock_digest != lock_digest {
        return Err(CliError::new(format!(
            "runtime plan {} lock digest does not match applied state",
            runtime_path.display()
        )));
    }
    if runtime.runtime_user.spec != plan.intended_user_spec() {
        return Err(CliError::new(format!(
            "runtime plan {} user does not match the intended user",
            runtime_path.display()
        )));
    }
    let expected_cards = plan
        .deck
        .cards
        .iter()
        .map(|card| {
            (
                &card.document.name,
                PathBuf::from(&card.document.mount.target),
            )
        })
        .collect::<Vec<_>>();
    let actual_cards = runtime
        .cards
        .iter()
        .map(|card| (&card.name, card.mount_target.clone()))
        .collect::<Vec<_>>();
    if actual_cards != expected_cards {
        return Err(CliError::new(format!(
            "runtime plan {} Cards do not match the resolved Deck",
            runtime_path.display()
        )));
    }

    let binary = runtime_root.join("bin/dembly");
    let metadata = std::fs::metadata(&binary).map_err(|error| {
        CliError::new(format!(
            "cannot inspect Runtime binary {}: {error}",
            binary.display()
        ))
    })?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(CliError::new(format!(
            "Runtime binary {} must be an executable regular file",
            binary.display()
        )));
    }
    Ok(())
}
