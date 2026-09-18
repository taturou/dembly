use crate::{
    expand_bind_source, expand_bind_target, load_card, load_config, plan_environment,
    resolve_volume_path, validate_mount_targets, validate_shared_volume_consistency, CardDocument,
    CardEnvironment, ConfigDocument, CoreError, HostVariables, MountResource, VolumeOwner,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ResolvedDeck {
    pub path: PathBuf,
    pub root: PathBuf,
    pub document: ConfigDocument,
    pub cards: Vec<ResolvedCard>,
    pub volumes: Vec<ResolvedVolume>,
    pub binds: Vec<ResolvedBind>,
    pub environment: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedCard {
    pub manifest_path: PathBuf,
    pub document: CardDocument,
}
#[derive(Clone, Debug)]
pub struct ResolvedVolume {
    pub owner: VolumeOwner,
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub shared: bool,
}
#[derive(Clone, Debug)]
pub struct ResolvedBind {
    pub owner: String,
    pub declared_source: String,
    pub source: PathBuf,
    pub target: String,
    pub mode: String,
    pub required: bool,
}

pub fn resolve_deck(
    config_path: &Path,
    variables: &HostVariables,
) -> Result<ResolvedDeck, CoreError> {
    let path = config_path.canonicalize().map_err(|error| {
        CoreError::parse(config_path, format!("cannot resolve config path: {error}"))
    })?;
    let root = path
        .parent()
        .ok_or_else(|| CoreError::parse(&path, "config.toml has no parent directory"))?
        .to_path_buf();
    let deck = load_config(&path)?;
    if deck.compose.path.is_empty() || deck.compose.service.is_empty() {
        return Err(error("Compose path and service are required"));
    }
    let host = HostVariables {
        host_home: variables.host_home.clone(),
        deck_root: root.clone(),
    };
    let mut names = BTreeSet::new();
    let mut cards = Vec::new();
    for reference in &deck.cards {
        let manifest_path = resolve_card_path(&root, &reference.path)?;
        let card = load_card(&manifest_path)?;
        validate_card(&manifest_path, &card)?;
        if !names.insert(card.name.clone()) {
            return Err(error(format!("duplicate Card name: {}", card.name)));
        }
        cards.push(ResolvedCard {
            manifest_path,
            document: card,
        });
    }
    let mut declarations = deck
        .volumes
        .iter()
        .map(|volume| (volume.name.clone(), volume.shared))
        .collect::<Vec<_>>();
    for card in &cards {
        declarations.extend(
            card.document
                .volumes
                .iter()
                .map(|volume| (volume.name.clone(), volume.shared)),
        );
    }
    validate_shared_volume_consistency(&declarations)?;
    let mut volumes = Vec::new();
    for volume in &deck.volumes {
        volumes.push(resolve_volume(&root, VolumeOwner::Deck, volume)?);
    }
    for card in &cards {
        for volume in &card.document.volumes {
            volumes.push(resolve_volume(
                &root,
                VolumeOwner::Card(card.document.name.clone()),
                volume,
            )?);
        }
    }
    let mut binds = Vec::new();
    let mut warnings = Vec::new();
    for bind in &deck.binds {
        resolve_bind(&root, "deck".into(), bind, &host, &mut binds, &mut warnings)?;
    }
    for card in &cards {
        for bind in &card.document.binds {
            resolve_bind(
                &root,
                format!("card:{}", card.document.name),
                bind,
                &host,
                &mut binds,
                &mut warnings,
            )?;
        }
    }
    let mut mounts = cards
        .iter()
        .map(|card| {
            MountResource::new(
                format!("card:{}", card.document.name),
                &card.document.mount.target,
            )
        })
        .collect::<Vec<_>>();
    mounts.extend(
        volumes
            .iter()
            .map(|volume| MountResource::new(format!("volume:{}", volume.name), &volume.target)),
    );
    mounts.extend(
        binds
            .iter()
            .map(|bind| MountResource::new(&bind.owner, runtime_target_path(&bind.target))),
    );
    validate_mount_targets(&mounts)?;
    validate_exports(&cards)?;
    let card_environment = cards
        .iter()
        .map(|card| {
            CardEnvironment::new(
                &card.document.mount.target,
                card.document.environment.clone(),
                card.document.environment_path_prepend.clone(),
            )
        })
        .collect::<Vec<_>>();
    let environment = plan_environment(
        BTreeMap::new(),
        deck.environment.clone(),
        &card_environment,
        &deck.environment_path_prepend,
    )?;
    Ok(ResolvedDeck {
        path,
        root,
        document: deck,
        cards,
        volumes,
        binds,
        environment,
        warnings,
    })
}

fn validate_schema(path: &Path, schema: u32, kind: &str) -> Result<(), CoreError> {
    if schema == 1 {
        Ok(())
    } else {
        Err(CoreError::parse(
            path,
            format!("{kind} schema_version must be 1"),
        ))
    }
}
fn validate_card(path: &Path, card: &CardDocument) -> Result<(), CoreError> {
    validate_schema(path, card.schema_version, "Card")?;
    if card.name.is_empty()
        || !card
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(CoreError::parse(
            path,
            format!("invalid Card name: {}", card.name),
        ));
    }
    if card.filesystem.file_type != "squashfs" {
        return Err(CoreError::parse(path, "filesystem.type must be squashfs"));
    }
    if card.filesystem.sha256.len() != 64
        || !card
            .filesystem
            .sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(CoreError::parse(
            path,
            "filesystem.sha256 must be lowercase hex SHA-256",
        ));
    }
    if !Path::new(&card.mount.target).is_absolute()
        || card.mount.target.contains("..")
        || card.mount.target.contains('$')
    {
        return Err(CoreError::parse(
            path,
            format!("invalid Card mount target: {}", card.mount.target),
        ));
    }
    for export in &card.exports {
        if Path::new(&export.source).is_absolute()
            || export.source.split('/').any(|part| part == "..")
            || !Path::new(&export.target).is_absolute()
            || !export.target.starts_with("/usr/local/bin/")
        {
            return Err(CoreError::parse(path, "invalid Card export"));
        }
    }
    for hook in &card.post_mount_hooks {
        validate_card_relative(path, &hook.exec, "hook exec")?;
    }
    if let Some(check) = &card.check {
        validate_card_relative(path, &check.exec, "check exec")?;
    }
    Ok(())
}
fn validate_card_relative(path: &Path, value: &str, field: &str) -> Result<(), CoreError> {
    if value.is_empty()
        || Path::new(value).is_absolute()
        || value.split('/').any(|part| part == "..")
    {
        Err(CoreError::parse(
            path,
            format!("{field} must be a Card-root relative path"),
        ))
    } else {
        Ok(())
    }
}
fn resolve_volume(
    root: &Path,
    owner: VolumeOwner,
    volume: &crate::Volume,
) -> Result<ResolvedVolume, CoreError> {
    if !Path::new(&volume.target).is_absolute() || volume.target.contains('$') {
        return Err(error(format!(
            "Volume {} target must be an absolute Runtime path",
            volume.name
        )));
    }
    let source = resolve_volume_path(root, &owner, &volume.name, volume.shared)?;
    if source.is_symlink() {
        return Err(error(format!(
            "Volume physical path is a symlink: {}",
            source.display()
        )));
    }
    Ok(ResolvedVolume {
        owner,
        name: volume.name.clone(),
        source,
        target: PathBuf::from(&volume.target),
        shared: volume.shared,
    })
}
fn resolve_bind(
    root: &Path,
    owner: String,
    bind: &crate::Bind,
    variables: &HostVariables,
    result: &mut Vec<ResolvedBind>,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    if bind.mode != "ro" && bind.mode != "rw" {
        return Err(error(format!(
            "Host Bind mode must be ro or rw: {}",
            bind.mode
        )));
    }
    let expanded = expand_bind_source(&bind.source, variables)?;
    let source = if expanded.is_absolute() {
        expanded
    } else {
        normalize_relative(
            root,
            expanded.to_string_lossy().as_ref(),
            "Host Bind source",
        )?
    };
    let target = expand_bind_target(&bind.target)?;
    if !source.exists() {
        if bind.required {
            return Err(error(format!(
                "missing required Host Bind source: {}",
                source.display()
            )));
        }
        warnings.push(format!(
            "optional Host Bind source does not exist; skipping: {}",
            source.display()
        ));
        return Ok(());
    }
    result.push(ResolvedBind {
        owner,
        declared_source: bind.source.clone(),
        source,
        target,
        mode: bind.mode.clone(),
        required: bind.required,
    });
    Ok(())
}
fn validate_exports(cards: &[ResolvedCard]) -> Result<(), CoreError> {
    let mut targets = BTreeSet::new();
    for card in cards {
        for export in &card.document.exports {
            if !targets.insert(export.target.clone()) {
                return Err(error(format!("duplicate export target: {}", export.target)));
            }
        }
    }
    Ok(())
}
fn normalize_relative(root: &Path, value: &str, label: &str) -> Result<PathBuf, CoreError> {
    let value = Path::new(value);
    if value.is_absolute()
        || value
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(error(format!("{label} must be a Deck-root relative path")));
    }
    Ok(root.join(value))
}

fn resolve_card_path(root: &Path, value: &str) -> Result<PathBuf, CoreError> {
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        normalize_relative(root, value, "Card manifest path")
    }
}

fn runtime_target_path(value: &str) -> PathBuf {
    PathBuf::from(
        value
            .replace("${HOME}", "/runtime/home")
            .replace("${USER}", "runtime-user"),
    )
}
fn error(message: impl Into<String>) -> CoreError {
    CoreError::parse("<resolved deck>", message)
}
