pub mod accounts;
// Phase 6.4: AI commit message suggestions (Ollama local / user API keys).
mod ai;
// Phase 3B.3: log / cache / app-data folders (Settings → Storage).
mod app_paths;
pub mod auth;
// Phase 6.3 / 6.6: encrypted account export/import, full backup/restore.
mod backup;
// Phase 6.1: safe branch switch (double-click a branch, autostash).
mod branch_switch;
mod branches;
// Phase 4.3: CI status of commits.
mod ci;
// Phase 3B.1: accounts signed in with the gh / glab CLI.
mod cli_accounts;
mod clone;
mod commands;
mod conflicts;
mod env_path;
mod error;
mod file_views;
// Phase 4.3: fork & clone, fork the open repository.
mod forks;
mod graph;
mod guards;
mod hosting;
mod import;
// Phase 4.2: issues of a repository, "My work" across accounts.
mod issues;
mod lfs;
mod lfs_transfer;
mod linked_worktrees;
// Phase 4.2: "My work" across accounts.
mod mywork;
// Phase 3B.2: first-run setup (tool check / install, progress).
mod onboarding;
// Phase 4.1: pull requests (GitHub) / merge requests (GitLab).
mod publish;
mod pulls;
mod rebase;
mod repos;
mod settings;
mod signing;
mod ssh_keys;
mod stash;
mod state;
mod submodules;
mod terminal;
mod undo;
// Phase 3B.4: Check for Update (GitHub Releases).
mod updater;
// Phase 6.2 / 6.5: custom themes and personal notes (stored in the settings table).
mod user_data;
mod worktree;

use std::path::PathBuf;

use specta_typescript::Typescript;
use tauri::Manager;
use tauri_specta::{Builder, ErrorHandlingMode, collect_commands, collect_events};

use crate::state::AppState;

/// Location of the generated TypeScript bindings, relative to this crate.
const BINDINGS_PATH: &str = "../src/ipc/bindings.ts";

pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .error_handling(ErrorHandlingMode::Throw)
        .dangerously_cast_bigints_to_number()
        .commands(collect_commands![
            commands::app_info,
            commands::get_settings,
            commands::update_settings,
            graph::graph_open,
            graph::graph_rows,
            graph::graph_find,
            graph::graph_search,
            graph::commit_details,
            graph::startup_args,
            graph::report_benchmark,
            repos::tool_status,
            repos::repo_open,
            repos::repo_init,
            repos::repo_close,
            repos::workspace_state,
            repos::workspace_set_active,
            repos::workspace_reorder,
            repos::recent_repos,
            repos::recent_remove,
            repos::repo_refs,
            worktree::worktree_status,
            worktree::file_diff,
            worktree::file_contents,
            worktree::stage_files,
            worktree::unstage_files,
            worktree::stage_all,
            worktree::unstage_all,
            worktree::discard_files,
            worktree::apply_lines,
            worktree::commit_create,
            worktree::commit_identity,
            worktree::head_message,
            // Conflict editor, file history, blame, image diff (step 3.2)
            conflicts::conflict_context,
            conflicts::conflict_file,
            conflicts::blob_preview,
            conflicts::conflict_resolve_content,
            conflicts::conflict_take_side,
            conflicts::conflict_delete_file,
            file_views::file_history,
            file_views::blame_file,
            file_views::file_at_commit,
            file_views::diff_images,
            stash::stash_create,
            stash::stash_apply,
            stash::stash_pop,
            stash::stash_drop,
            stash::stash_show,
            lfs::lfs_status,
            lfs::lfs_install_local,
            lfs::lfs_track,
            lfs::lfs_untrack,
            lfs::lfs_restage,
            lfs::large_file_check,
            branches::branch_create,
            branches::branch_checkout,
            branches::branch_checkout_remote,
            branches::commit_checkout,
            branches::branch_rename,
            branches::branch_delete,
            branches::branch_set_upstream,
            branches::tag_create,
            branches::tag_delete,
            branches::commit_cherry_pick,
            branches::commit_revert,
            branches::reset_to,
            branches::repo_operation_state,
            branches::operation_abort,
            branches::operation_continue,
            branches::merge_into_current,
            branches::remote_fetch,
            branches::remote_pull,
            branches::remote_push,
            branches::remote_push_tag,
            branches::remote_delete_branch,
            branches::transfer_cancel,
            // Rebase, interactive rebase, undo/redo, branch drag-and-drop (step 3.1)
            rebase::rebase_preview,
            rebase::rebase_todo,
            rebase::rebase_start,
            rebase::rebase_interactive,
            rebase::rebase_continue,
            rebase::rebase_skip,
            rebase::branch_relation,
            rebase::branch_fast_forward,
            undo::undo_state,
            undo::undo_run,
            undo::redo_run,
            lfs_transfer::lfs_push_estimate,
            lfs_transfer::lfs_push,
            lfs_transfer::lfs_download_estimate,
            lfs_transfer::lfs_pull,
            lfs_transfer::lfs_fetch_all,
            lfs_transfer::lfs_transfer_queue,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            accounts::commands::provider_list,
            accounts::commands::provider_save,
            accounts::commands::provider_delete,
            accounts::commands::account_list,
            accounts::commands::account_save,
            accounts::commands::account_delete,
            accounts::commands::account_reorder,
            accounts::commands::account_save_token,
            accounts::commands::account_delete_token,
            accounts::commands::identity_list,
            accounts::commands::identity_save,
            accounts::commands::identity_delete,
            accounts::commands::identity_set_default,
            accounts::commands::rule_list,
            accounts::commands::rule_save,
            accounts::commands::rule_delete,
            accounts::commands::rule_reorder,
            accounts::commands::rule_test,
            accounts::commands::repo_binding_get,
            accounts::commands::repo_binding_set,
            accounts::commands::repo_binding_clear,
            accounts::commands::repo_account_context,
            accounts::commands::account_settings_get,
            accounts::commands::account_settings_update,
            auth::auth_device_start,
            auth::auth_device_wait,
            auth::auth_pkce_start,
            auth::auth_pkce_wait,
            auth::auth_cancel,
            auth::auth_add_pat,
            auth::auth_update_token,
            auth::auth_config,
            auth::auth_config_set,
            auth::account_health,
            auth::account_health_all,
            auth::account_refresh,
            auth::account_emails,
            auth::account_colors,
            ssh_keys::ssh_key_list,
            ssh_keys::ssh_key_generate,
            ssh_keys::ssh_key_import,
            ssh_keys::ssh_key_delete,
            ssh_keys::ssh_key_set_default,
            ssh_keys::ssh_key_upload,
            ssh_keys::ssh_key_check_permissions,
            ssh_keys::ssh_key_repair_permissions,
            ssh_keys::ssh_test_connection,
            import::import_scan,
            import::import_apply,
            import::onboarding_status,
            import::onboarding_set_import_done,
            // Phase 3B.1: accounts signed in with the gh / glab CLI.
            cli_accounts::cli_accounts_scan,
            cli_accounts::cli_accounts_import,
            cli_accounts::cli_accounts_dismiss,
            // Phase 3B.2: first-run setup.
            onboarding::setup_status,
            onboarding::setup_save,
            onboarding::setup_tools,
            onboarding::setup_install,
            onboarding::setup_install_cancel,
            onboarding::setup_restart,
            clone::clone_repo_list,
            clone::clone_guess,
            clone::clone_destination,
            clone::clone_set_template,
            clone::clone_start,
            guards::commit_guard,
            guards::push_guard,
            guards::guard_fix_authors,
            guards::guard_lfs_migrate,
            guards::guard_undo,
            guards::guard_backups,
            guards::guard_backup_delete,
            // Phase 3.3: submodules, linked worktrees, LFS locks / files / maintenance, signing.
            submodules::submodule_list,
            submodules::submodule_update,
            submodules::submodule_add,
            submodules::submodule_url_account,
            submodules::submodule_sync,
            submodules::submodule_deinit,
            submodules::submodule_remove,
            linked_worktrees::linked_worktree_list,
            linked_worktrees::linked_worktree_add,
            linked_worktrees::linked_worktree_remove,
            linked_worktrees::linked_worktree_prune,
            linked_worktrees::linked_worktree_lock,
            lfs::lfs_locks,
            lfs::lfs_lock,
            lfs::lfs_unlock,
            lfs::lfs_set_lockable,
            lfs::lfs_files,
            lfs::lfs_storage,
            lfs::lfs_prune_preview,
            lfs::lfs_prune,
            lfs::lfs_fsck,
            lfs_transfer::lfs_pull_files,
            signing::signing_gpg_keys,
            signing::signing_test,
            signing::commit_signature,
            // Phase 4: hosted project of each remote (shared by PR/MR, issues, CI, forks).
            hosting::hosted_remote_list,
            // Phase 4.2: issues of a repository, "My work" across accounts.
            issues::issue_list,
            issues::issue_get,
            issues::issue_comments,
            issues::issue_comment_create,
            issues::issue_create,
            issues::issue_update,
            issues::issue_labels,
            issues::issue_assignable_users,
            issues::issue_branch_name,
            issues::issue_branch_create,
            mywork::mywork_list,
            mywork::mywork_cancel,
            // Phase 4.3: CI status of commits, fork & clone, fork the open repository.
            ci::ci_statuses,
            ci::ci_status_for,
            forks::fork_namespaces,
            forks::fork_find,
            forks::fork_create,
            forks::fork_wait,
            forks::fork_add_upstream,
            forks::fork_attach,
            publish::publish_create_repo,
            publish::publish_add_remote,
            // Phase 4.1: pull requests (GitHub) / merge requests (GitLab).
            pulls::pull_list,
            pulls::pull_detail,
            pulls::pull_reviews,
            pulls::pull_commits,
            pulls::pull_files,
            pulls::pull_comments,
            pulls::pull_comment_create,
            pulls::pull_review_submit,
            pulls::pull_create,
            pulls::pull_merge,
            pulls::pull_set_open,
            pulls::pull_create_context,
            pulls::pull_checkout,
            pulls::pull_local_diff,
            // Phase 3B.4: Check for Update (GitHub Releases).
            updater::updater_status,
            updater::updater_check,
            updater::updater_install,
            updater::updater_skip_version,
            updater::updater_set_channel,
            // Phase 3B.3: Settings → Storage (log / cache / app-data folders).
            app_paths::storage_overview,
            app_paths::storage_status,
            app_paths::storage_check_target,
            app_paths::storage_change,
            app_paths::storage_cancel_pending,
            app_paths::storage_open,
            app_paths::storage_clear_cache,
            app_paths::storage_prune_logs,
            app_paths::storage_restart,
            // Phase 6.1: safe branch switch (double-click a branch, autostash).
            branch_switch::branch_switch_check,
            branch_switch::branch_switch_run,
            branch_switch::branch_switch_restore,
            branch_switch::branch_switch_prefs_get,
            branch_switch::branch_switch_prefs_set,
            // Phase 6.2 / 6.5: custom themes, personal notes, Settings → Storage summary.
            user_data::custom_theme_list,
            user_data::custom_theme_save,
            user_data::custom_theme_delete,
            user_data::custom_theme_delete_all,
            user_data::custom_theme_set_active,
            user_data::custom_theme_export,
            user_data::custom_theme_import,
            user_data::notes_load,
            user_data::notes_save,
            user_data::notes_delete,
            user_data::notes_delete_all,
            user_data::notes_tag_save,
            user_data::notes_tag_delete,
            user_data::notes_export_file,
            user_data::notes_import_file,
            user_data::notes_export_markdown,
            user_data::user_data_summary,
            // Phase 6.4: AI commit message suggestions (settings, keys, Ollama helpers).
            ai::ai_settings,
            ai::ai_settings_save,
            ai::ai_key_set,
            ai::ai_key_delete,
            ai::ai_test,
            ai::ai_generate,
            ai::ai_cancel,
            ai::ai_ollama_status,
            ai::ai_ollama_install,
            ai::ai_ollama_install_cancel,
            ai::ai_ollama_start,
            ai::ai_ollama_pull,
            ai::ai_ollama_delete,
            // Phase 6.3 / 6.6: account export/import, full backup/restore.
            backup::backup_accounts_export,
            backup::backup_accounts_open,
            backup::backup_accounts_import,
            backup::backup_full_create,
            backup::backup_full_open,
            backup::backup_full_restore,
            backup::backup_restore_notice,
        ])
        .events(collect_events![
            repos::RepoChanged,
            branches::TransferProgress,
            lfs_transfer::LfsTransferProgress,
            clone::CloneProgress,
            clone::CloneListUpdated,
            updater::UpdateProgress,
            // Phase 3B.3: log/cache folder move progress.
            app_paths::StorageMoveProgress,
            // Phase 6.4: Ollama model download progress.
            ai::ollama::AiPullProgress
        ])
}

