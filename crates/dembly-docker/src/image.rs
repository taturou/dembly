use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardFileBind {
    pub name: String,
    pub source: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageRuntimePlan {
    pub image: String,
    pub container_name: String,
    pub executable: PathBuf,
    pub runtime_config: PathBuf,
    pub cards: Vec<CardFileBind>,
    pub labels: Vec<(String, String)>,
    pub extra_mounts: Vec<(PathBuf, String, bool)>,
}

pub fn image_create_command(plan: &ImageRuntimePlan) -> Vec<OsString> {
    let mut command = vec!["docker".into(), "create".into(), "--name".into(), plan.container_name.clone().into(), "--privileged".into()];
    for (key, value) in &plan.labels {
        command.extend(["--label".into(), format!("{key}={value}").into()]);
    }
    add_bind(&mut command, &plan.executable, "/run/dembly/bin/dembly", true);
    add_bind(&mut command, &plan.runtime_config, "/run/dembly/runtime.toml", true);
    for card in &plan.cards {
        add_bind(&mut command, &card.source, &format!("/run/dembly/cards/{}.squashfs", card.name), true);
    }
    for (source, target, read_only) in &plan.extra_mounts {
        add_bind(&mut command, source, target, *read_only);
    }
    command.extend([
        "--user".into(), "0:0".into(), "--entrypoint".into(), "/run/dembly/bin/dembly".into(),
        plan.image.clone().into(), "__runtime".into(), "init".into(), "/run/dembly/runtime.toml".into(),
    ]);
    command
}

fn add_bind(command: &mut Vec<OsString>, source: &std::path::Path, target: &str, read_only: bool) {
    let mode = if read_only { "ro" } else { "rw" };
    command.extend(["--volume".into(), format!("{}:{target}:{mode}", source.display()).into()]);
}
