use dembly_docker::{load_devcontainer, validate_devcontainer, DevContainerCommand};
use std::fs;
use std::path::{Path, PathBuf};

fn temporary_directory(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("dembly-devcontainer-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn loads_string_and_array_compose_files_with_strict_field_types() {
    let root = temporary_directory("types");
    let string_path = root.join("string.json");
    write(
        &string_path,
        r#"{"dockerComposeFile":"compose.yaml","service":"dev","initializeCommand":["sh","init.sh"],"overrideCommand":false,"containerUser":"root","remoteUser":"vscode","updateRemoteUserUID":true}"#,
    );
    let document = load_devcontainer(&string_path).unwrap();
    assert_eq!(document.docker_compose_file, ["compose.yaml"]);
    assert_eq!(
        document.initialize_command,
        Some(DevContainerCommand::Array(vec![
            "sh".into(),
            "init.sh".into()
        ]))
    );
    assert_eq!(document.remote_user, "vscode");
    assert_eq!(document.update_remote_user_uid, Some(true));

    let array_path = root.join("array.json");
    write(
        &array_path,
        r#"{"dockerComposeFile":["base.yaml","compose.yaml"],"service":"dev","remoteUser":"vscode"}"#,
    );
    assert_eq!(
        load_devcontainer(&array_path).unwrap().docker_compose_file,
        ["base.yaml", "compose.yaml"]
    );

    for (name, content) in [
        (
            "compose-entry",
            r#"{"dockerComposeFile":["compose.yaml",1],"service":"dev","remoteUser":"vscode"}"#,
        ),
        (
            "service",
            r#"{"dockerComposeFile":"compose.yaml","service":[],"remoteUser":"vscode"}"#,
        ),
        (
            "remote-user",
            r#"{"dockerComposeFile":"compose.yaml","service":"dev","remoteUser":false}"#,
        ),
        (
            "override-command",
            r#"{"dockerComposeFile":"compose.yaml","service":"dev","remoteUser":"vscode","overrideCommand":"false"}"#,
        ),
        (
            "container-user",
            r#"{"dockerComposeFile":"compose.yaml","service":"dev","remoteUser":"vscode","containerUser":0}"#,
        ),
        (
            "update-uid",
            r#"{"dockerComposeFile":"compose.yaml","service":"dev","remoteUser":"vscode","updateRemoteUserUID":"true"}"#,
        ),
    ] {
        let malformed = root.join(format!("{name}.json"));
        write(&malformed, content);
        assert!(load_devcontainer(&malformed).is_err(), "{name}");
    }
}

#[test]
fn validates_devcontainer_against_the_managed_compose_contract() {
    let root = temporary_directory("validation");
    let devcontainer = root.join(".devcontainer/devcontainer.json");
    let base = root.join(".devcontainer/base.yaml");
    let managed = root.join("compose.yaml");
    write(&base, "services: {}\n");
    write(&managed, "services: {}\n");
    write(
        &devcontainer,
        r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"dev","overrideCommand":false,"containerUser":"root","remoteUser":"vscode"}"#,
    );
    let document = load_devcontainer(&devcontainer).unwrap();
    assert_eq!(
        validate_devcontainer(&document, &devcontainer, &managed, "dev", "vscode").unwrap(),
        [base.clone(), managed.clone()]
    );

    for (name, content) in [
        (
            "service",
            r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"other","remoteUser":"vscode"}"#,
        ),
        (
            "managed-last",
            r#"{"dockerComposeFile":["../compose.yaml","base.yaml"],"service":"dev","remoteUser":"vscode"}"#,
        ),
        (
            "override-command",
            r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"dev","overrideCommand":true,"remoteUser":"vscode"}"#,
        ),
        (
            "container-user",
            r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"dev","containerUser":"node","remoteUser":"vscode"}"#,
        ),
        (
            "remote-user",
            r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"dev","remoteUser":"other"}"#,
        ),
    ] {
        write(&devcontainer, content);
        let invalid = load_devcontainer(&devcontainer).unwrap();
        assert!(
            validate_devcontainer(&invalid, &devcontainer, &managed, "dev", "vscode").is_err(),
            "{name}"
        );
    }

    write(&base, "x-dembly: {}\n");
    write(
        &devcontainer,
        r#"{"dockerComposeFile":["base.yaml","../compose.yaml"],"service":"dev","remoteUser":"vscode"}"#,
    );
    let document = load_devcontainer(&devcontainer).unwrap();
    assert!(validate_devcontainer(&document, &devcontainer, &managed, "dev", "vscode").is_err());

    write(&base, "{x-dembly: {}}\n");
    let document = load_devcontainer(&devcontainer).unwrap();
    assert!(validate_devcontainer(&document, &devcontainer, &managed, "dev", "vscode").is_err());

    write(
        &base,
        "services:\n  dev:\n    x-dembly: {}\n# x-dembly: {}\n",
    );
    assert_eq!(
        validate_devcontainer(&document, &devcontainer, &managed, "dev", "vscode").unwrap(),
        [base.clone(), managed.clone()]
    );

    write(
        &devcontainer,
        r#"{"dockerComposeFile":"../compose.yaml","service":"dev"}"#,
    );
    assert!(load_devcontainer(&devcontainer).is_err());
}
