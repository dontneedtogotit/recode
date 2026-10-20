fn normalize_fractional_scaling() {
    // On Wayland (or XWayland) the compositor owns display scaling and applies
    // it to X11/XWayland windows at the monitor's (possibly fractional) factor.
    // GTK3/webkit2gtk additionally honor the *integer* GDK_SCALE env var, so an
    // inherited GDK_SCALE=2 makes the webview render at 2x on top of the
    // compositor's own (e.g. 1.6x) scale -- everything ends up ~25% too big.
    // Force GDK_SCALE=1 so the webview matches the rest of the desktop, which
    // the compositor scales itself. Under pure X11 (no Wayland) we leave it
    // alone, where GDK_SCALE is the only scaling mechanism.
    let on_wayland = std::env::var_os("XDG_SESSION_TYPE")
        .as_deref()
        .map(|t| t.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    if !on_wayland {
        return;
    }
    let scale: u32 = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1);
    if scale > 1 {
        std::env::set_var("GDK_SCALE", "1");
        std::env::set_var("GDK_DPI_SCALE", "1");
    }
}

fn main() {
    // Initialize tracing subscriber for structured spans
    tracing_subscriber::fmt::try_init().ok();
    normalize_fractional_scaling();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            recode::tauri_commands::create_workspace,
            recode::tauri_commands::list_workspaces,
            recode::tauri_commands::write_project_file,
            recode::tauri_commands::read_project_file,
            recode::tauri_commands::list_project_files,
            recode::tauri_commands::cancel_build,
            recode::tauri_commands::validate_workspace_path,
            recode::tauri_commands::create_and_execute_plan,
            recode::tauri_commands::run_pipeline,
            recode::tauri_commands::reconcile_workspace,
            recode::tauri_commands::install_dependencies,
            recode::tauri_commands::verify_build,
            recode::tauri_commands::get_generation_checkpoint,
            recode::tauri_commands::cancel_pipeline,
            recode::tauri_commands::pause_pipeline,
            recode::tauri_commands::resume_pipeline,
            recode::tauri_commands::abort_pipeline,
            recode::tauri_commands::get_pipeline_status,
            recode::tauri_commands::report_dev_server_status,
            recode::tauri_commands::run_quality_scan,
            recode::tauri_commands::record_quality_baseline,
            recode::tauri_commands::list_quality_baselines,
            recode::tauri_commands::delete_quality_baseline,
            recode::tauri_commands::save_agent_task_run,
            recode::tauri_commands::list_agent_task_runs,
            recode::tauri_commands::save_plan,
            recode::tauri_commands::list_plans,
            recode::tauri_commands::record_plan_metrics,
            recode::tauri_commands::attempt_self_heal,
            recode::tauri_commands::attempt_startup_self_heal,
            recode::tauri_commands::list_monitor_events,
            recode::tauri_commands::list_agent_task_metrics,
            recode::tauri_commands::capture_diagnostics,
            recode::updater::commands::check_updates,
            recode::updater::commands::verify_signature,
            recode::ipc_benchmark::run_ipc_benchmark_command,
            recode::startup_benchmark::run_startup_benchmark_command,
            recode::agent_runtime_settings::commands::save_agent_runtime_settings,
            recode::agent_runtime_settings::commands::load_agent_runtime_settings,
            recode::provider_settings::commands::save_provider_settings,
            recode::provider_settings::commands::load_provider_settings,
            recode::tauri_commands::git_status,
            recode::tauri_commands::git_init,
            recode::tauri_commands::git_add_all,
            recode::tauri_commands::git_commit,
            recode::agents::list_agents,
            recode::agents::register_agent,
            recode::agents::execute_agent_task,
            recode::tauri_commands::record_agent_task_history,
            recode::tauri_commands::list_agent_task_history,
            recode::tauri_commands::create_team_task,
            recode::tauri_commands::list_team_tasks,
            recode::tauri_commands::ensure_learning_schema,
            recode::tauri_commands::record_success_pattern,
            recode::tauri_commands::record_anti_pattern,
            recode::tauri_commands::list_success_patterns,
            recode::tauri_commands::list_anti_patterns,
            recode::tauri_commands::record_pipeline_run,
            recode::tauri_commands::list_pipeline_runs,
            recode::tauri_commands::get_pipeline_result,
            recode::tauri_commands::run_gateway_task,
            recode::tauri_commands::get_gateway_status,
            recode::tauri_commands::save_provider,
            recode::tauri_commands::list_providers,
            recode::tauri_commands::delete_provider,
            recode::tauri_commands::get_provider_selection_status,
            recode::tauri_commands::list_models,
            recode::tauri_commands::upsert_model,
            recode::tauri_commands::remove_model,
            recode::tauri_commands::discover_provider_models,
            recode::tauri_commands::import_env_api_key,
            recode::tauri_commands::list_provider_credentials,
            recode::tauri_commands::save_provider_config,
            recode::tauri_commands::save_provider_api_key,
            recode::tauri_commands::delete_provider_api_key,
            recode::tauri_commands::verify_provider_key,
            recode::tauri_commands::calibrate_model_concurrency,
            recode::tauri_commands::get_selection_state,
            recode::tauri_commands::set_model_selected,
            recode::tauri_commands::set_provider_enabled,
            recode::tauri_commands::route_models,
            recode::tauri_commands::infer_routing_request,
            recode::tauri_commands::run_orchestrator_step,
            recode::tauri_commands::run_orchestrator_autonomous,
        ])
        .on_menu_event(|app, event| {
            let _ = event;
            let _ = app;
        })
        .setup(|app| {
            #[cfg(desktop)]
            {
                let menu = tauri::menu::MenuBuilder::new(app)
                    .text("recode", "recode")
                    .separator()
                    .quit()
                    .build()?;
                app.set_menu(menu)?;
            }

            let _ = recode::tauri_commands::attempt_startup_self_heal();

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
