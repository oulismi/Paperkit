mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::pdf::open_pdf,
            commands::pdf::read_pdf,
            commands::pdf::assemble,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PaperKit");
}
