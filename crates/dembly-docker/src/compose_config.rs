use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveService {
    pub image: String,
    /// `None` means Compose did not override the image value. An empty vector is explicit.
    pub entrypoint: Option<Vec<String>>,
    /// `None` means Compose did not override the image value. An empty vector is explicit.
    pub command: Option<Vec<String>>,
    /// `None` means Compose did not override the image user.
    pub user: Option<String>,
    /// A `None` value explicitly removes an environment variable inherited from the image.
    pub environment: BTreeMap<String, Option<String>>,
}

pub(crate) fn parse_compose_service_config(
    document: &str,
    service: &str,
) -> Result<EffectiveService, String> {
    let document: ComposeDocument = serde_json::from_str(document)
        .map_err(|error| format!("invalid Compose config JSON: {error}"))?;
    let selected = document
        .services
        .get(service)
        .ok_or_else(|| format!("Compose config has no service {service}"))?;
    let image = selected.image.clone().unwrap_or_default();
    if image.is_empty() {
        return Err(format!("Compose service {service} has no effective image"));
    }
    Ok(EffectiveService {
        image,
        entrypoint: selected.entrypoint.clone().map(Argv::into_vec),
        command: selected.command.clone().map(Argv::into_vec),
        user: selected.user.clone(),
        environment: selected.environment.clone(),
    })
}

#[derive(Deserialize)]
struct ComposeDocument {
    services: BTreeMap<String, ComposeService>,
}

#[derive(Deserialize)]
struct ComposeService {
    image: Option<String>,
    entrypoint: Option<Argv>,
    command: Option<Argv>,
    user: Option<String>,
    #[serde(default)]
    environment: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Argv {
    String(String),
    Array(Vec<String>),
}

impl Argv {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::String(value) => vec![value],
            Self::Array(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_compose_service_config;

    #[test]
    fn parses_service_process_overrides_without_confusing_null_and_empty() {
        let value = parse_compose_service_config(
            r#"{"services":{"dev":{"image":"dev","entrypoint":["/init"],"command":[],"user":"1000:1001"},"other":{"image":"other","entrypoint":null,"command":null}}}"#,
            "dev",
        )
        .unwrap();

        assert_eq!(value.entrypoint, Some(vec!["/init".into()]));
        assert_eq!(value.command, Some(Vec::new()));
        assert_eq!(value.user, Some("1000:1001".into()));
    }

    #[test]
    fn parses_json_unicode_surrogate_pairs_in_unrelated_config() {
        let value = parse_compose_service_config(
            r#"{"label":"\ud83d\ude80","services":{"dev":{"image":"dev","entrypoint":null,"command":["run"],"user":null}}}"#,
            "dev",
        )
        .unwrap();

        assert_eq!(value.command, Some(vec!["run".into()]));
    }
}
