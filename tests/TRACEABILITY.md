# Requirement Traceability

`design/spec.md` 14節の受入条件を、上から順に AC-14-01 から AC-14-20 として対応付けます。

## 14節の受入条件

| ID | 受入条件 | 自動化された証拠 |
| --- | --- | --- |
| AC-14-01 | `init` が人間の選択を経て `.dembly/config.toml` を生成する | `one_card_candidate_requires_an_explicit_adoption_confirmation`、`selected_devcontainer_shows_service_and_compose_sequence_before_confirmation`、`no_devcontainer_prompts_for_compose_path_and_service` |
| AC-14-02 | `init` が固定した Card と Dev Containers の探索位置だけを対象とし、深い階層と symlink を除外する | `discovers_only_direct_regular_files_in_fixed_roots_in_lexical_order`、`excludes_symlinked_directories_and_candidate_files`、`excludes_cards_when_a_fixed_root_parent_is_a_symlink` |
| AC-14-03 | 公開 Host command が container lifecycle と container 内 command を実行しない | `public_help_lists_host_commands_without_legacy_lifecycle_commands`、`validate_resolves_explicit_config_without_writing_project_files`、`check_verifies_static_host_integrity_without_running_card_checks`、各 Host command fixture の read-only Docker call assertion |
| AC-14-04 | `lock` だけが `x-dembly.lock` を更新する | `set_lock_changes_only_the_embedded_lock_and_is_deterministic`、`lock_embeds_resolved_identities_and_is_idempotent`、`apply_stages_runtime_artifacts_and_updates_only_the_selected_service` |
| AC-14-05 | Lock がないか無効な場合に `apply` と `check` が失敗する | `apply_requires_a_fresh_lock_before_writing_and_reapply_is_stable`、`check_rejects_a_missing_lock_without_writing_project_files`、`check_rejects_a_stale_lock_without_writing_project_files` |
| AC-14-06 | `apply` が選択 service だけを更新し、他 service と管理外 field を保持する | `first_apply_records_exact_values_and_preserves_other_services_and_entries`、`apply_stages_runtime_artifacts_and_updates_only_the_selected_service` |
| AC-14-07 | `apply` と `unapply` が外部変更を検出し、利用者の変更を上書きしない | `apply_rejects_external_managed_field_edits_without_artifact_writes`、`unapply_conflict_preserves_compose_and_runtime`、`unapply_conflict_leaves_every_managed_value_and_state_unchanged` |
| AC-14-08 | 同じ入力への `lock` と `apply` が追跡対象 file に差分を残さない | `lock_embeds_resolved_identities_and_is_idempotent`、`apply_requires_a_fresh_lock_before_writing_and_reapply_is_stable`、`identical_reapply_is_byte_stable_after_initial_normalization` |
| AC-14-09 | `apply` 直後の `unapply` が管理 field を適用前へ戻す | `unapply_restores_original_fields_and_retains_persistent_data`、`unapply_restores_missing_and_null_and_removes_only_state` |
| AC-14-10 | Card 変更が基礎 image の再 build なしで Compose container の再作成へ反映される | `native_compose_owns_applied_runtime_lifecycle` |
| AC-14-11 | SquashFS mount が Runtime の mount namespace だけに存在する | `native_compose_owns_applied_runtime_lifecycle`、`card_mount_command_is_always_read_only` |
| AC-14-12 | Runtime Dembly と post-mount hook が root で動作する | `init_runs_root_setup_in_strict_order_before_intended_user_exec`、`non_root_init_stops_before_plan_or_account_reads_and_mounts`、`native_compose_owns_applied_runtime_lifecycle` |
| AC-14-13 | 元の process、Compose `run` command、Dev Containers 接続 process が指定利用者で動作する | `init_uses_saved_argv_by_default_and_replaces_it_with_compose_run_argv`、`native_compose_owns_applied_runtime_lifecycle` と同 test 内の `devcontainer read-configuration` assertion |
| AC-14-14 | native Compose の `ps`、`logs`、`exec`、`run`、`down` が同じ project へ作用する | `native_compose_owns_applied_runtime_lifecycle` |
| AC-14-15 | AI の native Compose 運用と人間の Dev Containers 運用が同じ Compose file、Dockerfile、Card を使用する | `native_compose_owns_applied_runtime_lifecycle` と同 test 内の Compose file order、service、user assertion |
| AC-14-16 | Runtime 初期化または Card check の失敗が非ゼロになる | `setup_failure_names_the_operation_and_target_and_prevents_drop_and_exec`、`check_failure_names_card_and_path_and_stops_later_checks`、`native_compose_owns_applied_runtime_lifecycle` |
| AC-14-17 | Runtime 初期化失敗時に元の process と Compose `run` command を実行しない | `invalid_plan_or_user_stops_before_card_mount`、`environment_failure_prevents_root_hook_privilege_drop_and_exec`、`setup_failure_names_the_operation_and_target_and_prevents_drop_and_exec` |
| AC-14-18 | Runtime 初期化の失敗理由を標準エラーと Compose logs から確認できる | `setup_failure_names_the_operation_and_target_and_prevents_drop_and_exec`、`native_compose_owns_applied_runtime_lifecycle` |
| AC-14-19 | Docker 形式の利用者指定を保存し、container 内情報から UID、GID、home を解決する | `intended_user_falls_back_to_image_user_then_root`、`resolves_user_and_uid_with_primary_gid_fallback`、`resolves_named_and_numeric_explicit_groups`、`rejects_unresolvable_or_malformed_user_specs` |
| AC-14-20 | 未対応 schema または未知 field の検出時に入力 file を変更しない | `config_parser_requires_compose_and_rejects_unknown_fields_and_schema_versions`、`invalid_project_and_x_dembly_schemas_are_rejected_without_writes`、`runtime_config_rejects_unknown_fields_at_every_level`、`runtime_config_requires_supported_schema_and_nonempty_lock_digest`、read-only Host command tests |

