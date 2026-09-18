use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn resolve_static_service_image(files: &[PathBuf], service: &str) -> Result<String, String> {
    let first = files
        .first()
        .ok_or_else(|| "Compose file list must not be empty".to_owned())?;
    let project_directory = first.parent().unwrap_or_else(|| Path::new(""));
    let environment = interpolation_environment(project_directory)?;
    let resolver = StaticComposeResolver { environment };
    let mut merged = Mapping::new();
    for file in files {
        if let Some(service_mapping) = resolver.resolve_service(file, service, &mut Vec::new())? {
            merge_mapping(&mut merged, service_mapping);
        }
    }
    match merged.get(key("image")) {
        Some(Value::String(image)) => Ok(image.clone()),
        Some(_) => Err(format!(
            "Compose service {service} image must resolve to a string"
        )),
        None => Err(format!(
            "Compose service {service} has no statically declared image"
        )),
    }
}

struct StaticComposeResolver {
    environment: BTreeMap<String, String>,
}

impl StaticComposeResolver {
    fn resolve_service(
        &self,
        file: &Path,
        service: &str,
        stack: &mut Vec<(PathBuf, String)>,
    ) -> Result<Option<Mapping>, String> {
        let file = normalize_path(file);
        let identity = (file.clone(), service.to_owned());
        if stack.contains(&identity) {
            let chain = stack
                .iter()
                .chain(std::iter::once(&identity))
                .map(|(path, service)| format!("{}:{service}", path.display()))
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(format!("Compose service extends cycle: {chain}"));
        }

        let mut document = load_compose_value(&file)?;
        interpolate_value(&mut document, &self.environment)?;
        let Some(mut mapping) = service_mapping(&document, service, &file)?.cloned() else {
            return Ok(None);
        };
        let extends = mapping.remove(key("extends"));
        let Some(extends) = extends else {
            return Ok(Some(mapping));
        };

        let reference = parse_extends(&extends, &file, service)?;
        stack.push(identity);
        let base = self
            .resolve_service(&reference.file, &reference.service, stack)?
            .ok_or_else(|| {
                format!(
                    "Compose service {} extended by {service} is missing from {}",
                    reference.service,
                    reference.file.display()
                )
            });
        stack.pop();
        let mut base = base?;
        merge_mapping(&mut base, mapping);
        Ok(Some(base))
    }
}

struct ExtendsReference {
    file: PathBuf,
    service: String,
}

fn parse_extends(
    value: &Value,
    current_file: &Path,
    service: &str,
) -> Result<ExtendsReference, String> {
    let (file, referenced_service) = match value {
        Value::String(referenced_service) => (None, referenced_service.as_str()),
        Value::Mapping(mapping) => {
            let referenced_service = mapping
                .get(key("service"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "Compose service {service} extends.service in {} must be a string",
                        current_file.display()
                    )
                })?;
            let file = match mapping.get(key("file")) {
                Some(Value::String(file)) => Some(file.as_str()),
                Some(_) => {
                    return Err(format!(
                        "Compose service {service} extends.file in {} must be a string",
                        current_file.display()
                    ));
                }
                None => None,
            };
            (file, referenced_service)
        }
        _ => {
            return Err(format!(
                "Compose service {service} extends in {} must be a string or mapping",
                current_file.display()
            ));
        }
    };
    let file = match file {
        Some(file) if Path::new(file).is_absolute() => PathBuf::from(file),
        Some(file) => current_file
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(file),
        None => current_file.to_path_buf(),
    };
    Ok(ExtendsReference {
        file: normalize_path(&file),
        service: referenced_service.to_owned(),
    })
}

fn load_compose_value(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read Compose file {}: {error}", path.display()))?;
    let mut document: Value = serde_yaml::from_slice(&bytes)
        .map_err(|error| format!("cannot parse Compose file {}: {error}", path.display()))?;
    document.apply_merge().map_err(|error| {
        format!(
            "cannot apply YAML merge in Compose file {}: {error}",
            path.display()
        )
    })?;
    Ok(document)
}

