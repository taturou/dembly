use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const X_DEMBLY: &str = "x-dembly";

#[derive(Clone, Debug)]
pub struct ManagedCompose {
    path: PathBuf,
    document: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceSnapshot {
    pub entrypoint: ManagedValue,
    pub command: ManagedValue,
    pub user: ManagedValue,
    pub privileged: ManagedValue,
    pub labels: ManagedValue,
    pub mounts: ManagedValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ManagedValue {
    Missing,
    Present(Value),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedFields {
    pub entrypoint: Value,
    pub command: Value,
    pub user: Value,
    pub privileged: Value,
    pub labels: BTreeMap<String, Value>,
    pub mounts: Vec<ManagedMount>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedMount {
    pub target: String,
    pub value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardLock {
    pub name: String,
    pub version: String,
    pub manifest_sha256: String,
    pub filesystem_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemblyLock {
    pub compose_path: String,
    pub service: String,
    pub image: String,
    pub cards: Vec<CardLock>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyState {
    pub service: String,
    pub lock_digest: String,
    pub fields: AppliedFields,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedFields {
    pub entrypoint: FieldState,
    pub command: FieldState,
    pub user: FieldState,
    pub privileged: FieldState,
    pub labels: LabelState,
    pub mounts: MountState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldState {
    pub original: ManagedValue,
    pub applied: ManagedValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelState {
    pub container_original: ManagedValue,
    pub entries: BTreeMap<String, FieldState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountState {
    pub container_original: ManagedValue,
    pub entries: Vec<ManagedMountState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedMountState {
    pub target: String,
    pub original: ManagedValue,
    pub applied: ManagedValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    pub service: String,
    pub field: String,
    pub expected: ManagedValue,
    pub current: ManagedValue,
}

impl fmt::Display for Conflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "service {} field {} conflicts: expected {}, current {}",
            self.service,
            self.field,
            render_managed_value(&self.expected),
            render_managed_value(&self.current)
        )
    }
}

impl std::error::Error for Conflict {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DemblyExtension {
    schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lock: Option<DemblyLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state: Option<ApplyState>,
}

impl DemblyExtension {
    fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            lock: None,
            state: None,
        }
    }

    fn validate(self) -> Result<Self, String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "x-dembly schema_version must be {SCHEMA_VERSION}, got {}",
                self.schema_version
            ));
        }
        Ok(self)
    }
}

impl ManagedCompose {
    pub fn read(path: &Path) -> Result<Self, String> {
        let source = fs::read(path)
            .map_err(|error| format!("cannot read managed Compose {}: {error}", path.display()))?;
        let document: Value = serde_yaml::from_slice(&source)
            .map_err(|error| format!("cannot parse managed Compose {}: {error}", path.display()))?;
        let compose = Self {
            path: path.to_path_buf(),
            document,
        };
        compose.project_name()?;
        compose.extension()?;
        Ok(compose)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn project_name(&self) -> Result<&str, String> {
        let name = self
            .root()?
            .get(key("name"))
            .and_then(Value::as_str)
            .ok_or("managed Compose requires a string top-level name")?;
        if valid_project_name(name) {
            Ok(name)
        } else {
            Err(format!("invalid Compose project name: {name}"))
        }
    }

    pub fn service(&self, name: &str) -> Result<ServiceSnapshot, String> {
        let service = self.service_mapping(name)?;
        Ok(ServiceSnapshot {
            entrypoint: field(service, "entrypoint"),
            command: field(service, "command"),
            user: field(service, "user"),
            privileged: field(service, "privileged"),
            labels: field(service, "labels"),
            mounts: field(service, "volumes"),
        })
    }

    pub fn lock(&self) -> Result<Option<DemblyLock>, String> {
        Ok(self.extension()?.and_then(|extension| extension.lock))
    }

    pub fn state(&self) -> Result<Option<ApplyState>, String> {
        Ok(self.extension()?.and_then(|extension| extension.state))
    }

    pub fn set_lock(&mut self, lock: DemblyLock) -> Result<(), String> {
        let mut extension = self.extension()?.unwrap_or_else(DemblyExtension::empty);
        extension.lock = Some(lock);
        self.set_extension(extension)
    }

    pub fn apply(
        &mut self,
        service_name: &str,
        desired: ManagedFields,
        lock_digest: &str,
    ) -> Result<(), Conflict> {
        validate_desired_mounts(service_name, &desired.mounts)?;
        let extension = self
            .extension()
            .map_err(|error| structural_conflict(service_name, X_DEMBLY, error))?
            .unwrap_or_else(DemblyExtension::empty);
        let existing = extension.state.clone();

        if let Some(state) = &existing {
            if state.service != service_name {
                return Err(Conflict::new(
                    service_name,
                    "state.service",
                    ManagedValue::Present(string(service_name)),
                    ManagedValue::Present(string(&state.service)),
                ));
            }
            let service = self
                .service_mapping(service_name)
                .map_err(|error| structural_conflict(service_name, "service", error))?;
            validate_applied_state(service_name, service, state)?;
        } else {
            self.service_mapping(service_name)
                .map_err(|error| structural_conflict(service_name, "service", error))?;
        }

        let mut candidate = self.clone();
        let next_state = {
            let service = candidate
                .service_mapping_mut(service_name)
                .map_err(|error| structural_conflict(service_name, "service", error))?;
            update_service(service_name, service, existing, desired, lock_digest)?
        };
        let mut next_extension = extension;
        next_extension.state = Some(next_state);
        candidate
            .set_extension(next_extension)
            .map_err(|error| structural_conflict(service_name, X_DEMBLY, error))?;
        *self = candidate;
        Ok(())
    }

    pub fn unapply(&mut self, service_name: &str) -> Result<(), Conflict> {
        let mut extension = self
            .extension()
            .map_err(|error| structural_conflict(service_name, X_DEMBLY, error))?
            .unwrap_or_else(DemblyExtension::empty);
        let state = extension.state.clone().ok_or_else(|| {
            Conflict::new(
                service_name,
                "state",
                ManagedValue::Present(string("applied state")),
                ManagedValue::Missing,
            )
        })?;
        if state.service != service_name {
            return Err(Conflict::new(
                service_name,
                "state.service",
                ManagedValue::Present(string(service_name)),
                ManagedValue::Present(string(&state.service)),
            ));
        }
        let service = self
            .service_mapping(service_name)
            .map_err(|error| structural_conflict(service_name, "service", error))?;
        validate_applied_state(service_name, service, &state)?;

        let mut candidate = self.clone();
        {
            let service = candidate
                .service_mapping_mut(service_name)
                .map_err(|error| structural_conflict(service_name, "service", error))?;
            restore_field(service, "entrypoint", &state.fields.entrypoint.original);
            restore_field(service, "command", &state.fields.command.original);
            restore_field(service, "user", &state.fields.user.original);
            restore_field(service, "privileged", &state.fields.privileged.original);
            restore_labels(service_name, service, &state.fields.labels)?;
            restore_mounts(service_name, service, &state.fields.mounts)?;
        }
        extension.state = None;
        candidate
            .set_extension(extension)
            .map_err(|error| structural_conflict(service_name, X_DEMBLY, error))?;
        *self = candidate;
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut output = serde_yaml::to_string(&self.document)
            .map_err(|error| format!("cannot serialize managed Compose: {error}"))?
            .into_bytes();
        if !output.ends_with(b"\n") {
            output.push(b'\n');
        }
        Ok(output)
    }

    fn root(&self) -> Result<&Mapping, String> {
        self.document
            .as_mapping()
            .ok_or("managed Compose root must be a mapping".into())
    }

    fn root_mut(&mut self) -> Result<&mut Mapping, String> {
        self.document
            .as_mapping_mut()
            .ok_or("managed Compose root must be a mapping".into())
    }

    fn service_mapping(&self, name: &str) -> Result<&Mapping, String> {
        self.root()?
            .get(key("services"))
            .and_then(Value::as_mapping)
            .ok_or("managed Compose services must be a mapping")?
            .get(key(name))
            .and_then(Value::as_mapping)
            .ok_or_else(|| format!("managed Compose has no mapping service {name}"))
    }

    fn service_mapping_mut(&mut self, name: &str) -> Result<&mut Mapping, String> {
        self.root_mut()?
            .get_mut(key("services"))
            .and_then(Value::as_mapping_mut)
            .ok_or("managed Compose services must be a mapping")?
            .get_mut(key(name))
            .and_then(Value::as_mapping_mut)
            .ok_or_else(|| format!("managed Compose has no mapping service {name}"))
    }

    fn extension(&self) -> Result<Option<DemblyExtension>, String> {
        let Some(value) = self.root()?.get(key(X_DEMBLY)) else {
            return Ok(None);
        };
        serde_yaml::from_value::<DemblyExtension>(value.clone())
            .map_err(|error| format!("invalid x-dembly: {error}"))
            .and_then(DemblyExtension::validate)
            .map(Some)
    }

    fn set_extension(&mut self, extension: DemblyExtension) -> Result<(), String> {
        let value = serde_yaml::to_value(extension)
            .map_err(|error| format!("cannot serialize x-dembly: {error}"))?;
        self.root_mut()?.insert(key(X_DEMBLY), value);
        Ok(())
    }
}

impl Conflict {
    fn new(
        service: impl Into<String>,
        field: impl Into<String>,
        expected: ManagedValue,
        current: ManagedValue,
    ) -> Self {
        Self {
            service: service.into(),
            field: field.into(),
            expected,
            current,
        }
    }
}

fn update_service(
    service_name: &str,
    service: &mut Mapping,
    existing: Option<ApplyState>,
    desired: ManagedFields,
    lock_digest: &str,
) -> Result<ApplyState, Conflict> {
    let entrypoint = update_scalar(
        service,
        "entrypoint",
        existing.as_ref().map(|state| &state.fields.entrypoint),
        desired.entrypoint,
    );
    let command = update_scalar(
        service,
        "command",
        existing.as_ref().map(|state| &state.fields.command),
        desired.command,
    );
    let user = update_scalar(
        service,
        "user",
        existing.as_ref().map(|state| &state.fields.user),
        desired.user,
    );
    let privileged = update_scalar(
        service,
        "privileged",
        existing.as_ref().map(|state| &state.fields.privileged),
        desired.privileged,
    );
    let labels = update_labels(
        service_name,
        service,
        existing.as_ref().map(|state| &state.fields.labels),
        desired.labels,
    )?;
    let mounts = update_mounts(
        service_name,
        service,
        existing.as_ref().map(|state| &state.fields.mounts),
        desired.mounts,
    )?;
    Ok(ApplyState {
        service: service_name.into(),
        lock_digest: lock_digest.into(),
        fields: AppliedFields {
            entrypoint,
            command,
            user,
            privileged,
            labels,
            mounts,
        },
    })
}

fn update_scalar(
    service: &mut Mapping,
    name: &str,
    existing: Option<&FieldState>,
    desired: Value,
) -> FieldState {
    let original = existing
        .map(|state| state.original.clone())
        .unwrap_or_else(|| field(service, name));
    let applied = ManagedValue::Present(desired);
    restore_field(service, name, &applied);
    FieldState { original, applied }
}

fn update_labels(
    service_name: &str,
    service: &mut Mapping,
    existing: Option<&LabelState>,
    desired: BTreeMap<String, Value>,
) -> Result<LabelState, Conflict> {
    let container_original = existing
        .map(|state| state.container_original.clone())
        .unwrap_or_else(|| field(service, "labels"));
    let mut entries = existing
        .map(|state| state.entries.clone())
        .unwrap_or_default();

    let removed = entries
        .keys()
        .filter(|label| !desired.contains_key(*label))
        .cloned()
        .collect::<Vec<_>>();
    for label in removed {
        let state = entries.remove(&label).expect("label key came from map");
        set_label(service_name, service, &label, &state.original)?;
    }
    restore_empty_container(service, "labels", &container_original);

    for (label, value) in desired {
        let original = match entries.get(&label) {
            Some(state) => state.original.clone(),
            None => label_value(service_name, service, &label)?,
        };
        let applied = put_label(service_name, service, &label, value)?;
        entries.insert(label, FieldState { original, applied });
    }
    Ok(LabelState {
        container_original,
        entries,
    })
}

fn update_mounts(
    service_name: &str,
    service: &mut Mapping,
    existing: Option<&MountState>,
    desired: Vec<ManagedMount>,
) -> Result<MountState, Conflict> {
    let container_original = existing
        .map(|state| state.container_original.clone())
        .unwrap_or_else(|| field(service, "volumes"));
    let entries = existing
        .map(|state| state.entries.clone())
        .unwrap_or_default();
    for mount in &entries {
        set_mount(service_name, service, &mount.target, &mount.original)?;
    }
    restore_empty_container(service, "volumes", &container_original);

    let mut next_entries = Vec::with_capacity(desired.len());
    for desired_mount in desired {
        if let Some(state) = entries
            .iter()
            .find(|mount| mount.target == desired_mount.target)
        {
            let applied = put_mount(
                service_name,
                service,
                &desired_mount.target,
                desired_mount.value,
            )?;
            next_entries.push(ManagedMountState {
                target: desired_mount.target,
                original: state.original.clone(),
                applied,
            });
        } else {
            let original = mount_value(service_name, service, &desired_mount.target)?;
            let applied = put_mount(
                service_name,
                service,
                &desired_mount.target,
                desired_mount.value,
            )?;
            next_entries.push(ManagedMountState {
                target: desired_mount.target,
                original,
                applied,
            });
        }
    }
    Ok(MountState {
        container_original,
        entries: next_entries,
    })
}

fn validate_applied_state(
    service_name: &str,
    service: &Mapping,
    state: &ApplyState,
) -> Result<(), Conflict> {
    let mut first = None;
    for (name, expected) in [
        ("entrypoint", &state.fields.entrypoint.applied),
        ("command", &state.fields.command.applied),
        ("user", &state.fields.user.applied),
        ("privileged", &state.fields.privileged.applied),
    ] {
        record_conflict(
            &mut first,
            service_name,
            name,
            expected,
            field(service, name),
        );
    }
    for (label, expected) in &state.fields.labels.entries {
        let name = format!("labels.{label}");
        match label_value(service_name, service, label) {
            Ok(current) => {
                record_conflict(&mut first, service_name, &name, &expected.applied, current)
            }
            Err(conflict) => {
                first.get_or_insert(conflict);
            }
        };
    }
    for expected in &state.fields.mounts.entries {
        let name = format!("volumes.{}", expected.target);
        match mount_value(service_name, service, &expected.target) {
            Ok(current) => {
                record_conflict(&mut first, service_name, &name, &expected.applied, current)
            }
            Err(conflict) => {
                first.get_or_insert(conflict);
            }
        };
    }
    first.map_or(Ok(()), Err)
}

fn record_conflict(
    first: &mut Option<Conflict>,
    service: &str,
    name: &str,
    expected: &ManagedValue,
    current: ManagedValue,
) {
    if *expected != current && first.is_none() {
        *first = Some(Conflict::new(service, name, expected.clone(), current));
    }
}

fn restore_labels(
    service_name: &str,
    service: &mut Mapping,
    state: &LabelState,
) -> Result<(), Conflict> {
    for (label, values) in &state.entries {
        set_label(service_name, service, label, &values.original)?;
    }
    restore_empty_container(service, "labels", &state.container_original);
    Ok(())
}

fn restore_mounts(
    service_name: &str,
    service: &mut Mapping,
    state: &MountState,
) -> Result<(), Conflict> {
    for mount in &state.entries {
        set_mount(service_name, service, &mount.target, &mount.original)?;
    }
    restore_empty_container(service, "volumes", &state.container_original);
    Ok(())
}

fn label_value(
    service_name: &str,
    service: &Mapping,
    label: &str,
) -> Result<ManagedValue, Conflict> {
    match service.get(key("labels")) {
        None | Some(Value::Null) => Ok(ManagedValue::Missing),
        Some(Value::Mapping(labels)) => Ok(labels
            .get(key(label))
            .cloned()
            .map(ManagedValue::Present)
            .unwrap_or(ManagedValue::Missing)),
        Some(Value::Sequence(labels)) => {
            let matches = labels
                .iter()
                .filter(|value| label_key(value).is_some_and(|key| key == label))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [] => Ok(ManagedValue::Missing),
                [value] => Ok(ManagedValue::Present((*value).clone())),
                _ => Err(structural_conflict(
                    service_name,
                    format!("labels.{label}"),
                    "duplicate label entries",
                )),
            }
        }
        Some(current) => Err(Conflict::new(
            service_name,
            "labels",
            ManagedValue::Present(string("mapping or sequence")),
            ManagedValue::Present(current.clone()),
        )),
    }
}

fn put_label(
    service_name: &str,
    service: &mut Mapping,
    label: &str,
    desired: Value,
) -> Result<ManagedValue, Conflict> {
    match service.get_mut(key("labels")) {
        None | Some(Value::Null) => {
            let mut labels = Mapping::new();
            labels.insert(key(label), desired.clone());
            service.insert(key("labels"), Value::Mapping(labels));
            Ok(ManagedValue::Present(desired))
        }
        Some(Value::Mapping(labels)) => {
            labels.insert(key(label), desired.clone());
            Ok(ManagedValue::Present(desired))
        }
        Some(Value::Sequence(labels)) => {
            let applied = label_sequence_value(service_name, label, desired)?;
            replace_sequence_label(labels, label, Some(applied.clone()));
            Ok(ManagedValue::Present(applied))
        }
        Some(current) => Err(Conflict::new(
            service_name,
            "labels",
            ManagedValue::Present(string("mapping or sequence")),
            ManagedValue::Present(current.clone()),
        )),
    }
}

fn set_label(
    service_name: &str,
    service: &mut Mapping,
    label: &str,
    value: &ManagedValue,
) -> Result<(), Conflict> {
    match service.get_mut(key("labels")) {
        None | Some(Value::Null) => match value {
            ManagedValue::Missing => Ok(()),
            ManagedValue::Present(value) => {
                let mut labels = Mapping::new();
                labels.insert(key(label), value.clone());
                service.insert(key("labels"), Value::Mapping(labels));
                Ok(())
            }
        },
        Some(Value::Mapping(labels)) => {
            match value {
                ManagedValue::Missing => {
                    labels.remove(key(label));
                }
                ManagedValue::Present(value) => {
                    labels.insert(key(label), value.clone());
                }
            }
            Ok(())
        }
        Some(Value::Sequence(labels)) => {
            let value = match value {
                ManagedValue::Missing => None,
                ManagedValue::Present(value) => Some(value.clone()),
            };
            replace_sequence_label(labels, label, value);
            Ok(())
        }
        Some(current) => Err(Conflict::new(
            service_name,
            "labels",
            ManagedValue::Present(string("mapping or sequence")),
            ManagedValue::Present(current.clone()),
        )),
    }
}

fn replace_sequence_label(labels: &mut Vec<Value>, label: &str, value: Option<Value>) {
    if let Some(index) = labels
        .iter()
        .position(|current| label_key(current).is_some_and(|key| key == label))
    {
        match value {
            Some(value) => labels[index] = value,
            None => {
                labels.remove(index);
            }
        }
    } else if let Some(value) = value {
        labels.push(value);
    }
}

fn label_sequence_value(
    service_name: &str,
    label: &str,
    desired: Value,
) -> Result<Value, Conflict> {
    match desired {
        Value::String(value) => Ok(string(&format!("{label}={value}"))),
        Value::Null => Ok(string(label)),
        current => Err(Conflict::new(
            service_name,
            format!("labels.{label}"),
            ManagedValue::Present(string("string or null for sequence labels")),
            ManagedValue::Present(current),
        )),
    }
}

fn label_key(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(|label| label.split_once('=').map_or(label, |(key, _)| key))
}

fn mount_value(
    service_name: &str,
    service: &Mapping,
    target: &str,
) -> Result<ManagedValue, Conflict> {
    match service.get(key("volumes")) {
        None | Some(Value::Null) => Ok(ManagedValue::Missing),
        Some(Value::Sequence(mounts)) => {
            let matches = mounts
                .iter()
                .filter(|value| mount_has_target(value, target))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [] => Ok(ManagedValue::Missing),
                [value] => Ok(ManagedValue::Present((*value).clone())),
                _ => Err(structural_conflict(
                    service_name,
                    format!("volumes.{target}"),
                    "duplicate mount targets",
                )),
            }
        }
        Some(current) => Err(Conflict::new(
            service_name,
            "volumes",
            ManagedValue::Present(string("sequence")),
            ManagedValue::Present(current.clone()),
        )),
    }
}

fn put_mount(
    service_name: &str,
    service: &mut Mapping,
    target: &str,
    desired: Value,
) -> Result<ManagedValue, Conflict> {
    set_mount(
        service_name,
        service,
        target,
        &ManagedValue::Present(desired.clone()),
    )?;
    Ok(ManagedValue::Present(desired))
}

fn set_mount(
    service_name: &str,
    service: &mut Mapping,
    target: &str,
    value: &ManagedValue,
) -> Result<(), Conflict> {
    match service.get_mut(key("volumes")) {
        None | Some(Value::Null) => match value {
            ManagedValue::Missing => Ok(()),
            ManagedValue::Present(value) => {
                service.insert(key("volumes"), Value::Sequence(vec![value.clone()]));
                Ok(())
            }
        },
        Some(Value::Sequence(mounts)) => {
            if let Some(index) = mounts
                .iter()
                .position(|current| mount_has_target(current, target))
            {
                match value {
                    ManagedValue::Missing => {
                        mounts.remove(index);
                    }
                    ManagedValue::Present(value) => mounts[index] = value.clone(),
                }
            } else if let ManagedValue::Present(value) = value {
                mounts.push(value.clone());
            }
            Ok(())
        }
        Some(current) => Err(Conflict::new(
            service_name,
            "volumes",
            ManagedValue::Present(string("sequence")),
            ManagedValue::Present(current.clone()),
        )),
    }
}

fn mount_has_target(value: &Value, target: &str) -> bool {
    match value {
        Value::String(short) => {
            if short == target {
                return true;
            }
            let needle = format!(":{target}");
            short.rfind(&needle).is_some_and(|position| {
                let remainder = &short[position + needle.len()..];
                remainder.is_empty() || remainder.starts_with(':')
            })
        }
        Value::Mapping(long) => long
            .get(key("target"))
            .and_then(Value::as_str)
            .is_some_and(|current| current == target),
        _ => false,
    }
}

fn validate_desired_mounts(service: &str, mounts: &[ManagedMount]) -> Result<(), Conflict> {
    let mut targets = BTreeSet::new();
    for mount in mounts {
        if mount.target.is_empty() || !targets.insert(mount.target.as_str()) {
            return Err(structural_conflict(
                service,
                format!("volumes.{}", mount.target),
                "managed mount targets must be non-empty and unique",
            ));
        }
        if !mount_has_target(&mount.value, &mount.target) {
            return Err(Conflict::new(
                service,
                format!("volumes.{}", mount.target),
                ManagedValue::Present(string("mount with matching target")),
                ManagedValue::Present(mount.value.clone()),
            ));
        }
    }
    Ok(())
}

fn restore_empty_container(service: &mut Mapping, field_name: &str, original: &ManagedValue) {
    let empty = service
        .get(key(field_name))
        .is_some_and(|value| match value {
            Value::Mapping(values) => values.is_empty(),
            Value::Sequence(values) => values.is_empty(),
            _ => false,
        });
    if empty
        && matches!(
            original,
            ManagedValue::Missing | ManagedValue::Present(Value::Null)
        )
    {
        restore_field(service, field_name, original);
    }
}

fn restore_field(mapping: &mut Mapping, name: &str, value: &ManagedValue) {
    match value {
        ManagedValue::Missing => {
            mapping.remove(key(name));
        }
        ManagedValue::Present(value) => {
            mapping.insert(key(name), value.clone());
        }
    }
}

fn field(mapping: &Mapping, name: &str) -> ManagedValue {
    mapping
        .get(key(name))
        .cloned()
        .map(ManagedValue::Present)
        .unwrap_or(ManagedValue::Missing)
}

fn structural_conflict(
    service: impl Into<String>,
    field: impl Into<String>,
    current: impl Into<String>,
) -> Conflict {
    Conflict::new(
        service,
        field,
        ManagedValue::Present(string("valid managed value")),
        ManagedValue::Present(string(&current.into())),
    )
}

fn valid_project_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'0'..=b'9'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

fn render_managed_value(value: &ManagedValue) -> String {
    match value {
        ManagedValue::Missing => "<missing>".into(),
        ManagedValue::Present(value) => serde_yaml::to_string(value)
            .map(|text| text.trim().replace('\n', " "))
            .unwrap_or_else(|_| "<unrenderable>".into()),
    }
}

fn key(value: &str) -> Value {
    string(value)
}

fn string(value: &str) -> Value {
    Value::String(value.into())
}
