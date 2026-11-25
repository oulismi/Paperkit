mod commands;
mod shell;

pub fn run() {
    let mut builder = tauri::Builder::default();
    if shell::demo().is_none() {
        // Must be first: a second launch (e.g. from the Start menu while running
        // in the tray) just brings up the running PaperKit. Demo runs (for
        // screenshots) are allowed beside it.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::show_main(app)));
    }
    builder
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![shell::BACKGROUND_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(shell::ShellState::default())
        .manage(commands::model::ModelState::default())
        .manage(commands::redact::TextCache::default())
        .invoke_handler(tauri::generate_handler![
            commands::pdf::open_pdf,
            commands::pdf::read_pdf,
            commands::pdf::assemble,
            commands::redact::redact_scan,
            commands::redact::redact_search,
            commands::redact::redact_apply,
            commands::redact::redact_ai_scan,
            commands::convert::convert_pdf,
            commands::convert::images_to_pdf,
            commands::edit::edit_info,
            commands::edit::edit_apply,
            commands::edit::read_image,
            commands::edit::edit_font,
            commands::model::model_status,
            commands::model::model_download,
            commands::model::model_download_cancel,
            shell::open_in_main,
            shell::take_open_request,
            shell::bubble_moved,
            shell::bubble_hide,
            shell::show_main_window,
            shell::ai_offer,
            shell::ai_offer_decline,
            shell::demo_request,
        ])
        .setup(|app| {
            shell::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(shell::on_window_event)
        .run(tauri::generate_context!())
        .expect("error while running PaperKit");
}
