use crate::commands::validate::write_warnings;
use dembly_cli::{CliError, HostContext, HostPlan};
use dembly_core::{sha256_file, CardIdentity, LockInput, ResolvedDeck};
use dembly_docker::atomic_replace;
use std::io::Write;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let mut plan = HostPlan::resolve(&context.config_path)?;
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
    let lock = resolved_lock(&plan)?;
    plan.managed_compose
        .set_lock(lock.clone())
        .map_err(CliError::new)?;
    let bytes = plan.managed_compose.to_bytes().map_err(CliError::new)?;
    atomic_replace(&plan.deck.compose_path, &bytes).map_err(CliError::new)?;

    writeln!(output, "locked: {}", plan.deck.compose_path.display())
        .and_then(|()| writeln!(output, "image: {}", lock.image))
        .and_then(|()| writeln!(output, "cards:"))
        .and_then(|()| {
            lock.cards
                .iter()
                .try_for_each(|card| writeln!(output, "  - {} {}", card.name, card.version))
        })
        .map_err(|error| CliError::new(format!("cannot write Lock result: {error}")))
}

pub(crate) fn resolved_lock(plan: &HostPlan) -> Result<LockInput, CliError> {
    resolved_lock_from_deck(&plan.deck, plan.image.id.clone())
}

pub(crate) fn resolved_lock_from_deck(
    deck: &ResolvedDeck,
    image: String,
) -> Result<LockInput, CliError> {
    let cards = deck
        .cards
        .iter()
        .map(|card| {
            Ok(CardIdentity {
                name: card.document.name.clone(),
                version: card.document.version.clone(),
                manifest_sha256: sha256_file(&card.manifest_path)
                    .map_err(|error| CliError::new(error.to_string()))?,
                filesystem_sha256: card.document.filesystem.sha256.clone(),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(LockInput {
        compose_path: deck.compose_path.display().to_string(),
        service: deck.document.compose.service.clone(),
        image,
        cards,
    })
}
