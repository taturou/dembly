use crate::args::{CliError, HostContext};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};

pub fn run(
    context: &HostContext,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), CliError> {
    refuse_existing_destination(&context.config_path)?;
    let candidates = dembly_core::discover_init_candidates(&context.cwd)
        .map_err(|error| CliError::new(format!("cannot discover init candidates: {error}")))?;
    let cards = select_cards(&candidates.cards, input, output)?;
    reject_duplicate_card_names(&cards)?;

    let config_parent = context
        .config_path
        .parent()
        .ok_or_else(|| CliError::new("config path has no parent directory"))?;
    let devcontainer = select_devcontainer(
        &candidates.devcontainers,
        context,
        config_parent,
        input,
        output,
    )?;
    let (compose_path, service, devcontainer_path) = match devcontainer {
        Some(selection) => (
            selection.compose_path,
            selection.service,
            Some(selection.path),
        ),
        None => (
            required_answer(input, output, "Compose path")?,
            required_answer(input, output, "Compose service")?,
            None,
        ),
    };
    let card_paths = cards
        .iter()
        .map(|candidate| relative_path(config_parent, &context.cwd.join(&candidate.path)))
        .collect::<Result<Vec<_>, _>>()?;
    let document = render_config(
        &compose_path,
        &service,
        devcontainer_path.as_deref(),
        &card_paths,
    )?;
    validate_config(&document)?;

    if let Some(parent) = context.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::new(format!(
                "cannot create config directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&context.config_path)
        .map_err(|error| {
            CliError::new(format!(
                "cannot create config {}: {error}",
                context.config_path.display()
            ))
        })?;
    destination
        .write_all(document.as_bytes())
        .map_err(|error| {
            CliError::new(format!(
                "cannot write config {}: {error}",
                context.config_path.display()
            ))
        })?;
    destination.sync_all().map_err(|error| {
        CliError::new(format!(
            "cannot sync config {}: {error}",
            context.config_path.display()
        ))
    })?;
    writeln!(output, "Created {}", context.config_path.display())
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    Ok(())
}

struct DevcontainerSelection {
    path: String,
    compose_path: String,
    service: String,
}

fn select_cards(
    candidates: &[dembly_core::CardCandidate],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Vec<dembly_core::CardCandidate>, CliError> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    writeln!(output, "Card candidates:")
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    for (index, candidate) in candidates.iter().enumerate() {
        writeln!(
            output,
            "  {}. {} ({})",
            index + 1,
            candidate.path.display(),
            candidate.name
        )
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    }
    let selected = selected_indices(
        &required_or_empty_answer(
            input,
            output,
            "Select Card candidates (comma-separated numbers; empty for none)",
        )?,
        candidates.len(),
        "Card candidate",
    )?;
    let cards = selected
        .into_iter()
        .map(|index| candidates[index].clone())
        .collect::<Vec<_>>();
    reject_duplicate_card_names(&cards)?;
    let mut adopted = Vec::new();
    for card in cards {
        if !confirmation(
            input,
            output,
            &format!("Adopt Card {} ({})", card.path.display(), card.name),
        )? {
            continue;
        }
        adopted.push(card);
    }
    Ok(adopted)
}

fn reject_duplicate_card_names(cards: &[dembly_core::CardCandidate]) -> Result<(), CliError> {
    let mut names = BTreeSet::new();
    for card in cards {
        if !names.insert(&card.name) {
            return Err(CliError::new(format!(
                "duplicate Card name selected: {}",
                card.name
            )));
        }
    }
    Ok(())
}

fn select_devcontainer(
    candidates: &[PathBuf],
    context: &HostContext,
    config_parent: &Path,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Option<DevcontainerSelection>, CliError> {
    if candidates.is_empty() {
        return Ok(None);
    }
    writeln!(output, "Dev Containers candidates:")
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    for (index, candidate) in candidates.iter().enumerate() {
        writeln!(output, "  {}. {}", index + 1, candidate.display())
            .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    }
    let answer = required_or_empty_answer(
        input,
        output,
        "Select Dev Containers configuration (number; empty for none)",
    )?;
    if answer.trim().is_empty() {
        return Ok(None);
    }
    let selected = selected_indices(&answer, candidates.len(), "Dev Containers candidate")?;
    if selected.len() != 1 {
        return Err(CliError::new(
            "select at most one Dev Containers configuration",
        ));
    }
    let candidate = &candidates[selected[0]];
    let absolute = context.cwd.join(candidate);
    let document = dembly_docker::load_devcontainer(&absolute)
        .map_err(|error| CliError::new(format!("cannot read {}: {error}", candidate.display())))?;
    let Some(last_compose) = document.docker_compose_file.last() else {
        return Err(CliError::new(format!(
            "{} does not list a Compose file",
            candidate.display()
        )));
    };
    writeln!(output, "Dev Containers: {}", candidate.display())
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    writeln!(output, "Service: {}", document.service)
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    writeln!(output, "Compose files:")
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    for compose in &document.docker_compose_file {
        writeln!(output, "  {compose}")
            .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    }
    if !confirmation(input, output, "Adopt Dev Containers configuration")? {
        return Ok(None);
    }
    let compose_file = absolute.parent().unwrap_or(&context.cwd).join(last_compose);
    Ok(Some(DevcontainerSelection {
        path: relative_path(config_parent, &absolute)?,
        compose_path: relative_path(config_parent, &compose_file)?,
        service: document.service,
    }))
}

fn selected_indices(value: &str, limit: usize, label: &str) -> Result<Vec<usize>, CliError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for part in value.split(',') {
        let number = part.trim().parse::<usize>().map_err(|_| {
            CliError::new(format!(
                "{label} selection must be a comma-separated list of numbers"
            ))
        })?;
        if number == 0 || number > limit {
            return Err(CliError::new(format!(
                "{label} selection is out of range: {number}"
            )));
        }
        if !seen.insert(number) {
            return Err(CliError::new(format!(
                "{label} was selected more than once: {number}"
            )));
        }
        selected.push(number - 1);
    }
    Ok(selected)
}

fn confirmation(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
) -> Result<bool, CliError> {
    let answer = required_or_empty_answer(input, output, &format!("{label}? [y/N]"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn required_answer(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
) -> Result<String, CliError> {
    let value = required_or_empty_answer(input, output, label)?;
    if value.trim().is_empty() {
        return Err(CliError::new(format!("{label} is required")));
    }
    Ok(value)
}

fn required_or_empty_answer(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    label: &str,
) -> Result<String, CliError> {
    write!(output, "{label}: ")
        .and_then(|_| output.flush())
        .map_err(|error| CliError::new(format!("cannot write init output: {error}")))?;
    let mut answer = String::new();
    let bytes = input
        .read_line(&mut answer)
        .map_err(|error| CliError::new(format!("cannot read init input: {error}")))?;
    if bytes == 0 {
        return Err(CliError::new(format!("input ended while reading {label}")));
    }
    Ok(answer.trim().to_owned())
}

fn refuse_existing_destination(path: &Path) -> Result<(), CliError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(CliError::new(format!(
            "config already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError::new(format!(
            "cannot inspect config {}: {error}",
            path.display()
        ))),
    }
}

fn render_config(
    compose_path: &str,
    service: &str,
    devcontainer_path: Option<&str>,
    card_paths: &[String],
) -> Result<String, CliError> {
    let mut result = format!(
        "schema_version = 1\n\n[compose]\npath = {}\nservice = {}\n",
        toml_string(compose_path),
        toml_string(service)
    );
    if let Some(path) = devcontainer_path {
        result.push_str(&format!("\n[devcontainer]\npath = {}\n", toml_string(path)));
    }
    for path in card_paths {
        result.push_str(&format!("\n[[cards]]\npath = {}\n", toml_string(path)));
    }
    Ok(result)
}

fn validate_config(content: &str) -> Result<(), CliError> {
    toml::from_str::<dembly_core::ConfigDocument>(content)
        .map(|_| ())
        .map_err(|error| CliError::new(format!("cannot render a valid config: {error}")))
}

fn toml_string(value: &str) -> String {
    let mut result = String::from('"');
    for character in value.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            control if control.is_control() => {
                result.push_str(&format!("\\u{:04x}", control as u32))
            }
            value => result.push(value),
        }
    }
    result.push('"');
    result
}

fn relative_path(base: &Path, target: &Path) -> Result<String, CliError> {
    let base = normalize_absolute(base)?;
    let target = normalize_absolute(target)?;
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for _ in base.iter().skip(common) {
        result.push("..");
    }
    for component in target.iter().skip(common) {
        result.push(component);
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    path_as_utf8(&result)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, CliError> {
    if !path.is_absolute() {
        return Err(CliError::new(format!(
            "path is not absolute: {}",
            path.display()
        )));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    Ok(normalized)
}

fn path_as_utf8(path: &Path) -> Result<String, CliError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CliError::new(format!("path is not UTF-8: {}", path.display())))
}
