# Requirement Traceability

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-DCK-002 to REQ-DCK-004 | `discovery_does_not_search_parent_directories` | Unit tested |
| REQ-CFG-004 | `deck_parser_rejects_unknown_fields` | Unit tested |
| REQ-CRD-040 to REQ-CRD-041 | `checksum_verification_accepts_matching_file_and_rejects_mismatch` | Unit tested |
| REQ-BND-001 to REQ-BND-007 | `bind_variables_are_scoped_by_source_and_target` | Unit tested |
| REQ-VOL-011 to REQ-VOL-021 | `volume_layout_distinguishes_private_and_shared_card_volumes`, `mixed_shared_volume_declarations_are_rejected` | Unit tested |
| REQ-MNT-001 to REQ-MNT-002 | `exact_mount_target_collision_is_rejected_but_nested_targets_are_allowed` | Unit tested |
| REQ-ENV-001 to REQ-ENV-012 | `card_environment_overrides_deck_and_path_preserves_card_order`, `duplicate_card_environment_keys_are_rejected` | Unit tested |
| REQ-CBL-021 | `non_interactive_build_requires_name_and_version_without_prompting` | Unit tested |
| REQ-RST-004, REQ-IMG-003 | `image_runtime_plan_binds_same_binary_and_card_files_read_only` | Unit tested |
| REQ-SQF-021 to REQ-SQF-024 | `runtime_mount_uses_external_read_only_kernel_squashfs_command` | Unit tested |
| REQ-CMP-010 to REQ-CMP-011 | `compose_override_changes_only_selected_service_with_internal_entrypoint` | Unit tested |
| REQ-CLI-001 | `validate_checks_card_manifest_and_filesystem_from_deck_root` | Unit tested |
| REQ-CLI-060 | `inspect_displays_deck_name_base_and_card_without_runtime` | Unit tested |
| IT-001 to IT-040 | Not implemented | Unverified |
| REQ-DOD-006 to REQ-DOD-016 | Docker integration and full lifecycle are not implemented | Unverified |

`cargo test --workspace` covers the unit tests listed above.

Docker, Compose, SquashFS mount, loop device, static musl binary, and proprietary Card acceptance require separate integration coverage before completion can be claimed.
