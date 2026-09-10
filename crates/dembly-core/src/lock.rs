use crate::CoreError;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckLock {
    pub schema_version: u32,
    pub base: LockBase,
    pub cards: Vec<LockedCard>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LockBase {
    Image {
        reference: String,
        resolved_image_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockedCard {
    pub name: String,
    pub version: String,
    pub source: String,
    pub manifest_sha256: String,
    pub filesystem_sha256: String,
}

pub fn write_lock(path: &Path, lock: &DeckLock) -> Result<(), CoreError> {
    let LockBase::Image {
        reference,
        resolved_image_id,
    } = &lock.base;
    let mut document = format!("schema_version = {}\n\n[base]\nkind = \"image\"\nreference = \"{reference}\"\nresolved_image_id = \"{resolved_image_id}\"\n", lock.schema_version);
    for card in &lock.cards {
        document.push_str(&format!("\n[[cards]]\nname = \"{}\"\nversion = \"{}\"\nsource = \"{}\"\nmanifest_sha256 = \"{}\"\nfilesystem_sha256 = \"{}\"\n", card.name, card.version, card.source, card.manifest_sha256, card.filesystem_sha256));
    }
    fs::write(path, document)
        .map_err(|error| CoreError::parse(path, format!("cannot write lock: {error}")))
}

pub fn read_lock(path: &Path) -> Result<DeckLock, CoreError> {
    let content = fs::read_to_string(path).map_err(|source| CoreError::Io {
        path: path.into(),
        source,
    })?;
    let mut root = BTreeMap::new();
    let mut base = BTreeMap::new();
    let mut cards = Vec::new();
    let mut current_card: Option<BTreeMap<String, String>> = None;
    let mut section = "root";
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[base]" {
            section = "base";
            continue;
        }
        if line == "[[cards]]" {
            if let Some(card) = current_card.take() {
                cards.push(card);
            }
            current_card = Some(BTreeMap::new());
            section = "cards";
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(CoreError::parse(path, "lock contains a non key/value line"));
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value)
            .to_owned();
        match section {
            "root" => {
                root.insert(key.trim().into(), value);
            }
            "base" => {
                base.insert(key.trim().into(), value);
            }
            "cards" => {
                current_card
                    .as_mut()
                    .unwrap()
                    .insert(key.trim().into(), value);
            }
            _ => unreachable!(),
        }
    }
    if let Some(card) = current_card {
        cards.push(card);
    }
    if base.get("kind").map(String::as_str) != Some("image") {
        return Err(CoreError::parse(
            path,
            "only image lock bases are supported",
        ));
    }
    let cards = cards
        .into_iter()
        .map(|card| {
            Ok(LockedCard {
                name: required(path, &card, "name")?,
                version: required(path, &card, "version")?,
                source: required(path, &card, "source")?,
                manifest_sha256: required(path, &card, "manifest_sha256")?,
                filesystem_sha256: required(path, &card, "filesystem_sha256")?,
            })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    Ok(DeckLock {
        schema_version: required(path, &root, "schema_version")?
            .parse()
            .map_err(|_| CoreError::parse(path, "lock schema_version must be an integer"))?,
        base: LockBase::Image {
            reference: required(path, &base, "reference")?,
            resolved_image_id: required(path, &base, "resolved_image_id")?,
        },
        cards,
    })
}

fn required(
    path: &Path,
    fields: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, CoreError> {
    fields
        .get(key)
        .cloned()
        .ok_or_else(|| CoreError::parse(path, format!("lock is missing {key}")))
}
