use crate::commands::check::{lock_status, static_applied_state};
use crate::commands::validate::write_warnings;
use dembly_cli::{CliError, HostContext, HostPlan};
use std::fmt::Write as _;
use std::io::Write;

pub fn run(context: &HostContext, output: &mut dyn Write) -> Result<(), CliError> {
    let plan = HostPlan::resolve(&context.config_path)?;
    write_warnings(&plan);
    output
        .write_all(render(&plan).as_bytes())
        .map_err(|error| CliError::new(format!("cannot write inspection result: {error}")))
}

pub(crate) fn render(plan: &HostPlan) -> String {
    let mut rendered = String::new();
    let _ = writeln!(rendered, "config: {}", plan.deck.path.display());
    let _ = writeln!(rendered, "deck root: {}", plan.deck.root.display());
    let _ = writeln!(rendered, "compose:");
    let _ = writeln!(
        rendered,
        "  project: {}",
        plan.managed_compose.project_name().unwrap_or("invalid")
    );
    let _ = writeln!(rendered, "  path: {}", plan.deck.compose_path.display());
    let _ = writeln!(
        rendered,
        "  service: {}",
        plan.deck.document.compose.service
    );
    let _ = writeln!(rendered, "user: {}", plan.intended_user_spec());
    let _ = writeln!(rendered, "cards:");
    for card in &plan.deck.cards {
        let _ = writeln!(
            rendered,
            "  - {} {}",
            card.document.name, card.document.version
        );
        let _ = writeln!(rendered, "    manifest: {}", card.manifest_path.display());
        let filesystem = card
            .manifest_path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_default()
            .join(&card.document.filesystem.file);
        let _ = writeln!(rendered, "    filesystem: {}", filesystem.display());
        let _ = writeln!(rendered, "    mount: {}", card.document.mount.target);
    }
    let _ = writeln!(rendered, "mounts:");
    for card in &plan.deck.cards {
        let _ = writeln!(
            rendered,
            "  - card:{} -> {}",
            card.document.name, card.document.mount.target
        );
    }
    for volume in &plan.deck.volumes {
        let _ = writeln!(
            rendered,
            "  - volume:{} -> {}",
            volume.name,
            volume.target.display()
        );
    }
    for bind in &plan.deck.binds {
        let _ = writeln!(
            rendered,
            "  - bind:{} -> {} ({})",
            bind.source.display(),
            bind.target,
            bind.mode
        );
    }
    let _ = writeln!(rendered, "environment:");
    for (key, value) in &plan.environment {
        let _ = writeln!(rendered, "  {key}={value}");
    }
    let _ = writeln!(rendered, "lock: {}", lock_status(plan));
    match plan.managed_compose.state() {
        Ok(None) => {
            let _ = writeln!(rendered, "apply: not applied");
            let _ = writeln!(rendered, "conflicts: none");
        }
        Ok(Some(_)) => match static_applied_state(plan) {
            Ok(()) => {
                let _ = writeln!(rendered, "apply: applied");
                let _ = writeln!(rendered, "conflicts: none");
            }
            Err(error) => {
                let _ = writeln!(rendered, "apply: conflict");
                let _ = writeln!(rendered, "conflicts: {error}");
            }
        },
        Err(error) => {
            let _ = writeln!(rendered, "apply: invalid");
            let _ = writeln!(rendered, "conflicts: {error}");
        }
    }
    let runtime_root = plan.deck.root.join("runtime");
    let service = &plan.deck.document.compose.service;
    let _ = writeln!(rendered, "next apply:");
    let _ = writeln!(rendered, "  fields:");
    let _ = writeln!(
        rendered,
        "    entrypoint: /run/dembly/bin/dembly __runtime init /run/dembly/runtime/{service}.toml"
    );
    let _ = writeln!(rendered, "    command: []");
    let _ = writeln!(rendered, "    user: root");
    let _ = writeln!(rendered, "    privileged: true");
    let _ = writeln!(rendered, "    label: io.dembly.lock-digest");
    let _ = writeln!(rendered, "  artifacts:");
    let _ = writeln!(
        rendered,
        "    - {}/{}.toml",
        runtime_root.display(),
        service
    );
    let _ = writeln!(rendered, "    - {}/bin/dembly", runtime_root.display());
    rendered
}
