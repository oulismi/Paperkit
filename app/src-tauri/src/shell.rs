//! Everything around the windows: the floating drop bubble, the tray icon,
//! start-with-the-system, and handing a dropped file to the main window.
//!
//! - Closing the main window hides it; PaperKit keeps running in the tray.
//! - Started with `--background` (autostart), only the tray and the bubble appear.
//! - The bubble remembers where it was put; settings live in `settings.json`
//!   in the app's config folder.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

pub const BUBBLE: &str = "bubble";
pub const MAIN: &str = "main";
/// Size of the bubble window when idle, in logical pixels.
const BUBBLE_SIZE: f64 = 76.0;
/// Argument the autostart entry launches PaperKit with.
pub const BACKGROUND_ARG: &str = "--background";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub bubble_visible: bool,
    /// Logical screen position of the bubble's top-left corner.
    pub bubble_x: Option<f64>,
    pub bubble_y: Option<f64>,
    /// Whether the "PaperKit is still running in the tray" note was shown.
    pub close_hint_shown: bool,
    /// The user said "No thanks" to the first-launch AI model offer.
    pub ai_offer_declined: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { bubble_visible: true, bubble_x: None, bubble_y: None, close_hint_shown: false, ai_offer_declined: false }
    }
}

/// A file (or files) dropped on the bubble, for the main window to open.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    /// "split", "merge", "redact", "edit", "convert".
    pub action: String,
    pub paths: Vec<String>,
}

#[derive(Default)]
pub struct ShellState {
    settings: Mutex<Settings>,
    pending: Mutex<Option<OpenRequest>>,
    /// The Windows installer asked for the AI model to be downloaded (read once).
    install_requested_ai: Mutex<Option<bool>>,
    bubble_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_settings<R: Runtime>(app: &AppHandle<R>) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(path) = settings_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(settings) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn update_settings(app: &AppHandle, f: impl FnOnce(&mut Settings)) {
    let state = app.state::<ShellState>();
    let mut settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut settings);
    save_settings(app, &settings);
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn set_bubble_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window(BUBBLE) {
        let _ = if visible { w.show() } else { w.hide() };
    }
    update_settings(app, |s| s.bubble_visible = visible);
    if let Some(item) = app.state::<ShellState>().bubble_item.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = item.set_checked(visible);
    }
}

/// Where the bubble goes: its saved spot if that is still on a screen, else
/// the bottom-right corner of the primary screen's work area.
fn bubble_position(app: &AppHandle, settings: &Settings) -> (f64, f64) {
    let monitors = app.available_monitors().unwrap_or_default();
    if let (Some(x), Some(y)) = (settings.bubble_x, settings.bubble_y) {
        let on_screen = monitors.iter().any(|m| {
            let scale = m.scale_factor();
            let pos = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            x >= pos.x - 10.0 && y >= pos.y - 10.0 && x + BUBBLE_SIZE <= pos.x + size.width + 10.0 && y + BUBBLE_SIZE <= pos.y + size.height + 10.0
        });
        if on_screen {
            return (x, y);
        }
    }
    match app.primary_monitor().ok().flatten() {
        Some(m) => {
            let scale = m.scale_factor();
            let area = m.work_area();
            let pos = area.position.to_logical::<f64>(scale);
            let size = area.size.to_logical::<f64>(scale);
            (pos.x + size.width - BUBBLE_SIZE - 24.0, pos.y + size.height - BUBBLE_SIZE - 24.0)
        }
        None => (100.0, 100.0),
    }
}

fn create_bubble(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    let (x, y) = bubble_position(app, settings);
    WebviewWindowBuilder::new(app, BUBBLE, WebviewUrl::App("bubble.html".into()))
        .title("PaperKit drop target")
        .inner_size(BUBBLE_SIZE, BUBBLE_SIZE)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(false)
        .visible(settings.bubble_visible)
        .build()?;
    Ok(())
}

