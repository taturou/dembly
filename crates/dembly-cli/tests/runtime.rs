use dembly_cli::runtime::{run_runtime_command, RuntimeSystem};
use dembly_runtime::{
    ResolvedRuntimeUser, RuntimeBind, RuntimeCard, RuntimeCheck, RuntimeConfig, RuntimeExport,
    RuntimeHook,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const PASSWD: &str = "root:x:0:0:root:/root:/bin/sh\nvscode:x:1000:1000::/home/vscode:/bin/sh\n";
const GROUP: &str = "root:x:0:\nvscode:x:1000:\n";

#[test]
fn init_runs_root_setup_in_strict_order_before_intended_user_exec() {
    let plan = fixture("ordered-init", &complete_config());
    let mut system = RecordingSystem::root();

    run_runtime_command(&strings(["init", plan.to_str().unwrap()]), &mut system).unwrap();

    assert_eq!(
        system.events,
        [
            "effective_uid",
            "read_passwd",
            "read_group",
            "mount_card:alpha:/run/dembly/cards/alpha.squashfs:/opt/cards/alpha",
            "mount_card:beta:/run/dembly/cards/beta.squashfs:/opt/cards/beta",
            "ensure_card_mounts:alpha,beta",
            "mount_bind:/run/dembly/binds/0:${HOME}/.gitconfig:ro:vscode",
            "create_export:/opt/cards/alpha/bin/tool:/usr/local/bin/tool",
            "set_environment:MODE=development",
            "run_hook:alpha:/opt/cards/alpha/setup.sh",
            "set_gid:1000",
            "set_uid:1000",
            "exec:/usr/local/bin/start|--watch",
        ]
    );
}

#[test]
fn non_root_init_stops_before_plan_or_account_reads_and_mounts() {
    let missing_plan = PathBuf::from("/definitely/missing/runtime.toml");
    let mut system = RecordingSystem::non_root();

    let error = run_runtime_command(
        &strings(["init", missing_plan.to_str().unwrap()]),
        &mut system,
    )
    .unwrap_err();

    assert!(error.contains("root"), "{error}");
    assert_eq!(system.events, ["effective_uid"]);
}

#[test]
fn invalid_plan_or_user_stops_before_card_mount() {
    let invalid_schema = complete_config().replacen("schema_version = 1", "schema_version = 2", 1);
    let invalid_lock = complete_config().replacen("sha256:lock", "", 1);
    let unknown_user = complete_config().replacen("spec = \"vscode\"", "spec = \"missing\"", 1);

    for (name, contents, expected_last_event) in [
        ("schema", invalid_schema, "effective_uid"),
        ("lock", invalid_lock, "effective_uid"),
        ("user", unknown_user, "read_group"),
    ] {
        let plan = fixture(name, &contents);
        let mut system = RecordingSystem::root();

        let error = run_runtime_command(&strings(["init", plan.to_str().unwrap()]), &mut system)
            .unwrap_err();

        assert!(
            !system
                .events
                .iter()
                .any(|event| event.starts_with("mount_")),
            "{name}: {error}"
        );
        assert_eq!(
            system.events.last().map(String::as_str),
            Some(expected_last_event)
        );
    }
}

#[test]
fn setup_failure_names_the_operation_and_target_and_prevents_drop_and_exec() {
    let plan = fixture("setup-errors", &complete_config());
    for (failed_event, required_fragments) in [
        (
            "mount_card:alpha",
            vec![
                "mount Card",
                "alpha",
                "/run/dembly/cards/alpha.squashfs",
                "/opt/cards/alpha",
            ],
        ),
        (
            "mount_bind:/run/dembly/binds/0",
            vec![
                "mount Host Bind",
                "/run/dembly/binds/0",
                "${HOME}/.gitconfig",
            ],
        ),
        (
            "create_export:/opt/cards/alpha/bin/tool",
            vec![
                "create export",
                "/opt/cards/alpha/bin/tool",
                "/usr/local/bin/tool",
            ],
        ),
        (
            "run_hook:alpha",
            vec!["post_mount hook", "alpha", "/opt/cards/alpha/setup.sh"],
        ),
    ] {
        let mut system = RecordingSystem::root().failing(failed_event);

        let error = run_runtime_command(&strings(["init", plan.to_str().unwrap()]), &mut system)
            .unwrap_err();

        for fragment in required_fragments {
            assert!(
                error.contains(fragment),
                "missing {fragment:?} in {error:?}"
            );
        }
        assert!(error.contains("synthetic failure"), "{error}");
        assert!(!system
            .events
            .iter()
            .any(|event| event.starts_with("set_gid")));
        assert!(!system
            .events
            .iter()
            .any(|event| event.starts_with("set_uid")));
        assert!(!system.events.iter().any(|event| event.starts_with("exec:")));
    }
}

#[test]
fn environment_failure_prevents_root_hook_privilege_drop_and_exec() {
    let plan = fixture("environment-error", &complete_config());
    let mut system = RecordingSystem::root().failing("set_environment:");

    let error =
        run_runtime_command(&strings(["init", plan.to_str().unwrap()]), &mut system).unwrap_err();

    assert!(error.contains("set runtime environment"), "{error}");
    for forbidden in ["run_hook:", "set_gid:", "set_uid:", "exec:"] {
        assert!(
            !system
                .events
                .iter()
                .any(|event| event.starts_with(forbidden)),
            "unexpected {forbidden} after environment failure: {:?}",
            system.events
        );
    }
}

#[test]
fn init_uses_saved_argv_by_default_and_replaces_it_with_compose_run_argv() {
    let plan = fixture("argv", &complete_config());
    let mut default_system = RecordingSystem::root();
    run_runtime_command(
        &strings(["init", plan.to_str().unwrap()]),
        &mut default_system,
    )
    .unwrap();
    assert_eq!(
        default_system.events.last().unwrap(),
        "exec:/usr/local/bin/start|--watch"
    );

    let mut override_system = RecordingSystem::root();
    run_runtime_command(
        &strings(["init", plan.to_str().unwrap(), "/bin/echo", "compose-run"]),
        &mut override_system,
    )
    .unwrap();
    assert_eq!(
        override_system.events.last().unwrap(),
        "exec:/bin/echo|compose-run"
    );
}

#[test]
fn native_compose_check_is_outer_exec_then_runs_checks_without_mounting_again() {
    let plan = fixture("native-check", &complete_config());
    let plan_text = plan.to_string_lossy().into_owned();
    let check_argv = [
        "init",
        plan_text.as_str(),
        "/run/dembly/bin/dembly",
        "__runtime",
        "check",
        "/run/dembly/runtime/dev.toml",
    ];
    let mut system = RecordingSystem::root();

    run_runtime_command(&strings(check_argv), &mut system).unwrap();

    assert_eq!(
        system.events.last().unwrap(),
        "exec:/run/dembly/bin/dembly|__runtime|check|/run/dembly/runtime/dev.toml"
    );

    system.events.clear();
    system.effective_uid = 1000;
    run_runtime_command(&strings(["check", plan_text.as_str()]), &mut system).unwrap();

    assert_eq!(
        system.events,
        [
            "read_passwd",
            "read_group",
            "run_check:alpha:/opt/cards/alpha/check",
            "run_check:beta:/opt/cards/beta/check",
        ]
    );
}

#[test]
fn check_failure_names_card_and_path_and_stops_later_checks() {
    let plan = fixture("check-error", &complete_config());
    let mut system = RecordingSystem::non_root().failing("run_check:alpha");

    let error =
        run_runtime_command(&strings(["check", plan.to_str().unwrap()]), &mut system).unwrap_err();

    assert!(error.contains("run check"), "{error}");
    assert!(error.contains("alpha"), "{error}");
    assert!(error.contains("/opt/cards/alpha/check"), "{error}");
    assert!(!system
        .events
        .iter()
        .any(|event| event.starts_with("run_check:beta")));
    assert!(!system
        .events
        .iter()
        .any(|event| event.starts_with("mount_")));
}

struct RecordingSystem {
    effective_uid: u32,
    events: Vec<String>,
    fail_at: Option<String>,
}

impl RecordingSystem {
    fn root() -> Self {
        Self {
            effective_uid: 0,
            events: Vec::new(),
            fail_at: None,
        }
    }

    fn non_root() -> Self {
        Self {
            effective_uid: 1000,
            events: Vec::new(),
            fail_at: None,
        }
    }

    fn failing(mut self, event_prefix: &str) -> Self {
        self.fail_at = Some(event_prefix.into());
        self
    }

    fn record(&mut self, event: String) -> Result<(), String> {
        let should_fail = self
            .fail_at
            .as_ref()
            .is_some_and(|prefix| event.starts_with(prefix));
        self.events.push(event);
        if should_fail {
            Err("synthetic failure".into())
        } else {
            Ok(())
        }
    }
}

impl RuntimeSystem for RecordingSystem {
    fn effective_uid(&mut self) -> u32 {
        self.events.push("effective_uid".into());
        self.effective_uid
    }

    fn read_passwd(&mut self) -> Result<String, String> {
        self.record("read_passwd".into())?;
        Ok(PASSWD.into())
    }

    fn read_group(&mut self) -> Result<String, String> {
        self.record("read_group".into())?;
        Ok(GROUP.into())
    }

    fn mount_card(&mut self, card: &RuntimeCard) -> Result<(), String> {
        self.record(format!(
            "mount_card:{}:{}:{}",
            card.name,
            card.image.display(),
            card.mount_target.display()
        ))
    }

    fn ensure_card_mounts(&mut self, cards: &[RuntimeCard]) -> Result<(), String> {
        self.record(format!(
            "ensure_card_mounts:{}",
            cards
                .iter()
                .map(|card| card.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ))
    }

    fn mount_bind(&mut self, bind: &RuntimeBind, user: &ResolvedRuntimeUser) -> Result<(), String> {
        self.record(format!(
            "mount_bind:{}:{}:{}:{}",
            bind.source.display(),
            bind.target,
            bind.mode,
            user.name
        ))
    }

    fn create_export(&mut self, export: &RuntimeExport) -> Result<(), String> {
        self.record(format!(
            "create_export:{}:{}",
            export.source.display(),
            export.target.display()
        ))
    }

    fn run_hook(
        &mut self,
        hook: &RuntimeHook,
        _cards: &[RuntimeCard],
        _environment: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        self.record(format!("run_hook:{}:{}", hook.card, hook.exec.display()))
    }

    fn set_environment(&mut self, environment: &BTreeMap<String, String>) -> Result<(), String> {
        self.record(format!(
            "set_environment:{}",
            environment
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(",")
        ))
    }

    fn set_gid(&mut self, gid: u32) -> Result<(), String> {
        self.record(format!("set_gid:{gid}"))
    }

    fn set_uid(&mut self, uid: u32) -> Result<(), String> {
        self.record(format!("set_uid:{uid}"))
    }

    fn exec(&mut self, argv: &[String]) -> Result<(), String> {
        self.record(format!("exec:{}", argv.join("|")))
    }

    fn run_check(
        &mut self,
        check: &RuntimeCheck,
        _card: &RuntimeCard,
        _config: &RuntimeConfig,
        _user: &ResolvedRuntimeUser,
    ) -> Result<(), String> {
        self.record(format!("run_check:{}:{}", check.card, check.exec.display()))
    }
}

fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn fixture(name: &str, contents: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dembly-cli-runtime-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("runtime.toml");
    fs::write(&path, contents).unwrap();
    path
}

fn complete_config() -> String {
    r#"schema_version = 1
lock_digest = "sha256:lock"

[runtime_user]
spec = "vscode"

[[cards]]
name = "alpha"
image = "/run/dembly/cards/alpha.squashfs"
mount_target = "/opt/cards/alpha"

[[cards]]
name = "beta"
image = "/run/dembly/cards/beta.squashfs"
mount_target = "/opt/cards/beta"

[[binds]]
source = "/run/dembly/binds/0"
target = "${HOME}/.gitconfig"
mode = "ro"

[[exports]]
source = "/opt/cards/alpha/bin/tool"
target = "/usr/local/bin/tool"

[[hooks]]
card = "alpha"
exec = "/opt/cards/alpha/setup.sh"

[[checks]]
card = "beta"
exec = "/opt/cards/beta/check"

[[checks]]
card = "alpha"
exec = "/opt/cards/alpha/check"

[environment]
MODE = "development"

[process]
argv = ["/usr/local/bin/start", "--watch"]
"#
    .into()
}
