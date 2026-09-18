use crate::checksum::sha256_bytes;
use serde::{Deserialize, Serialize};

const DIGEST_DOMAIN: &[u8] = b"dembly-lock-v1\0";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardIdentity {
    pub name: String,
    pub version: String,
    pub manifest_sha256: String,
    pub filesystem_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockInput {
    pub compose_path: String,
    pub service: String,
    pub image: String,
    pub cards: Vec<CardIdentity>,
}

impl LockInput {
    pub fn digest(&self) -> String {
        let mut content = DIGEST_DOMAIN.to_vec();
        for value in [&self.compose_path, &self.service, &self.image] {
            append_field(&mut content, value);
        }
        for card in &self.cards {
            for value in [
                &card.name,
                &card.version,
                &card.manifest_sha256,
                &card.filesystem_sha256,
            ] {
                append_field(&mut content, value);
            }
        }
        format!("sha256:{}", sha256_bytes(&content))
    }
}

fn append_field(content: &mut Vec<u8>, value: &str) {
    content.extend_from_slice(value.len().to_string().as_bytes());
    content.push(b':');
    content.extend_from_slice(value.as_bytes());
    content.push(0);
}
