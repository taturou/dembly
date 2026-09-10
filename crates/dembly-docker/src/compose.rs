use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeRuntimePlan {
    pub service: String,
    pub executable: PathBuf,
    pub runtime_config: PathBuf,
    pub mounts: Vec<(PathBuf, String, bool)>,
    pub labels: Vec<(String, String)>,
}

pub fn compose_override(plan: &ComposeRuntimePlan) -> String {
    let mut mounts = vec![
        format!("{}:/run/dembly/bin/dembly:ro", plan.executable.display()),
        format!(
            "{}:/run/dembly/runtime.toml:ro",
            plan.runtime_config.display()
        ),
    ];
    mounts.extend(plan.mounts.iter().map(|(source, target, read_only)| {
        let mode = if *read_only { "ro" } else { "rw" };
        format!("{}:{target}:{mode}", source.display())
    }));
    let mut document = format!(
        "services:\n  {}:\n    privileged: true\n    user: \"0:0\"\n    entrypoint: [\"/run/dembly/bin/dembly\", \"__runtime\", \"init\", \"/run/dembly/runtime.toml\"]\n    command: []\n    volumes:\n",
        yaml_string(&plan.service),
    );
    for mount in mounts {
        document.push_str(&format!("      - \"{}\"\n", yaml_string(&mount)));
    }
    document.push_str("    labels:\n");
    for (key, value) in &plan.labels {
        document.push_str(&format!(
            "      {}: \"{}\"\n",
            yaml_string(key),
            yaml_string(value)
        ));
    }
    document
}

fn yaml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