## 横断的な仕様証拠

| 仕様領域 | 自動化された証拠 |
| --- | --- |
| Card checksum と Lock digest | `checksum_verification_accepts_matching_file_and_rejects_mismatch`、`digest_changes_with_manifest_or_verified_filesystem_identity`、`verifies_card_filesystem_checksum_before_docker_inspection` |
| Card、mount、export、environment の競合 | `exact_mount_target_collision_is_rejected_but_nested_targets_are_allowed`、`validate_rejects_duplicate_export_targets_across_cards`、`duplicate_card_environment_keys_are_rejected` |
| Volume と Host Bind | `volume_layout_distinguishes_private_and_shared_card_volumes`、`mixed_shared_volume_declarations_are_rejected`、`validate_rejects_a_missing_required_host_bind`、`validate_warns_about_an_optional_missing_bind_without_writing_project_files` |
| Dev Containers 契約 | `loads_string_and_array_compose_files_with_strict_field_types`、`validates_devcontainer_against_the_managed_compose_contract`、`rejects_devcontainer_cross_file_mismatches` |
| 原子的な file 更新 | `atomic_replace_preserves_permissions_and_replaces_all_bytes`、`atomic_replace_write_failure_leaves_the_target_unchanged`、`artifact_generation_failure_keeps_the_applied_compose_and_all_artifacts` |
| Card build | `interactive_card_build_reads_prompts_and_reports_artifact_checksum`、`card_build_non_interactive_rejects_missing_required_options` |

## 手動環境の前提

`cargo test --workspace` の完全実行には、接続可能な Linux Docker Engine と Docker Compose plugin が必要です。

`native_compose_owns_applied_runtime_lifecycle` には、root または Docker daemon を利用できる権限、privileged container の起動能力、loop device、kernel SquashFS support、`mksquashfs`、musl target が必要です。

同 test の Dev Containers 設定検証には `devcontainer` CLI が必要であり、未導入環境ではこの任意 integration の検証前提を満たしません。

Host mount namespace に Runtime の Card mount が現れないこと、Compose project label がトップレベル `name` と一致すること、Card 更新後に container ID が変わって image ID が変わらないことは、`native_compose_owns_applied_runtime_lifecycle` が対応する環境上で検証します。
