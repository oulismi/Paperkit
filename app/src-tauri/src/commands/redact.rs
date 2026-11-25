//! Commands for the Redact screen: scan a PDF for sensitive data, search for
//! text, and apply approved redactions to a new file.

use std::path::PathBuf;

use pdf_core::redact::{self, Area, PageSize, PageText, RedactOptions, RedactReport};
use redact_ai::ner::Target;
use redact_ai::{Category, Finding, Source};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::model::{self, ModelState};
use super::pdf::{blocking, ensure_pdf};

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
    /// Pages with no extractable text: probably scanned images, which need OCR.
    pages_without_text: Vec<u32>,
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

/// Reads every page and runs the detection rules.
#[tauri::command]
pub async fn redact_scan(path: PathBuf) -> Result<ScanResult, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let pages = redact::page_texts(&path).map_err(|e| e.to_string())?;
        let pages_without_text = pages.iter().filter(|p| p.text.trim().is_empty()).map(|p| p.size.page).collect();
        let suggestions = pages.iter().flat_map(|p| to_suggestions(p, redact_ai::detect(&p.text))).collect();
        Ok(ScanResult { pages: pages.iter().map(|p| p.size.clone()).collect(), suggestions, pages_without_text })
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
        let pages = redact::page_texts(&path).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for page in &pages {
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
pub async fn redact_search(path: PathBuf, query: String) -> Result<Vec<Suggestion>, String> {
    ensure_pdf(&path)?;
    blocking(move || {
        let pages = redact::page_texts(&path).map_err(|e| e.to_string())?;
        Ok(pages.iter().flat_map(|p| to_suggestions(p, redact_ai::search(&p.text, &query))).collect())
    })
    .await
}

/// Writes a redacted copy of `path` to `output` and verifies it.
#[tauri::command]
pub async fn redact_apply(path: PathBuf, output: PathBuf, areas: Vec<Area>, remove_metadata: bool) -> Result<RedactReport, String> {
    ensure_pdf(&path)?;
    ensure_pdf(&output)?;
    blocking(move || {
        redact::redact_file(&path, &output, &areas, RedactOptions { remove_metadata }).map_err(|e| e.to_string())
    })
    .await
}