fn service_mapping<'a>(
    document: &'a Value,
    service: &str,
    path: &Path,
) -> Result<Option<&'a Mapping>, String> {
    let Some(services) = document
        .as_mapping()
        .and_then(|root| root.get(key("services")))
    else {
        return Ok(None);
    };
    let services = services
        .as_mapping()
        .ok_or_else(|| format!("Compose services in {} must be a mapping", path.display()))?;
    let Some(service_value) = services.get(key(service)) else {
        return Ok(None);
    };
    service_value.as_mapping().map(Some).ok_or_else(|| {
        format!(
            "Compose service {service} in {} must be a mapping",
            path.display()
        )
    })
}

fn merge_mapping(base: &mut Mapping, override_mapping: Mapping) {
    for (key, override_value) in override_mapping {
        match (base.get_mut(&key), override_value) {
            (Some(Value::Mapping(base_mapping)), Value::Mapping(override_mapping)) => {
                merge_mapping(base_mapping, override_mapping);
            }
            (_, override_value) => {
                base.insert(key, override_value);
            }
        }
    }
}

fn interpolation_environment(project_directory: &Path) -> Result<BTreeMap<String, String>, String> {
    let process_environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let process_keys = process_environment.keys().cloned().collect::<BTreeSet<_>>();
    let mut environment = process_environment;
    let path = project_directory.join(".env");
    match fs::read_to_string(&path) {
        Ok(source) => parse_dotenv(&source, &path, &process_keys, &mut environment)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "cannot read Compose .env {}: {error}",
                path.display()
            ))
        }
    }
    Ok(environment)
}

fn parse_dotenv(
    source: &str,
    path: &Path,
    process_keys: &BTreeSet<String>,
    environment: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    for (index, raw_line) in source.lines().enumerate() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim_start();
        }
        let separator = dotenv_separator(line).ok_or_else(|| {
            format!(
                "invalid Compose .env assignment {}:{}",
                path.display(),
                index + 1
            )
        })?;
        let name = line[..separator].trim();
        if !valid_variable_name(name) {
            return Err(format!(
                "invalid Compose .env variable {name:?} at {}:{}",
                path.display(),
                index + 1
            ));
        }
        let value =
            parse_dotenv_value(line[separator + 1..].trim(), environment).map_err(|error| {
                format!(
                    "invalid Compose .env {}:{}: {error}",
                    path.display(),
                    index + 1
                )
            })?;
        if !process_keys.contains(name) {
            environment.insert(name.to_owned(), value);
        }
    }
    Ok(())
}

