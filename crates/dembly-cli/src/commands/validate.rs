use dembly_cli::{CliError, HostContext, HostPlan};
use std::io::Write;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let plan = HostPlan::resolve(&context.config_path)?;
    if plan
        .managed_compose
        .state()
        .map_err(CliError::new)?
        .is_some()
    {
        plan.managed_compose
            .validate_applied_state(&plan.deck.document.compose.service)
            .map_err(|error| CliError::new(error.to_string()))?;
    }
    write_warnings(&plan);
    writeln!(output, "valid: {}", plan.deck.path.display())
        .map_err(|error| CliError::new(format!("cannot write validation result: {error}")))
}

pub(crate) fn write_warnings(plan: &HostPlan) {
    for warning in &plan.deck.warnings {
        eprintln!("warning: {warning}");
    }
}
