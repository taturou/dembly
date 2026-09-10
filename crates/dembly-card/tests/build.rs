use dembly_card::{build_card, CardBuildRequest};
use std::path::PathBuf;

#[test]
fn non_interactive_build_requires_name_and_version_without_prompting() {
    let request = CardBuildRequest {
        tool_root: PathBuf::from("/tool"),
        cards_root: PathBuf::from("/cards"),
        name: None,
        version: None,
        mount_target: None,
        path_prepend: Vec::new(),
        non_interactive: true,
    };

    let error = build_card(&request).unwrap_err();
    assert!(error.to_string().contains("--name"));
}