fn dotenv_separator(line: &str) -> Option<usize> {
    line.find('=').or_else(|| {
        line.char_indices().find_map(|(index, character)| {
            if character == ':'
                && line[index + character.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
            {
                Some(index)
            } else {
                None
            }
        })
    })
}

fn parse_dotenv_value(raw: &str, environment: &BTreeMap<String, String>) -> Result<String, String> {
    let raw = strip_inline_comment(raw).trim();
    if raw.starts_with('\'') {
        if raw.len() < 2 || !raw.ends_with('\'') {
            return Err("unterminated single-quoted value".into());
        }
        return Ok(raw[1..raw.len() - 1].replace("\\'", "'"));
    }
    if raw.starts_with('"') {
        if raw.len() < 2 || !raw.ends_with('"') {
            return Err("unterminated double-quoted value".into());
        }
        let unescaped = unescape_double_quoted(&raw[1..raw.len() - 1]);
        return interpolate(&unescaped, environment);
    }
    interpolate(raw, environment)
}

fn strip_inline_comment(value: &str) -> &str {
    value.find(" #").map_or(value, |comment| &value[..comment])
}

fn unescape_double_quoted(value: &str) -> String {
    let mut result = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('\\') => result.push('\\'),
            Some('"') => result.push('"'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

fn interpolate_value(
    value: &mut Value,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    match value {
        Value::String(string) => *string = interpolate(string, environment)?,
        Value::Sequence(sequence) => {
            for value in sequence {
                interpolate_value(value, environment)?;
            }
        }
        Value::Mapping(mapping) => {
            for value in mapping.values_mut() {
                interpolate_value(value, environment)?;
            }
        }
        Value::Tagged(tagged) => interpolate_value(&mut tagged.value, environment)?,
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn interpolate(input: &str, environment: &BTreeMap<String, String>) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut rendered = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let character = input[index..]
                .chars()
                .next()
                .expect("valid string boundary");
            rendered.push(character);
            index += character.len_utf8();
            continue;
        }
        if bytes.get(index + 1) == Some(&b'$') {
            rendered.push('$');
            index += 2;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'{') {
            let end = closing_brace(bytes, index + 2)
                .ok_or_else(|| format!("unterminated Compose interpolation in {input:?}"))?;
            rendered.push_str(&evaluate_expression(&input[index + 2..end], environment)?);
            index = end + 1;
            continue;
        }
        let name_start = index + 1;
        if bytes
            .get(name_start)
            .is_some_and(|byte| variable_start(*byte))
        {
            let mut end = name_start + 1;
            while bytes.get(end).is_some_and(|byte| variable_continue(*byte)) {
                end += 1;
            }
            rendered.push_str(
                environment
                    .get(&input[name_start..end])
                    .map_or("", String::as_str),
            );
            index = end;
            continue;
        }
        rendered.push('$');
        index += 1;
    }
    Ok(rendered)
}

fn closing_brace(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut depth = 1;
    while index < bytes.len() {
        if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
            depth += 1;
            index += 2;
        } else if bytes[index] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    None
}

fn evaluate_expression(
    expression: &str,
    environment: &BTreeMap<String, String>,
) -> Result<String, String> {
    let bytes = expression.as_bytes();
    if !bytes.first().is_some_and(|byte| variable_start(*byte)) {
        return Err(format!("invalid Compose interpolation ${{{expression}}}"));
    }
    let mut name_end = 1;
    while bytes
        .get(name_end)
        .is_some_and(|byte| variable_continue(*byte))
    {
        name_end += 1;
    }
    let name = &expression[..name_end];
    let value = environment.get(name);
    let suffix = &expression[name_end..];
    let (operator, operand) = [":-", ":?", ":+", "-", "?", "+"]
        .into_iter()
        .find_map(|operator| {
            suffix
                .strip_prefix(operator)
                .map(|operand| (operator, operand))
        })
        .unwrap_or(("", suffix));
    if operator.is_empty() && !operand.is_empty() {
        return Err(format!(
            "unsupported Compose interpolation ${{{expression}}}"
        ));
    }
    let is_set = value.is_some();
    let is_nonempty = value.is_some_and(|value| !value.is_empty());
    match operator {
        "" => Ok(value.cloned().unwrap_or_default()),
        ":-" if !is_nonempty => interpolate(operand, environment),
        "-" if !is_set => interpolate(operand, environment),
        ":?" if !is_nonempty => Err(required_variable_error(name, operand)),
        "?" if !is_set => Err(required_variable_error(name, operand)),
        ":+" if is_nonempty => interpolate(operand, environment),
        "+" if is_set => interpolate(operand, environment),
        ":+" | "+" => Ok(String::new()),
        ":-" | "-" | ":?" | "?" => Ok(value.cloned().unwrap_or_default()),
        _ => unreachable!(),
    }
}

fn required_variable_error(name: &str, message: &str) -> String {
    if message.is_empty() {
        format!("required Compose variable {name} is not set")
    } else {
        format!("required Compose variable {name} is not set: {message}")
    }
}

fn valid_variable_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(|byte| variable_start(*byte))
        && bytes[1..].iter().all(|byte| variable_continue(*byte))
}

fn variable_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn variable_continue(byte: u8) -> bool {
    variable_start(byte) || byte.is_ascii_digit()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push(component);
                }
            }
            Component::Normal(value) => normalized.push(value),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn key(value: &str) -> Value {
    Value::String(value.to_owned())
}
