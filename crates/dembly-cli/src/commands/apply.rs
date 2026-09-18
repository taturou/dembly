use crate::commands::lock::resolved_lock;
use crate::commands::validate::write_warnings;
use dembly_cli::artifacts::{build_managed_fields, ApplyArtifacts};
use dembly_cli::{CliError, HostContext, HostPlan};
use std::io::Write;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let mut plan = HostPlan::resolve(&context.config_path)?;
    write_warnings(&plan);
    let lock = plan
        .managed_compose
        .lock()
        .map_err(CliError::new)?
        .ok_or_else(|| CliError::new("Dembly Lock is missing; run dembly lock"))?;
    if lock != resolved_lock(&plan)? {
        return Err(CliError::new("Dembly Lock is stale; run dembly lock"));
    }
    let lock_digest = lock.digest();
    let executable = std::env::current_exe()
        .map_err(|error| CliError::new(format!("cannot locate current executable: {error}")))?;
    let artifacts = ApplyArtifacts::prepare(&plan, &lock_digest, &executable)?;
    let desired = build_managed_fields(
        &plan,
        artifacts.runtime_path(),
        &lock_digest,
        artifacts.runtime_digest(),
        artifacts.binary_digest(),
    );
    plan.managed_compose
        .apply(&plan.deck.document.compose.service, desired, &lock_digest)
        .map_err(|error| CliError::new(error.to_string()))?;
    let compose_bytes = plan.managed_compose.to_bytes().map_err(CliError::new)?;

    artifacts.write(&plan.deck.compose_path, &compose_bytes)?;

    writeln!(output, "applied: {}", plan.deck.compose_path.display())
        .and_then(|()| writeln!(output, "lock: {lock_digest}"))
        .map_err(|error| CliError::new(format!("cannot write apply result: {error}")))
}
