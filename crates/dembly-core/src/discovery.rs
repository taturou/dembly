use crate::{load_card, CoreError};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitCandidates {
    pub cards: Vec<CardCandidate>,
    pub devcontainers: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardCandidate {
    pub path: PathBuf,
    pub name: String,
}

pub fn discover_config(
    explicit: Option<&Path>,
    current_directory: &Path,
) -> Result<PathBuf, CoreError> {
    let candidate = explicit
        .map(|path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                current_directory.join(path)
            }
        })
        .unwrap_or_else(|| current_directory.join(".dembly/config.toml"));
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(CoreError::ConfigNotFound(candidate))
    }
}

pub fn discover_init_candidates(cwd: &Path) -> Result<InitCandidates, CoreError> {
    let mut cards = Vec::new();
    for root in [".dembly/cards", ".dembly-cards", ".cards", "cards"] {
        let root_path = cwd.join(root);
        let Some(entries) = direct_directory_entries(&root_path)? else {
            continue;
        };
        for entry in entries {
            let child = entry.map_err(|source| CoreError::Io {
                path: root_path.clone(),
                source,
            })?;
            let child_path = child.path();
            if !is_regular_directory(&child_path)? {
                continue;
            }
            let card_path = child_path.join("card.toml");
            if !is_regular_file(&card_path)? {
                continue;
            }
            let document = load_card(&card_path)?;
            cards.push(CardCandidate {
                path: relative_display_path(cwd, &card_path),
                name: document.name,
            });
        }
    }

    let mut devcontainers = Vec::new();
    let direct = cwd.join(".devcontainer.json");
    if is_regular_file(&direct)? {
        devcontainers.push(relative_display_path(cwd, &direct));
    }

    let directory = cwd.join(".devcontainer");
    if let Some(entries) = direct_directory_entries(&directory)? {
        let direct = directory.join("devcontainer.json");
        if is_regular_file(&direct)? {
            devcontainers.push(relative_display_path(cwd, &direct));
        }
        for entry in entries {
            let child = entry.map_err(|source| CoreError::Io {
                path: directory.clone(),
                source,
            })?;
            let child_path = child.path();
            if !is_regular_directory(&child_path)? {
                continue;
            }
            let candidate = child_path.join("devcontainer.json");
            if is_regular_file(&candidate)? {
                devcontainers.push(relative_display_path(cwd, &candidate));
            }
        }
    }

    cards.sort_by(|left, right| display_sort_key(&left.path).cmp(&display_sort_key(&right.path)));
    devcontainers.sort_by_key(display_sort_key);
    Ok(InitCandidates {
        cards,
        devcontainers,
    })
}

fn direct_directory_entries(path: &Path) -> Result<Option<fs::ReadDir>, CoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            fs::read_dir(path)
                .map(Some)
                .map_err(|source| CoreError::Io {
                    path: path.into(),
                    source,
                })
        }
        Ok(_) => Ok(None),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CoreError::Io {
            path: path.into(),
            source,
        }),
    }
}

fn is_regular_directory(path: &Path) -> Result<bool, CoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_dir()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(CoreError::Io {
            path: path.into(),
            source,
        }),
    }
}

fn is_regular_file(path: &Path) -> Result<bool, CoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(CoreError::Io {
            path: path.into(),
            source,
        }),
    }
}

fn relative_display_path(cwd: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(cwd).unwrap_or(path).to_path_buf()
}

fn display_sort_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
