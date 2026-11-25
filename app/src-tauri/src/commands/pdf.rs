//! Commands the UI calls for page operations. They are thin wrappers around
//! `pdf-core`; errors reach the UI as readable strings.

use std::path::{Path, PathBuf};

use pdf_core::OutputSpec;
use serde::Serialize;
use tauri::ipc::Response;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    path: PathBuf,
    name: String,
    page_count: u32,
}

pub(super) fn ensure_pdf(path: &Path) -> Result<(), String> {
    let is_pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if is_pdf {
        Ok(())
    } else {
        Err(format!("{} is not a PDF file", path.display()))
    }
}

/// Runs blocking PDF work off the main thread so the window stays responsive.
pub(super) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Checks that a file is a PDF we can work with and returns its page count.
#[tauri::command]
pub async fn open_pdf(path: PathBuf) -> Result<PdfInfo, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let page_count = pdf_core::page_count(&path).map_err(|e| e.to_string())?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(PdfInfo { path, name, page_count })
    })
    .await
}

/// Returns the raw bytes of a PDF so the UI can render page previews.
#[tauri::command]
pub async fn read_pdf(path: PathBuf) -> Result<Response, String> {
    ensure_pdf(&path)?;
    let bytes = blocking(move || std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))).await?;
    Ok(Response::new(bytes))
}

/// Builds one or more PDFs from pages of the source files.
///
/// Unless `overwrite` is set, nothing is written when an output file already
/// exists: the existing paths are returned so the UI can ask the user first.
/// An empty list means every output was written.
#[tauri::command]
pub async fn assemble(sources: Vec<PathBuf>, outputs: Vec<OutputSpec>, overwrite: bool) -> Result<Vec<PathBuf>, String> {
    for path in sources.iter().chain(outputs.iter().map(|o| &o.path)) {
        ensure_pdf(path)?;
    }
    if !overwrite {
        let existing: Vec<PathBuf> = outputs.iter().map(|o| o.path.clone()).filter(|p| p.exists()).collect();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    blocking(move || pdf_core::assemble_files(&sources, &outputs).map_err(|e| e.to_string()))
        .await
        .map(|()| Vec::new())
}
