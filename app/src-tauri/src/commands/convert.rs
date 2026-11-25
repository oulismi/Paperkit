//! Commands for the Convert screen.

use std::path::PathBuf;

use pdf_core::convert::{self, ImageFormat, PageFit};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::AppHandle;

use super::pdf::{blocking, ensure_pdf};
use super::redact::ocr_config;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Docx,
    Png,
    Jpg,
    Txt,
    Searchable,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    done: u32,
    total: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResult {
    /// Files written.
    written: Vec<PathBuf>,
    /// When not empty, nothing was written: these files exist and `overwrite` was false.
    existing: Vec<PathBuf>,
    /// Pages read with OCR (searchable PDF only).
    ocr_pages: Vec<u32>,
}

impl ConvertResult {
    fn written(written: Vec<PathBuf>) -> Self {
        Self { written, existing: Vec::new(), ocr_pages: Vec::new() }
    }
}

/// Converts `path`. `output` is the file to write, or for images the folder to
/// write `<stem>_p<N>.png|jpg` into. `pages` = None means all pages.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn convert_pdf(
    app: AppHandle,
    path: PathBuf,
    target: Target,
    output: PathBuf,
    pages: Option<Vec<u32>>,
    dpi: Option<u32>,
    use_ocr: bool,
    overwrite: bool,
    progress: Channel<Progress>,
) -> Result<ConvertResult, String> {
    ensure_pdf(&path)?;
    let ocr = if use_ocr || matches!(target, Target::Searchable) { ocr_config(&app) } else { None };
    if matches!(target, Target::Searchable) && ocr.is_none() {
        return Err("OCR language data is missing from this installation.".into());
    }
    blocking(move || {
        let count = pdf_core::page_count(&path).map_err(|e| e.to_string())?;
        let pages = pages.unwrap_or_else(|| (1..=count).collect());
        let report = |done, total| {
            let _ = progress.send(Progress { done, total });
        };
        let err = |e: pdf_core::Error| e.to_string();
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "page".into());
        match target {
            Target::Png | Target::Jpg => {
                let format = if matches!(target, Target::Png) { ImageFormat::Png } else { ImageFormat::Jpeg };
                if !overwrite {
                    let existing: Vec<PathBuf> =
                        convert::image_names(&stem, &pages, count, format).into_iter().map(|n| output.join(n)).filter(|p| p.exists()).collect();
                    if !existing.is_empty() {
                        return Ok(ConvertResult { written: Vec::new(), existing, ocr_pages: Vec::new() });
                    }
                }
                let written = convert::to_images(&path, &pages, &output, &stem, format, dpi.unwrap_or(150), &report).map_err(err)?;
                Ok(ConvertResult::written(written))
            }
            // For single files the save dialog has already asked about replacing.
            Target::Txt => {
                convert::to_text(&path, &pages, &output, ocr.as_ref(), &report).map_err(err)?;
                Ok(ConvertResult::written(vec![output]))
            }
            Target::Docx => {
                convert::to_docx(&path, &pages, &output, ocr.as_ref(), &report).map_err(err)?;
                Ok(ConvertResult::written(vec![output]))
            }
            Target::Searchable => {
                let ocr_pages = convert::searchable_pdf(&path, &output, ocr.as_ref().expect("checked above"), &report).map_err(err)?;
                let written = if ocr_pages.is_empty() { Vec::new() } else { vec![output] };
                Ok(ConvertResult { written, existing: Vec::new(), ocr_pages })
            }
        }
    })
    .await
}

/// One PDF page per image, in order.
#[tauri::command]
pub async fn images_to_pdf(images: Vec<PathBuf>, output: PathBuf, fit: PageFit, progress: Channel<Progress>) -> Result<(), String> {
    ensure_pdf(&output)?;
    blocking(move || {
        let report = |done, total| {
            let _ = progress.send(Progress { done, total });
        };
        convert::images_to_pdf(&images, &output, fit, &report).map_err(|e| e.to_string())
    })
    .await
}
