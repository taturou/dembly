use crate::commands::lock::{resolved_lock, resolved_lock_from_deck};
use dembly_cli::artifacts::{
    artifact_digest, LOCK_DIGEST_LABEL, RUNTIME_BINARY_DIGEST_LABEL, RUNTIME_PLAN_DIGEST_LABEL,
};
use dembly_cli::{CheckHostPlan, CliError, HostContext, HostPlan};
use dembly_core::LockInput;
use dembly_runtime::{load_runtime_config, render_runtime_config};
use serde_yaml::Value;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let plan = CheckHostPlan::resolve(&context.config_path)?;
    for warning in &plan.deck.warnings {
        eprintln!("warning: {warning}");
    }
    let lock = require_current_static_lock(&plan)?;
    let digest = lock.digest();
    let state = plan
        .managed_compose
        .state()
        .map_err(|error| CliError::new(error.to_string()))?
        .ok_or_else(|| CliError::new("Dembly applied state is missing; run dembly apply"))?;
    require_current_lock_generation(&state, &digest)?;
    static_applied_compose(&plan.managed_compose, &plan.deck.document.compose.service)?;
    validate_runtime_artifacts(&plan, &state, &digest)?;

    writeln!(output, "host integrity: valid")
        .and_then(|()| writeln!(output, "Card checks are not run by Host check."))
        .and_then(|()| {
            writeln!(
                output,
                "Run Card checks with: {}",
                runtime_check_command(&plan)
            )
        })
        .map_err(|error| CliError::new(format!("cannot write check result: {error}")))
}

fn require_current_lock_generation(
    state: &dembly_docker::ApplyState,
    digest: &str,
) -> Result<(), CliError> {
    if state.lock_digest != digest {
        return Err(CliError::new(
            "applied state lock digest does not match the current Lock; run dembly apply",
        ));
    }
    let label = state
        .fields
        .labels
        .entries
        .get(LOCK_DIGEST_LABEL)
        .ok_or_else(|| {
            CliError::new(format!(
                "Dembly managed label {LOCK_DIGEST_LABEL} is missing from applied state"
            ))
        })?;
    if label.applied != dembly_docker::ManagedValue::Present(Value::String(digest.into())) {
        return Err(CliError::new(format!(
            "Dembly managed label {LOCK_DIGEST_LABEL} does not match the current Lock digest"
        )));
    }
    Ok(())
}

pub(crate) fn static_applied_state(plan: &HostPlan) -> Result<(), CliError> {
    static_applied_compose(&plan.managed_compose, &plan.deck.document.compose.service)
}

fn static_applied_compose(
    managed_compose: &dembly_docker::ManagedCompose,
    service: &str,
) -> Result<(), CliError> {
    managed_compose
        .validate_applied_state(service)
        .map_err(|error| CliError::new(error.to_string()))
}

pub(crate) fn lock_status(plan: &HostPlan) -> &'static str {
    match current_lock(plan) {
        Ok(Some(_)) => "valid",
        Ok(None) => "missing",
        Err(_) => "stale",
    }
}

fn current_lock(plan: &HostPlan) -> Result<Option<LockInput>, CliError> {
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

fn require_current_static_lock(plan: &CheckHostPlan) -> Result<LockInput, CliError> {
    let lock = plan
        .managed_compose
        .lock()
        .map_err(|error| CliError::new(error.to_string()))?
        .ok_or_else(|| CliError::new("Dembly Lock is missing; run dembly lock"))?;
    let expected = resolved_lock_from_deck(&plan.deck, lock.image.clone())?;
    if lock != expected {
        return Err(CliError::new("Dembly Lock is stale; run dembly lock"));
    }
    Ok(lock)
}

fn validate_runtime_artifacts(
    plan: &CheckHostPlan,
    state: &dembly_docker::ApplyState,
    lock_digest: &str,
) -> Result<(), CliError> {
    let runtime_root = plan.deck.root.join("runtime");
    let service = &plan.deck.document.compose.service;
    let runtime_path = runtime_root.join(format!("{service}.toml"));
    let runtime_bytes = std::fs::read(&runtime_path).map_err(|error| {
        CliError::new(format!(
            "cannot read Runtime plan {}: {error}",
            runtime_path.display()
        ))
    })?;
    let runtime = load_runtime_config(&runtime_path).map_err(CliError::new)?;
    if runtime.lock_digest != lock_digest {
        return Err(CliError::new(format!(
            "runtime plan {} lock digest does not match applied state",
            runtime_path.display()
        )));
    }
    let canonical = render_runtime_config(&runtime).map_err(CliError::new)?;
    if canonical.as_bytes() != runtime_bytes {
        return Err(CliError::new(format!(
            "Runtime plan {} is not in canonical form",
            runtime_path.display()
        )));
    }
    let expected_runtime_digest = applied_label(state, RUNTIME_PLAN_DIGEST_LABEL)?;
    let actual_runtime_digest = artifact_digest(&runtime_bytes);
    if actual_runtime_digest != expected_runtime_digest {
        return Err(CliError::new(format!(
            "Runtime plan digest does not match applied artifact identity: {}",
            runtime_path.display(),
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
    let expected_binary_digest = applied_label(state, RUNTIME_BINARY_DIGEST_LABEL)?;
    let actual_binary_digest = dembly_core::sha256_file(&binary)
        .map(|digest| format!("sha256:{digest}"))
        .map_err(|error| CliError::new(error.to_string()))?;
    if actual_binary_digest != expected_binary_digest {
        return Err(CliError::new(format!(
            "Runtime binary digest does not match applied artifact identity: {}",
            binary.display()
        )));
    }
    Ok(())
}

fn applied_label<'a>(
    state: &'a dembly_docker::ApplyState,
    label: &str,
) -> Result<&'a str, CliError> {
    match state
        .fields
        .labels
        .entries
        .get(label)
        .map(|entry| &entry.applied)
    {
        Some(dembly_docker::ManagedValue::Present(Value::String(value))) => Ok(value),
        _ => Err(CliError::new(format!(
            "Dembly managed label {label} is missing or invalid in applied state"
        ))),
    }
}

fn runtime_check_command(plan: &CheckHostPlan) -> String {
    let compose = plan
        .compose_files
        .iter()
        .map(|path| format!("-f {}", shell_argument(&path.display().to_string())))
        .collect::<Vec<_>>()
        .join(" ");
    let service = shell_argument(&plan.deck.document.compose.service);
    format!(
        "docker compose {compose} run --rm {service} /run/dembly/bin/dembly __runtime check /run/dembly/runtime/{}.toml",
        plan.deck.document.compose.service
    )
}

fn shell_argument(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return value.into();
    }
    format!("'{}'", value.replace('\'', "'\\\"'\\\"'"))
}
