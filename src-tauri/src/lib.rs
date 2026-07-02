pub mod audit;
pub mod automation;
pub mod commands;
pub mod config;
pub mod layout;
pub mod model;
pub mod output;
pub mod paste;
pub mod pipe;
pub mod session;
pub mod spool;
pub mod state;
pub mod template;

use state::{AppState, WorkspaceStore};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Current process RSS in bytes (also used by the benchmark).
pub fn current_rss_bytes() -> u64 {
    use sysinfo::{ProcessesToUpdate, System};
    let Ok(pid) = sysinfo::get_current_pid() else {
        return 0;
    };
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let config_path = config_dir.join("config.json");

            let cfg = match config::load_config(&config_path) {
                Ok(Some(cfg)) => cfg,
                Ok(None) => WorkspaceStore::with_default().to_config(Vec::new()),
                Err(e) => {
                    // Corrupt/unsupported config: keep the file for inspection,
                    // start with a fresh default rather than crashing.
                    eprintln!("[terminal-f] config load failed, starting fresh: {e}");
                    let backup = config_path.with_extension("json.invalid");
                    let _ = std::fs::copy(&config_path, backup);
                    WorkspaceStore::with_default().to_config(Vec::new())
                }
            };
            let automation_rules = cfg.automation.clone();
            let store = WorkspaceStore::from_config(cfg);

            let spool_dir = config_path
                .parent()
                .map(|p| p.join("spool"))
                .unwrap_or_else(|| PathBuf::from("spool"));
            let registry = Arc::new(session::SessionRegistry::with_spool_dir(spool_dir));
            app.manage(AppState {
                store: Mutex::new(store),
                registry: Arc::clone(&registry),
                config_path,
                injection_paused: std::sync::atomic::AtomicBool::new(false),
                automation: Mutex::new(automation::AutomationState::with_rules(automation_rules)),
            });

            // Automation poll loop (M2.1): evaluates git-watch rules every
            // POLL_INTERVAL_MS on a background thread.
            let auto_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("automation-poll".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(
                        automation::POLL_INTERVAL_MS,
                    ));
                    commands::poll_automation(&auto_handle);
                })
                .expect("failed to spawn automation thread");

            // Control API (M2.2): named-pipe server for external brokers.
            // Token + pipe name are written to control-api.json (user-readable)
            // so a broker can find and authenticate to the pipe.
            {
                let pipe_name = format!("terminal-f-{}.sock", &model::new_id()[..8]);
                let token = model::new_id();
                let info = pipe::ControlApiInfo {
                    pipe_name: pipe_name.clone(),
                    token: token.clone(),
                };
                if let Some(state) = app.try_state::<AppState>() {
                    let info_path = commands::control_api_info_path(&state);
                    if let Ok(json) = serde_json::to_string_pretty(&info) {
                        let _ = std::fs::write(&info_path, json);
                    }
                }
                let pipe_handle = app.handle().clone();
                let handler = std::sync::Arc::new(
                    move |method: &str, params: &serde_json::Value, conn: &mut pipe::ConnState| {
                        commands::handle_pipe_method(&pipe_handle, method, params, conn)
                    },
                );
                if let Err(e) = pipe::start_server(pipe_name, token, handler) {
                    eprintln!("[terminal-f] control API failed to start: {e}");
                }
            }

            let out_handle = app.handle().clone();
            let exit_handle = app.handle().clone();
            output::start_emitter(
                registry,
                move |ev| {
                    let _ = out_handle.emit("pty-output", &ev);
                },
                move |ev| {
                    let _ = exit_handle.emit("pty-exit", &ev);
                },
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::workspace_activity,
            commands::reorder_workspaces,
            commands::set_workspace_color,
            commands::set_ui_prefs,
            commands::list_workspaces,
            commands::create_workspace,
            commands::rename_workspace,
            commands::delete_workspace,
            commands::switch_workspace,
            commands::split_pane,
            commands::close_pane,
            commands::resize_split,
            commands::set_active_pane,
            commands::write_pane,
            commands::set_pane_labels,
            commands::set_pane_injection,
            commands::set_pane_observe,
            commands::read_pane_output,
            commands::control_api_info,
            commands::inject_prompt,
            commands::set_injection_paused,
            commands::injection_status,
            commands::read_audit,
            commands::list_rules,
            commands::upsert_rule,
            commands::remove_rule,
            commands::set_rule_enabled,
            commands::list_proposals,
            commands::resolve_proposal,
            commands::run_rule_now,
            commands::apply_template,
            commands::list_templates,
            commands::get_template,
            commands::save_template,
            commands::delete_template,
            commands::workspace_as_template,
            commands::read_repo_profile,
            commands::trust_repo,
            commands::save_pasted_image,
            commands::paste_clipboard,
            commands::resize_pty,
            commands::replay_pane,
            commands::get_boot_info,
            commands::memory_stats,
            commands::autotest_report,
            commands::exit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building terminal-f")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    state.registry.shutdown();
                }
            }
        });
}
