use crate::{Base, CardDocument, CardMount, CardReference, CoreError, DeckDocument, Filesystem};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct Tables {
    root: BTreeMap<String, String>,
    tables: BTreeMap<String, Vec<BTreeMap<String, String>>>,
}

pub fn load_deck(path: &Path) -> Result<DeckDocument, CoreError> {
    let tables = read_tables(path)?;
    reject_unknown_tables(path, &tables, &["base", "cards"])?;
    reject_unknown_fields(path, "root", &tables.root, &["schema_version", "name"])?;
    let base = one_table(path, &tables, "base")?;
    reject_unknown_fields(path, "base", base, &["image", "compose", "service"])?;
    let image = base.get("image");
    let compose = base.get("compose");
    let base = match (image, compose) {
        (Some(image), None) => Base::Image { image: image.clone() },
        (None, Some(compose)) => Base::Compose {
            compose: compose.clone(),
            service: required(path, base, "service")?.to_owned(),
        },
        _ => return Err(CoreError::parse(path, "[base] requires exactly one of image or compose")),
    };
    let cards = tables.tables.get("cards").into_iter().flatten().map(|card| {
        reject_unknown_fields(path, "[[cards]]", card, &["path"])?;
        Ok(CardReference { path: required(path, card, "path")?.to_owned() })
    }).collect::<Result<Vec<_>, CoreError>>()?;

    Ok(DeckDocument {
        schema_version: parse_u32(path, required(path, &tables.root, "schema_version")?, "schema_version")?,
        name: required(path, &tables.root, "name")?.to_owned(),
        base,
        cards,
    })
}

pub fn load_card(path: &Path) -> Result<CardDocument, CoreError> {
    let tables = read_tables(path)?;
    reject_unknown_tables(path, &tables, &["filesystem", "mount"])?;
    reject_unknown_fields(path, "root", &tables.root, &["schema_version", "name", "version"])?;
    let filesystem = one_table(path, &tables, "filesystem")?;
    reject_unknown_fields(path, "filesystem", filesystem, &["type", "file", "sha256"])?;
    let mount = one_table(path, &tables, "mount")?;
    reject_unknown_fields(path, "mount", mount, &["target"])?;

    Ok(CardDocument {
        schema_version: parse_u32(path, required(path, &tables.root, "schema_version")?, "schema_version")?,
        name: required(path, &tables.root, "name")?.to_owned(),
        version: required(path, &tables.root, "version")?.to_owned(),
        filesystem: Filesystem {
            file_type: required(path, filesystem, "type")?.to_owned(),
            file: required(path, filesystem, "file")?.to_owned(),
            sha256: required(path, filesystem, "sha256")?.to_owned(),
        },
        mount: CardMount { target: required(path, mount, "target")?.to_owned() },
    })
}

fn read_tables(path: &Path) -> Result<Tables, CoreError> {
    let content = fs::read_to_string(path).map_err(|source| CoreError::Io { path: path.into(), source })?;
    let mut result = Tables::default();
    let mut table_name: Option<String> = None;
    let mut table_index = 0usize;
    for (line_number, raw_line) in content.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() { continue; }
        if line.starts_with("[[") && line.ends_with("]]" ) {
            let name = line[2..line.len() - 2].trim().to_owned();
            table_index = result.tables.entry(name.clone()).or_default().len();
            result.tables.get_mut(&name).unwrap().push(BTreeMap::new());
            table_name = Some(name);
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_owned();
            if result.tables.contains_key(&name) {
                return Err(CoreError::parse(path, format!("duplicate table [{name}]")));
            }
            result.tables.insert(name.clone(), vec![BTreeMap::new()]);
            table_name = Some(name);
            table_index = 0;
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(CoreError::parse(path, format!("line {} is not a key/value entry", line_number + 1)));
        };
        let key = key.trim();
        if key.is_empty() { return Err(CoreError::parse(path, format!("line {} has an empty key", line_number + 1))); }
        let value = parse_scalar(path, raw_value.trim(), line_number + 1)?;
        let fields = match &table_name {
            Some(name) => &mut result.tables.get_mut(name).unwrap()[table_index],
            None => &mut result.root,
        };
        if fields.insert(key.to_owned(), value).is_some() {
            return Err(CoreError::parse(path, format!("duplicate field: {key}")));
        }
    }
    Ok(result)
}

fn parse_scalar(path: &Path, value: &str, line: usize) -> Result<String, CoreError> {
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Ok(value[1..value.len() - 1].to_owned())
    } else if value == "true" || value == "false" || value.chars().all(|character| character.is_ascii_digit()) {
        Ok(value.to_owned())
    } else {
        Err(CoreError::parse(path, format!("line {line} has an unsupported TOML scalar")))
    }
}

fn one_table<'a>(path: &Path, tables: &'a Tables, name: &str) -> Result<&'a BTreeMap<String, String>, CoreError> {
    match tables.tables.get(name) {
        Some(values) if values.len() == 1 => Ok(&values[0]),
        _ => Err(CoreError::parse(path, format!("[{name}] is required exactly once"))),
    }
}

fn required<'a>(path: &Path, fields: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, CoreError> {
    fields.get(key).map(String::as_str).ok_or_else(|| CoreError::parse(path, format!("missing required field: {key}")))
}

fn parse_u32(path: &Path, value: &str, key: &str) -> Result<u32, CoreError> {
    value.parse().map_err(|_| CoreError::parse(path, format!("{key} must be an unsigned integer")))
}

fn reject_unknown_tables(path: &Path, tables: &Tables, allowed: &[&str]) -> Result<(), CoreError> {
    for name in tables.tables.keys() {
        if !allowed.contains(&name.as_str()) { return Err(CoreError::parse(path, format!("unknown table: {name}"))); }
    }
    Ok(())
}

fn reject_unknown_fields(path: &Path, table: &str, fields: &BTreeMap<String, String>, allowed: &[&str]) -> Result<(), CoreError> {
    for field in fields.keys() {
        if !allowed.contains(&field.as_str()) {
            let prefix = if table == "root" { "" } else { " in " };
            let suffix = if table == "root" { String::new() } else { table.to_owned() };
            return Err(CoreError::parse(path, format!("unknown field: {field}{prefix}{suffix}")));
        }
    }
    Ok(())
}
