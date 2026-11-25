//! Commands for the Edit screen.

use std::collections::HashMap;
use std::path::PathBuf;

use pdf_core::edit::{self, EditOp, FormField};
use pdf_core::redact::{self as rd, PageSize};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};

use super::pdf::{blocking, ensure_pdf};

const FONT: &str = "fonts/NotoSans-Regular.ttf";
const IMAGE_EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "tif", "tiff", "bmp", "gif"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditInfo {
    pages: Vec<PageSize>,
    fields: Vec<FormField>,
}

fn font_path(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path().resource_dir().map_err(|e| e.to_string())?.join(FONT);
    if path.exists() {
        Ok(path)
    } else {
        Err("The text font is missing from this installation.".into())
    }
}

/// Page sizes and form fields.
#[tauri::command]
pub async fn edit_info(path: PathBuf) -> Result<EditInfo, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let pages = rd::page_sizes(&path).map_err(|e| e.to_string())?;
        let fields = edit::form_fields(&path).map_err(|e| e.to_string())?;
        Ok(EditInfo { pages, fields })
    })
    .await
}

/// Writes an edited copy of `path` to `output`.
#[tauri::command]
pub async fn edit_apply(app: AppHandle, path: PathBuf, output: PathBuf, ops: Vec<EditOp>, fields: HashMap<String, String>) -> Result<(), String> {
    ensure_pdf(&path)?;
    ensure_pdf(&output)?;
    let font = font_path(&app);
    blocking(move || {
        let needs_font = ops.iter().any(|o| matches!(o, EditOp::Text { .. }));
        let font = if needs_font { font? } else { PathBuf::new() };
        edit::edit_file(&path, &output, &ops, &fields, &font).map_err(|e| e.to_string())
    })
    .await
}

/// The bytes of an image file, to preview it before it is placed.
#[tauri::command]
pub async fn read_image(path: PathBuf) -> Result<Response, String> {
    let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()));
    if !ok {
        return Err(format!("{} is not a supported image", path.display()));
    }
    blocking(move || std::fs::read(&path).map(Response::new).map_err(|e| e.to_string())).await
}

/// The font added text is written in, so the editor previews it exactly.
#[tauri::command]
pub async fn edit_font(app: AppHandle) -> Result<Response, String> {
    let path = font_path(&app)?;
    blocking(move || std::fs::read(&path).map(Response::new).map_err(|e| e.to_string())).await
}
