use crate::{RuntimeCard, RuntimeUserSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    pub lock_digest: String,
    pub runtime_user: RuntimeUserSpec,
    pub cards: Vec<RuntimeCard>,
    pub volume_targets: Vec<PathBuf>,
    pub binds: Vec<RuntimeBind>,
    pub exports: Vec<RuntimeExport>,
    pub hooks: Vec<RuntimeHook>,
    pub checks: Vec<RuntimeCheck>,
    pub environment: BTreeMap<String, String>,
    pub process_argv: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBind {
    pub source: PathBuf,
    pub target: String,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeExport {
    pub source: PathBuf,
    pub target: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHook {
    pub card: String,
    pub exec: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCheck {
    pub card: String,
    pub exec: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfigDocument {
    schema_version: u32,
    lock_digest: String,
    runtime_user: RuntimeUserSpec,
    #[serde(default)]
    cards: Vec<RuntimeCard>,
    #[serde(default)]
    volume_targets: Vec<PathBuf>,
    #[serde(default)]
    binds: Vec<RuntimeBind>,
    #[serde(default)]
    exports: Vec<RuntimeExport>,
    #[serde(default)]
    hooks: Vec<RuntimeHook>,
    #[serde(default)]
    checks: Vec<RuntimeCheck>,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    process: RuntimeProcess,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProcess {
    argv: Vec<String>,
}

pub fn load_runtime_config(path: &Path) -> Result<RuntimeConfig, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("cannot read runtime config {}: {error}", path.display()))?;
    let document: RuntimeConfigDocument = toml::from_str(&content)
        .map_err(|error| format!("cannot parse runtime config {}: {error}", path.display()))?;
    RuntimeConfig::try_from(document)
}

pub fn render_runtime_config(config: &RuntimeConfig) -> Result<String, String> {
    validate_header(config.schema_version, &config.lock_digest)?;
    toml::to_string(&RuntimeConfigDocument::from(config.clone()))
        .map_err(|error| format!("cannot render runtime config: {error}"))
}

fn validate_header(schema_version: u32, lock_digest: &str) -> Result<(), String> {
    if schema_version != 1 {
        return Err("runtime schema_version must be 1".into());
    }
    if lock_digest.trim().is_empty() {
        return Err("runtime lock_digest must not be empty".into());
    }
    Ok(())
}

impl TryFrom<RuntimeConfigDocument> for RuntimeConfig {
    type Error = String;

    fn try_from(document: RuntimeConfigDocument) -> Result<Self, Self::Error> {
        validate_header(document.schema_version, &document.lock_digest)?;
        Ok(Self {
            schema_version: document.schema_version,
            lock_digest: document.lock_digest,
            runtime_user: document.runtime_user,
            cards: document.cards,
            volume_targets: document.volume_targets,
            binds: document.binds,
            exports: document.exports,
            hooks: document.hooks,
            checks: document.checks,
            environment: document.environment,
            process_argv: document.process.argv,
        })
    }
}

impl From<RuntimeConfig> for RuntimeConfigDocument {
    fn from(config: RuntimeConfig) -> Self {
        Self {
            schema_version: config.schema_version,
            lock_digest: config.lock_digest,
            runtime_user: config.runtime_user,
            cards: config.cards,
            volume_targets: config.volume_targets,
            binds: config.binds,
            exports: config.exports,
            hooks: config.hooks,
            checks: config.checks,
            environment: config.environment,
            process: RuntimeProcess {
                argv: config.process_argv,
            },
        }
    }
}
