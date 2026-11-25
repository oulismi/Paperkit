mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::pdf::open_pdf,
            commands::pdf::read_pdf,
            commands::pdf::assemble,
            commands::redact::redact_scan,
            commands::redact::redact_search,
            commands::redact::redact_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PaperKit");
}