fn create_tray(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open PaperKit", true, None::<&str>)?;
    let bubble = CheckMenuItem::with_id(app, "bubble", "Show drop bubble", true, settings.bubble_visible, None::<&str>)?;
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let login = CheckMenuItem::with_id(app, "autostart", "Start with the system", true, autostart, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Check for updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit PaperKit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &bubble, &login, &separator, &updates, &separator2, &quit])?;
    *app.state::<ShellState>().bubble_item.lock().unwrap_or_else(|e| e.into_inner()) = Some(bubble.clone());

    let mut tray = TrayIconBuilder::with_id("paperkit")
        .tooltip("PaperKit — drop PDFs on the bubble")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "bubble" => {
                let visible = app.get_webview_window(BUBBLE).and_then(|w| w.is_visible().ok()).unwrap_or(false);
                set_bubble_visible(app, !visible);
            }
            "autostart" => {
                let manager = app.autolaunch();
                let enabled = manager.is_enabled().unwrap_or(false);
                let _ = if enabled { manager.disable() } else { manager.enable() };
                let _ = login.set_checked(manager.is_enabled().unwrap_or(!enabled));
            }
            "updates" => {
                show_main(app);
                let _ = app.emit_to(MAIN, "paperkit://check-updates", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Demo mode, for screenshots: `--demo=<screen>` opens one screen with the
/// sample files in `--demo-files=<folder>`. No bubble, no update check, no
/// first-launch offers, and it runs beside a normal PaperKit.
#[derive(Debug, Clone, Serialize)]
pub struct Demo {
    pub screen: String,
    pub files: String,
}

pub fn demo() -> Option<Demo> {
    let arg = |name: &str| std::env::args().find_map(|a| a.strip_prefix(&format!("--{name}=")).map(str::to_owned));
    Some(Demo { screen: arg("demo")?, files: arg("demo-files").unwrap_or_default() })
}

#[tauri::command]
pub fn demo_request() -> Option<Demo> {
    demo()
}

/// Called from `setup`: tray, bubble, and the main window unless started in the background.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let settings = load_settings(app);
    *app.state::<ShellState>().settings.lock().unwrap_or_else(|e| e.into_inner()) = settings.clone();
    if demo().is_some() {
        show_main(app);
        return Ok(());
    }
    create_tray(app, &settings)?;
    create_bubble(app, &settings)?;
    if !std::env::args().any(|a| a == BACKGROUND_ARG) {
        show_main(app);
    }
    Ok(())
}

/// Closing the main window hides it instead; the first time, say where PaperKit went.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
        let app = window.app_handle().clone();
        let first = !app.state::<ShellState>().settings.lock().map(|s| s.close_hint_shown).unwrap_or(true);
        if first {
            update_settings(&app, |s| s.close_hint_shown = true);
            app.dialog()
                .message("PaperKit keeps running in the tray, so the drop bubble stays available.\n\nTo quit, use “Quit PaperKit” in the tray icon's menu.")
                .title("PaperKit is still running")
                .kind(MessageDialogKind::Info)
                .show(|_| {});
        }
    }
}

/// File the Windows installer leaves next to the app when the user answers
/// "Yes" to downloading the AI model (see app/src-tauri/windows/hooks.nsh).
const AI_REQUEST_FLAG: &str = "download-ai-model.flag";

/// Reads (and removes) the installer's request, once per run.
fn take_install_request(state: &ShellState) -> bool {
    let mut slot = state.install_requested_ai.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(value) = *slot {
        return value;
    }
    let flag = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join(AI_REQUEST_FLAG)));
    let requested = flag.as_ref().is_some_and(|f| f.exists());
    if let Some(f) = flag.filter(|_| requested) {
        let _ = std::fs::remove_file(f);
    }
    *slot = Some(requested);
    requested
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOffer {
    /// Show the first-launch card offering the model.
    show: bool,
    /// The installer already said yes: start the download right away.
    start_download: bool,
}

/// What the main window should do about the AI model on launch.
#[tauri::command]
pub fn ai_offer(app: AppHandle, state: tauri::State<'_, ShellState>) -> AiOffer {
    if demo().is_some() {
        return AiOffer { show: false, start_download: false };
    }
    let requested = take_install_request(&state);
    if crate::commands::model::installed(&app) {
        return AiOffer { show: false, start_download: false };
    }
    let declined = state.settings.lock().map(|s| s.ai_offer_declined).unwrap_or(false);
    AiOffer { show: requested || !declined, start_download: requested }
}

/// "No thanks": don't offer the model on launch again (Redact still does).
#[tauri::command]
pub fn ai_offer_decline(app: AppHandle) {
    update_settings(&app, |s| s.ai_offer_declined = true);
}

// ---- Commands ----

/// From the bubble: open the main window on a tool with these files.
#[tauri::command]
pub fn open_in_main(app: AppHandle, state: tauri::State<'_, ShellState>, request: OpenRequest) {
    *state.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(request.clone());
    show_main(&app);
    let _ = app.emit_to(MAIN, "paperkit://open", ());
}

/// For the main window: the request waiting for it, if any (taken once).
#[tauri::command]
pub fn take_open_request(state: tauri::State<'_, ShellState>) -> Option<OpenRequest> {
    state.pending.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// The bubble was moved (logical screen coordinates).
#[tauri::command]
pub fn bubble_moved(app: AppHandle, x: f64, y: f64) {
    update_settings(&app, |s| {
        s.bubble_x = Some(x);
        s.bubble_y = Some(y);
    });
}

#[tauri::command]
pub fn bubble_hide(app: AppHandle) {
    set_bubble_visible(&app, false);
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    show_main(&app);
}
