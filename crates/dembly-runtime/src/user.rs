use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeUserSpec {
    pub spec: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRuntimeUser {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
}

pub fn resolve_current_runtime_user(spec: &str) -> Result<ResolvedRuntimeUser, String> {
    let passwd = fs::read_to_string("/etc/passwd")
        .map_err(|error| format!("cannot read /etc/passwd: {error}"))?;
    let group = fs::read_to_string("/etc/group")
        .map_err(|error| format!("cannot read /etc/group: {error}"))?;
    resolve_runtime_user(spec, &passwd, &group)
}

pub fn resolve_runtime_user(
    spec: &str,
    passwd: &str,
    group: &str,
) -> Result<ResolvedRuntimeUser, String> {
    let mut components = spec.split(':');
    let user_spec = components.next().unwrap_or_default();
    let group_spec = components.next();
    if user_spec.is_empty() || group_spec == Some("") || components.next().is_some() {
        return Err(format!("invalid runtime user spec: {spec}"));
    }

    let numeric_uid = parse_numeric_identifier(user_spec, "UID")?;
    let numeric_gid = group_spec
        .map(|value| parse_numeric_identifier(value, "GID"))
        .transpose()?
        .flatten();
    if group_spec.is_some() && numeric_uid.is_some() != numeric_gid.is_some() {
        return Err(format!("invalid runtime user spec: {spec}"));
    }

    let passwd_entry = passwd
        .lines()
        .find_map(|line| parse_passwd_line(line, user_spec, numeric_uid))
        .transpose()?
        .ok_or_else(|| format!("runtime user {user_spec} has no matching /etc/passwd entry"))?;

    let gid = match group_spec {
        None => passwd_entry.gid,
        Some(value) => match numeric_gid {
            Some(gid) => gid,
            None => resolve_named_group(value, group)?,
        },
    };

    Ok(ResolvedRuntimeUser {
        name: passwd_entry.name,
        uid: passwd_entry.uid,
        gid,
        home: passwd_entry.home,
    })
}

struct PasswdEntry {
    name: String,
    uid: u32,
    gid: u32,
    home: String,
}

fn parse_passwd_line(
    line: &str,
    requested_name: &str,
    requested_uid: Option<u32>,
) -> Option<Result<PasswdEntry, String>> {
    let fields = line.split(':').collect::<Vec<_>>();
    if fields.len() != 7 {
        return None;
    }
    let matches = match requested_uid {
        Some(uid) => fields[2].parse::<u32>().ok() == Some(uid),
        None => fields[0] == requested_name,
    };
    if !matches {
        return None;
    }
    Some((|| {
        Ok(PasswdEntry {
            name: fields[0].into(),
            uid: parse_u32(fields[2], "UID in /etc/passwd")?,
            gid: parse_u32(fields[3], "GID in /etc/passwd")?,
            home: fields[5].into(),
        })
    })())
}

fn resolve_named_group(name: &str, group: &str) -> Result<u32, String> {
    for line in group.lines() {
        let fields = line.split(':').collect::<Vec<_>>();
        if fields.len() == 4 && fields[0] == name {
            return parse_u32(fields[2], "GID in /etc/group");
        }
    }
    Err(format!(
        "runtime group {name} has no matching /etc/group entry"
    ))
}

fn parse_numeric_identifier(value: &str, label: &str) -> Result<Option<u32>, String> {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    parse_u32(value, label).map(Some)
}

fn parse_u32(value: &str, label: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("{label} must be a value within u32: {value}"))
}
