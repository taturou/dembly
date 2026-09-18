use crate::args::CliError;
use dembly_core::{
    plan_environment, resolve_deck, verify_card_filesystem, CardEnvironment, HostVariables,
    ResolvedDeck,
};
use dembly_docker::{
    inspect_compose, inspect_image, load_devcontainer, validate_devcontainer, ApplyState,
    DevContainerDocument, EffectiveService, FieldState, ImageConfig, ManagedCompose, ManagedValue,
};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub struct HostPlan {
    pub deck: ResolvedDeck,
    pub managed_compose: ManagedCompose,
    pub compose_files: Vec<PathBuf>,
    pub devcontainer: Option<DevContainerDocument>,
    pub effective_service: EffectiveService,
    pub image: ImageConfig,
    pub environment: BTreeMap<String, String>,
}

impl HostPlan {
    pub fn resolve(config_path: &Path) -> Result<Self, CliError> {
        let host_home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::new("HOME is required to resolve Host paths"))?;
        let variables = HostVariables {
            host_home,
            deck_root: config_path
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf(),
        };
        let deck = resolve_deck(config_path, &variables).map_err(core_error)?;
        for card in &deck.cards {
            verify_card_filesystem(&card.manifest_path, &card.document).map_err(core_error)?;
        }

        let managed_compose = ManagedCompose::read(&deck.compose_path).map_err(docker_error)?;
        let service_name = &deck.document.compose.service;
        managed_compose
            .service(service_name)
            .map_err(docker_error)?;

        let (devcontainer, compose_files) = match &deck.document.devcontainer {
            Some(reference) => {
                let path = resolve_deck_path(&deck.root, &reference.path);
                let document = load_devcontainer(&path).map_err(docker_error)?;
                // The effective user is not known until the ordered Compose file set is
                // inspected. All other cross-file constraints can be checked first.
                let files = validate_devcontainer(
                    &document,
                    &path,
                    &deck.compose_path,
                    service_name,
                    &document.remote_user,
                )
                .map_err(docker_error)?;
                (Some((path, document)), files)
            }
            None => (None, vec![deck.compose_path.clone()]),
        };

        let mut effective_service =
            inspect_compose(&compose_files, service_name).map_err(docker_error)?;
        if let Some(state) = managed_compose.state().map_err(docker_error)? {
            let inherited_service = if compose_files.len() > 1 && needs_inherited_service(&state) {
                Some(
                    inspect_compose(&compose_files[..compose_files.len() - 1], service_name)
                        .map_err(docker_error)?,
                )
            } else {
                None
            };
            restore_original_service_fields(
                &mut effective_service,
                inherited_service.as_ref(),
                &state,
                service_name,
            )?;
        }
        let image = inspect_image(&effective_service.image).map_err(docker_error)?;
        let intended_user = select_intended_user(&effective_service, &image);

        let devcontainer = match devcontainer {
            Some((path, document)) => {
                let remote_user = user_portion(intended_user);
                let validated_files = validate_devcontainer(
                    &document,
                    &path,
                    &deck.compose_path,
                    service_name,
                    remote_user,
                )
                .map_err(docker_error)?;
                if validated_files != compose_files {
                    return Err(CliError::new(
                        "devcontainer Compose file order changed during resolution",
                    ));
                }
                Some(document)
            }
            None => None,
        };

        let environment = resolve_environment(&deck, &effective_service, &image)?;
        Ok(Self {
            deck,
            managed_compose,
            compose_files,
            devcontainer,
            effective_service,
            image,
            environment,
        })
    }

    pub fn intended_user_spec(&self) -> &str {
        select_intended_user(&self.effective_service, &self.image)
    }
}

fn restore_original_service_fields(
    service: &mut EffectiveService,
    inherited: Option<&EffectiveService>,
    state: &ApplyState,
    expected_service: &str,
) -> Result<(), CliError> {
    if state.service != expected_service {
        return Err(CliError::new(format!(
            "applied state service {} does not match configured service {expected_service}",
            state.service
        )));
    }
    service.entrypoint = original_argv(
        &state.fields.entrypoint,
        inherited.and_then(|service| service.entrypoint.clone()),
        "entrypoint",
    )?;
    service.command = original_argv(
        &state.fields.command,
        inherited.and_then(|service| service.command.clone()),
        "command",
    )?;
    service.user = original_user(
        &state.fields.user,
        inherited.and_then(|service| service.user.clone()),
    )?;
    Ok(())
}

fn needs_inherited_service(state: &ApplyState) -> bool {
    [
        &state.fields.entrypoint,
        &state.fields.command,
        &state.fields.user,
    ]
    .iter()
    .any(|field| matches!(field.original, ManagedValue::Missing))
}

fn original_argv(
    field: &FieldState,
    inherited: Option<Vec<String>>,
    name: &str,
) -> Result<Option<Vec<String>>, CliError> {
    match &field.original {
        ManagedValue::Missing => Ok(inherited),
        ManagedValue::Present(serde_yaml::Value::Null) => Ok(None),
        ManagedValue::Present(serde_yaml::Value::String(value)) => Ok(Some(vec![value.clone()])),
        ManagedValue::Present(serde_yaml::Value::Sequence(values)) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    CliError::new(format!(
                        "applied state original {name} must contain only strings"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        ManagedValue::Present(_) => Err(CliError::new(format!(
            "applied state original {name} must be a string, sequence, null, or missing"
        ))),
    }
}

fn original_user(
    field: &FieldState,
    inherited: Option<String>,
) -> Result<Option<String>, CliError> {
    match &field.original {
        ManagedValue::Missing => Ok(inherited),
        ManagedValue::Present(serde_yaml::Value::Null) => Ok(None),
        ManagedValue::Present(serde_yaml::Value::String(value)) => Ok(Some(value.clone())),
        ManagedValue::Present(_) => Err(CliError::new(
            "applied state original user must be a string, null, or missing",
        )),
    }
}

fn resolve_environment(
    deck: &ResolvedDeck,
    service: &EffectiveService,
    image: &ImageConfig,
) -> Result<BTreeMap<String, String>, CliError> {
    let mut base = image
        .environment
        .iter()
        .map(|entry| match entry.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (entry.clone(), String::new()),
        })
        .collect::<BTreeMap<_, _>>();
    for (key, value) in &service.environment {
        match value {
            Some(value) => {
                base.insert(key.clone(), value.clone());
            }
            None => {
                base.remove(key);
            }
        }
    }
    let cards = deck
        .cards
        .iter()
        .map(|card| {
            CardEnvironment::new(
                &card.document.mount.target,
                card.document.environment.clone(),
                card.document.environment_path_prepend.clone(),
            )
        })
        .collect::<Vec<_>>();
    plan_environment(
        base,
        deck.document.environment.clone(),
        &cards,
        &deck.document.environment_path_prepend,
    )
    .map_err(core_error)
}

fn select_intended_user<'a>(service: &'a EffectiveService, image: &'a ImageConfig) -> &'a str {
    service
        .user
        .as_deref()
        .filter(|user| !user.is_empty())
        .or_else(|| (!image.user.is_empty()).then_some(image.user.as_str()))
        .unwrap_or("root")
}

fn user_portion(spec: &str) -> &str {
    spec.split_once(':').map_or(spec, |(user, _)| user)
}

fn resolve_deck_path(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    normalize_path(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(component) => normalized.push(component),
        }
    }
    normalized
}

fn core_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

fn docker_error(error: impl Into<String>) -> CliError {
    CliError::new(error)
}
