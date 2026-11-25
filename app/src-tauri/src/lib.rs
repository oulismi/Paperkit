mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::model::ModelState::default())
        .invoke_handler(tauri::generate_handler![
            commands::pdf::open_pdf,
            commands::pdf::read_pdf,
            commands::pdf::assemble,
            commands::redact::redact_scan,
            commands::redact::redact_search,
            commands::redact::redact_apply,
            commands::redact::redact_ai_scan,
            commands::model::model_status,
            commands::model::model_download,
            commands::model::model_download_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PaperKit");
}
