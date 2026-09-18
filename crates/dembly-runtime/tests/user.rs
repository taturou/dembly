use dembly_runtime::resolve_runtime_user;

const PASSWD: &str = concat!(
    "root:x:0:0:root:/root:/bin/sh\n",
    "vscode:x:1000:1001:VS Code:/home/vscode:/bin/bash\n",
    "largeuid:x:4294967296:1001:Large:/home/large:/bin/sh\n",
    "largegid:x:1002:4294967296:Large:/home/largegid:/bin/sh\n",
);
const GROUP: &str = concat!(
    "root:x:0:\n",
    "developers:x:2000:vscode\n",
    "large:x:4294967296:\n",
);

#[test]
fn resolves_user_and_uid_with_primary_gid_fallback() {
    let by_name = resolve_runtime_user("vscode", PASSWD, GROUP).unwrap();
    let by_uid = resolve_runtime_user("1000", PASSWD, GROUP).unwrap();

    assert_eq!(by_name.name, "vscode");
    assert_eq!(by_name.uid, 1000);
    assert_eq!(by_name.gid, 1001);
    assert_eq!(by_name.home, "/home/vscode");
    assert_eq!(by_uid, by_name);
}

#[test]
fn resolves_named_and_numeric_explicit_groups() {
    assert_eq!(
        resolve_runtime_user("vscode:developers", PASSWD, GROUP)
            .unwrap()
            .gid,
        2000
    );
    assert_eq!(
        resolve_runtime_user("1000:3000", PASSWD, GROUP)
            .unwrap()
            .gid,
        3000
    );
}

#[test]
fn rejects_named_user_with_numeric_group() {
    let error = resolve_runtime_user("vscode:3000", PASSWD, GROUP).unwrap_err();

    assert!(error.contains("invalid runtime user spec"), "{error}");
}

#[test]
fn rejects_numeric_uid_with_named_group() {
    let error = resolve_runtime_user("1000:developers", PASSWD, GROUP).unwrap_err();

    assert!(error.contains("invalid runtime user spec"), "{error}");
}

#[test]
fn rejects_unresolvable_or_malformed_user_specs() {
    for spec in ["", ":developers", "vscode:", "vscode:developers:extra"] {
        assert!(resolve_runtime_user(spec, PASSWD, GROUP).is_err(), "{spec}");
    }

    let missing_uid = resolve_runtime_user("1234", PASSWD, GROUP).unwrap_err();
    assert!(missing_uid.contains("passwd"), "{missing_uid}");

    let missing_group = resolve_runtime_user("vscode:missing", PASSWD, GROUP).unwrap_err();
    assert!(missing_group.contains("group"), "{missing_group}");
}

#[test]
fn rejects_uid_and_gid_values_outside_u32() {
    for spec in [
        "4294967296",
        "vscode:4294967296",
        "largeuid",
        "largegid",
        "vscode:large",
    ] {
        let error = resolve_runtime_user(spec, PASSWD, GROUP).unwrap_err();
        assert!(error.contains("u32"), "{spec}: {error}");
    }
}
