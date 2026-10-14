pub mod accounts;
pub mod auth;
mod branches;
mod clone;
mod commands;
mod error;
mod graph;
mod guards;
mod import;
mod lfs;
mod lfs_transfer;
mod repos;
mod settings;
mod ssh_keys;
mod stash;
mod state;
mod terminal;
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
        ])
        .events(collect_events![
            repos::RepoChanged,
            branches::TransferProgress,
            lfs_transfer::LfsTransferProgress,
            clone::CloneProgress,
            clone::CloneListUpdated
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
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(&builder) {
        eprintln!("failed to export TypeScript bindings: {e}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let log_dir = app.path().app_log_dir()?;
            let guard = nanomgg_logging::init(&log_dir)?;
            app.manage(guard);

            let data_dir = app.path().app_data_dir()?;
            let storage = std::sync::Arc::new(nanomgg_storage::Storage::open(
                &data_dir.join("nanomgg.db"),
            )?);
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
        .run(tauri::generate_context!())
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
        ] {
            assert!(ts.contains(name), "bindings missing {name}");
        }
    }
}