pub fn export_bindings(builder: &Builder<tauri::Wry>) -> Result<PathBuf, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BINDINGS_PATH);
    builder
        .export(Typescript::default(), &path)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // First thing, before any thread: a GUI app on macOS / Linux starts with a minimal `PATH`
    // without Homebrew & co., which would hide git-lfs, gh, glab and brew for ever.
    let path_fix = env_path::fix();
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(&builder) {
        eprintln!("failed to export TypeScript bindings: {e}");
    }

    let mut context = tauri::generate_context!();
    // Phase 3B.4: the updater plugin needs a `plugins.updater` section (filled at runtime).
    updater::ensure_plugin_config(context.config_mut());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Phase 3B.4: no JS permission is granted; the app's own commands drive it.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Phase 3B.3: folders from `locations.json`; logging first, then a data move
            // scheduled last session, then the database.
            // 1.0 changed the bundle identifier: bring over data of earlier builds first.
            let legacy = app_paths::migrate_legacy_identifier(app.handle());
            let paths = app_paths::AppPaths::resolve(app.handle())?;
            paths.init_logging()?;
            if !path_fix.added.is_empty() {
                tracing::info!(
                    added = ?path_fix.added,
                    from_login_shell = path_fix.from_login_shell,
                    "extended PATH for a GUI launch"
                );
            }
            let legacy = match legacy {
                Ok(Some(moved)) => {
                    tracing::info!(from = %moved.from.display(), "copied the data of an earlier bundle identifier");
                    Some(moved)
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(error = %e, "could not copy the data of an earlier bundle identifier");
                    None
                }
            };
            let moved = paths.apply_pending_data();
            let log_dir = paths.get(nanomgg_app_paths::StorageKind::Log);
            let data_dir = paths.data_dir();
            app.manage(paths);
            // Phase 6.6: a restore staged last session replaces the database before it opens.
            app.manage(backup::apply_pending_restore(&data_dir));

            let storage = std::sync::Arc::new(nanomgg_storage::Storage::open(
                &data_dir.join("nanomgg.db"),
            )?);
            for moved in [&legacy, &moved].into_iter().flatten() {
                app_paths::after_data_move(&storage, moved);
            }
            tracing::info!(
                version = %app.package_info().version,
                data_dir = %data_dir.display(),
                log_dir = %log_dir.display(),
                schema = storage.schema_version()?,
                "NANOMultipleGitGUI started"
            );
            // Credential server (OS keychain) + sidecar for per-account git credentials.
            let accounts = accounts::AccountsRuntime::start(
                storage.clone(),
                std::sync::Arc::new(nanomgg_credentials::OsKeychain),
                &data_dir,
            );
            app.manage(AppState::with_accounts(storage, accounts));
            // Sign-in flows + shared HTTP client; refreshes GitLab OAuth tokens in the background.
            auth::setup(app.handle())?;
            app.manage(terminal::Terminals::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Shells must not outlive the app (managed state is not dropped on exit).
            if window.label() == "main"
                && matches!(event, tauri::WindowEvent::Destroyed)
                && let Some(terms) = window.try_state::<terminal::Terminals>()
            {
                terms.close_all();
            }
        })
        .on_page_load(|webview, payload| {
            // A reload drops every xterm view; their shells would otherwise linger unseen.
            if payload.event() == tauri::webview::PageLoadEvent::Started
                && let Some(terms) = webview.try_state::<terminal::Terminals>()
            {
                terms.close_all();
            }
        })
        .run(context)
        .expect("error while running NANOMultipleGitGUI");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::AppInfo;

    #[test]
    fn app_info_serializes_camel_case() {
        let info = AppInfo::current("NANOMultipleGitGUI", "0.0.1");
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "NANOMultipleGitGUI");
        assert_eq!(json["version"], "0.0.1");
        assert_eq!(json["os"], std::env::consts::OS);
        assert_eq!(json["arch"], std::env::consts::ARCH);
    }

    #[test]
    fn app_error_serializes_as_tagged_object() {
        let err = error::AppError::InvalidInput("bad path".into());
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            serde_json::json!({ "kind": "invalidInput", "message": "bad path" })
        );
    }

    /// Regenerates `src/ipc/bindings.ts`. CI fails if the committed file differs afterwards.
    #[test]
    fn export_typescript_bindings() {
        let path = export_bindings(&specta_builder()).expect("export bindings");
        let ts = std::fs::read_to_string(path).unwrap();
        for name in [
            "appInfo",
            "getSettings",
            "updateSettings",
            "graphOpen",
            "graphRows",
            "AppSettings",
            "AppError",
            "GraphRow",
            "graphFind",
            "graphSearch",
            "commitDetails",
            "CommitDetails",
            "repoOpen",
            "workspaceState",
            "repoRefs",
            "RepoRefs",
            "repoChanged",
            "worktreeStatus",
            "WorkingTreeStatus",
            "fileDiff",
            "FileDiff",
            "applyLines",
            "commitCreate",
            "stashCreate",
            "stashPop",
            "StashApplyOutcome",
            "StashDetails",
            "lfsStatus",
            "LfsStatus",
            "lfsTrack",
            "largeFileCheck",
            "GuardReport",
            "branchCreate",
            "branchCheckoutRemote",
            "branchDelete",
            "tagCreate",
            "commitCherryPick",
            "resetTo",
            "repoOperationState",
            "operationContinue",
            "mergeIntoCurrent",
            "remoteFetch",
            "remotePull",
            "remotePush",
            "transferCancel",
            "Outcome",
            "FailureKind",
            "MergeOutcome",
            "OpOutcome",
            "OperationState",
            "PushRequest",
            "transferProgress",
            "lfsPushEstimate",
            "lfsPush",
            "lfsPull",
            "lfsFetchAll",
            "lfsTransferQueue",
            "lfsTransferProgress",
            "LfsFailureKind",
            "terminalOpen",
            "terminalWrite",
            "terminalResize",
            "terminalClose",
            "TerminalEvent",
            "TerminalInfo",
            "providerList",
            "providerSave",
            "accountSave",
            "accountSaveToken",
            "identitySave",
            "ruleSave",
            "ruleReorder",
            "ruleTest",
            "repoBindingSet",
            "repoBindingClear",
            "repoAccountContext",
            "accountSettingsUpdate",
            "RepoAccountContext",
            "RemoteAccountContext",
            "RuleTestResult",
            "ContextSource",
            "ProviderKind",
            "AccountSecret",
            "sshKeyList",
            "sshKeyUpload",
            "sshTestConnection",
            "SshKeyInfo",
            "SshUploadResult",
            "importScan",
            "importApply",
            "onboardingStatus",
            "ImportReport",
            "ImportSelection",
            "publishCreateRepo",
            "publishAddRemote",
            "NewRepoOptions",
        ] {
            assert!(ts.contains(name), "bindings missing {name}");
        }
    }
}
