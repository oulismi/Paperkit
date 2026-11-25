//! Commands for the Redact screen: scan a PDF for sensitive data, search for
//! text, and apply approved redactions to a new file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pdf_core::redact::{self, Area, Ocr, PageSize, PageText, RedactOptions, RedactReport};
use redact_ai::ner::Target;
use redact_ai::{Category, Finding, Source};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::model::{self, ModelState};
use super::pdf::{blocking, ensure_pdf};

/// Tesseract languages for scanned pages, in the order tried. Only those whose
/// data file is bundled are used.
const OCR_LANGUAGES: [&str; 4] = ["eng", "fra", "nld", "ell"];

/// One occurrence of sensitive text on a page, with the boxes that cover it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    page: u32,
    text: String,
    label: String,
    category: Category,
    source: Source,
    confidence: Option<f32>,
    areas: Vec<Area>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pages: Vec<PageSize>,
    suggestions: Vec<Suggestion>,
    /// Pages whose text was read with OCR (no text layer: probably scanned).
    pages_ocr: Vec<u32>,
    /// Pages with no text at all, even after OCR.
    pages_without_text: Vec<u32>,
}

/// The last document's page text. OCR of a scanned document takes seconds per
/// page, so the scan, the AI pass and every search share one reading.
#[derive(Default)]
pub struct TextCache(Mutex<Option<(PathBuf, Option<SystemTime>, Arc<Vec<PageText>>)>>);

/// The bundled OCR data, if present.
pub(super) fn ocr_config(app: &AppHandle) -> Option<Ocr> {
    let datadir = app.path().resource_dir().ok()?.join("tessdata");
    let languages: Vec<&str> = OCR_LANGUAGES.iter().copied().filter(|l| datadir.join(format!("{l}.traineddata")).exists()).collect();
    (!languages.is_empty()).then(|| Ocr { datadir, languages: languages.join("+") })
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Page text for `path`, from the cache when the file hasn't changed.
fn texts(app: &AppHandle, path: &Path) -> Result<Arc<Vec<PageText>>, String> {
    let cache = app.state::<TextCache>();
    let stamp = modified(path);
    if let Some((p, t, pages)) = cache.0.lock().map_err(|e| e.to_string())?.as_ref() {
        if p == path && *t == stamp {
            return Ok(pages.clone());
        }
    }
    let pages = Arc::new(redact::page_texts(path, ocr_config(app).as_ref()).map_err(|e| e.to_string())?);
    *cache.0.lock().map_err(|e| e.to_string())? = Some((path.to_owned(), stamp, pages.clone()));
    Ok(pages)
}

fn to_suggestions(page: &PageText, findings: Vec<Finding>) -> impl Iterator<Item = Suggestion> + '_ {
    findings.into_iter().filter_map(move |f| {
        let areas = page.areas(f.start, f.end);
        // Text with no glyph boxes (e.g. a match made only of line breaks) can't be redacted.
        (!areas.is_empty()).then(|| Suggestion {
            page: page.size.page,
            text: page.text[f.start..f.end].split_whitespace().collect::<Vec<_>>().join(" "),
            label: f.label,
            category: f.category,
            source: f.source,
            confidence: f.confidence,
            areas,
        })
    })
}

/// Reads every page (with OCR where there is no text layer) and runs the detection rules.
#[tauri::command]
pub async fn redact_scan(app: AppHandle, path: PathBuf) -> Result<ScanResult, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let pages = texts(&app, &path)?;
        let pages_ocr = pages.iter().filter(|p| p.ocr).map(|p| p.size.page).collect();
        let pages_without_text = pages.iter().filter(|p| p.text.trim().is_empty()).map(|p| p.size.page).collect();
        let suggestions = pages.iter().flat_map(|p| to_suggestions(p, redact_ai::detect(&p.text))).collect();
        Ok(ScanResult { pages: pages.iter().map(|p| p.size.clone()).collect(), suggestions, pages_ocr, pages_without_text })
    })
    .await
}

/// Runs the AI model over every page. `defaults` adds people and addresses;
/// `extra` are the user's own descriptions ("patient name"). Text a format
/// rule already matched is left to the rule.
#[tauri::command]
pub async fn redact_ai_scan(app: AppHandle, path: PathBuf, defaults: bool, extra: Vec<String>) -> Result<Vec<Suggestion>, String> {
    ensure_pdf(&path)?;
    let mut targets = if defaults { Target::defaults() } else { Vec::new() };
    targets.extend(extra.iter().filter(|t| !t.trim().is_empty()).map(|t| Target::custom(t)));
    blocking(move || {
        let model = model::load(&app, &app.state::<ModelState>())?;
        let pages = texts(&app, &path)?;
        let mut out = Vec::new();
        for page in pages.iter() {
            let rules = redact_ai::detect(&page.text);
            let ai: Vec<Finding> = model
                .detect(&page.text, &targets)?
                .into_iter()
                .filter(|f| !rules.iter().any(|r| f.start < r.end && r.start < f.end))
                .collect();
            out.extend(to_suggestions(page, ai));
        }
        Ok(out)
    })
    .await
}

/// Every occurrence of `query` (ignoring case) on every page.
#[tauri::command]
pub async fn redact_search(app: AppHandle, path: PathBuf, query: String) -> Result<Vec<Suggestion>, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let pages = texts(&app, &path)?;
        Ok(pages.iter().flat_map(|p| to_suggestions(p, redact_ai::search(&p.text, &query))).collect())
    })
    .await
}

/// Writes a redacted copy of `path` to `output` and verifies it.
#[tauri::command]
pub async fn redact_apply(app: AppHandle, path: PathBuf, output: PathBuf, areas: Vec<Area>, remove_metadata: bool) -> Result<RedactReport, String> {
    ensure_pdf(&path)?;
    ensure_pdf(&output)?;
    let ocr = ocr_config(&app);
    blocking(move || redact::redact_file(&path, &output, &areas, RedactOptions { remove_metadata, ocr }).map_err(|e| e.to_string())).await
}
