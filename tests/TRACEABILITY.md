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
| REQ-LCK-001 to REQ-LCK-023 | `dembly lock`, `enforce_image_lock`, `enforce_compose_lock`, `lock_writer_round_trips_*` | Implemented; parser/writer unit tested, Docker identity verification unverified |
| REQ-CLI-020 to REQ-CLI-052 | `up`, `down`, `run_command`, `exec_command` | Implemented; Docker integration unverified |
| REQ-CMP-020 to REQ-CMP-034 | `up_compose`, `down_compose`, `run_compose`, `exec_compose`, `check_compose` | Implemented; Compose integration unverified |
| REQ-USR-001 to REQ-USR-007 | `runtime_probe`, `probe_image_user`, `runtime_user` | Implemented; image user probe integration unverified |
| REQ-RUN-010 to REQ-RUN-011 | `runtime_command`, `create_exports`, `run_hooks` | Implemented; Runtime integration unverified |
| IT-001, IT-003, IT-007, IT-027, IT-029, IT-030, IT-033 | `image_base_card_runs_without_host_squashfs_mount` | Verified by `cargo test --workspace` on this Docker/SquashFS host |
| IT-002, IT-004 to IT-006, IT-008 to IT-026, IT-028, IT-031 to IT-040 | Not implemented | Unverified |
| REQ-DOD-006 to REQ-DOD-016 | lifecycle implementation exists; integration acceptance coverage remains incomplete | Partially verified |

`cargo test --workspace` covers the unit tests listed above.

静的 musl binary は `cargo build --release --target x86_64-unknown-linux-musl -p dembly-cli` で検証済みです。
Docker、Compose、SquashFS mount、loop device、および proprietary Card acceptance は別途 integration coverage が必要です。
