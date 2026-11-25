//! Redaction with MuPDF: text with positions for finding what to redact, and
//! applying redactions so the content under each box is really removed.
//!
//! The rules from docs/ARCHITECTURE.md hold here:
//! - the original file is never modified; the result is a new file;
//! - text, image pixels and covered line art under each box are removed, not
//!   just painted over, and document metadata is dropped by default;
//! - the saved file is opened again and checked: if any character is still
//!   under a box, the output is deleted and an error is returned.

use std::fs;
use std::path::{Path, PathBuf};

use mupdf::pdf::{PdfDocument, PdfPage, PdfRedactImageMethod, PdfRedactLineArtMethod, PdfRedactOptions, PdfRedactTextMethod, PdfWriteOptions};
use mupdf::text_page::TextBlockType;
use mupdf::{Document, Rect, TextPageFlags};
use serde::{Deserialize, Serialize};

use crate::{same_file, Error, Result};

/// A box on a page, in MuPDF page space: points, origin at the top-left of the
/// visible page, y growing downwards, page rotation already applied. That is the
/// same space pdf.js uses for its viewport, so the UI can draw boxes directly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Area {
    /// 1-based page number.
    pub page: u32,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Area {
    fn rect(&self) -> Rect {
        Rect { x0: self.x0, y0: self.y0, x1: self.x1, y1: self.y1 }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PageSize {
    pub page: u32,
    pub width: f32,
    pub height: f32,
}

/// The text of one page, with the position of every character.
pub struct PageText {
    pub size: PageSize,
    /// Lines separated by `\n`.
    pub text: String,
    /// (byte offset in `text`, line number, glyph box) for every character that has one.
    chars: Vec<(usize, usize, Rect)>,
}

impl PageText {
    /// The boxes covering `text[start..end]`: one per line the span touches.
    pub fn areas(&self, start: usize, end: usize) -> Vec<Area> {
        let mut out: Vec<(usize, Area)> = Vec::new();
        for &(_, line, r) in self.chars.iter().filter(|(o, _, _)| (start..end).contains(o)) {
            match out.last_mut() {
                Some((l, a)) if *l == line => {
                    a.x0 = a.x0.min(r.x0);
                    a.y0 = a.y0.min(r.y0);
                    a.x1 = a.x1.max(r.x1);
                    a.y1 = a.y1.max(r.y1);
                }
                _ => out.push((line, Area { page: self.size.page, x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1 })),
            }
        }
        out.into_iter().map(|(_, a)| a).collect()
    }
}

fn mu(e: mupdf::Error) -> Error {
    Error::Mupdf(e.to_string())
}

fn open_pdf(path: &Path) -> Result<PdfDocument> {
    let s = path.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", path.display())))?;
    let doc = PdfDocument::open(s).map_err(|e| Error::Mupdf(format!("could not open {}: {e}", path.display())))?;
    if doc.needs_password().map_err(mu)? {
        return Err(Error::Encrypted(path.to_owned()));
    }
    Ok(doc)
}

fn read_page_text(doc: &Document, index: i32) -> Result<PageText> {
    let page = doc.load_page(index).map_err(mu)?;
    let bounds = page.bounds().map_err(mu)?;
    let size = PageSize { page: index as u32 + 1, width: bounds.x1 - bounds.x0, height: bounds.y1 - bounds.y0 };
    let stext = page.to_text_page(TextPageFlags::empty()).map_err(mu)?;
    let mut text = String::new();
    let mut chars = Vec::new();
    let mut line_no = 0;
    for block in stext.blocks().filter(|b| b.r#type() == TextBlockType::Text) {
        for line in block.lines() {
            for ch in line.chars() {
                let q = ch.quad();
                let xs = [q.ul.x, q.ur.x, q.ll.x, q.lr.x];
                let ys = [q.ul.y, q.ur.y, q.ll.y, q.lr.y];
                let rect = Rect {
                    x0: xs.iter().copied().fold(f32::INFINITY, f32::min) - bounds.x0,
                    y0: ys.iter().copied().fold(f32::INFINITY, f32::min) - bounds.y0,
                    x1: xs.iter().copied().fold(f32::NEG_INFINITY, f32::max) - bounds.x0,
                    y1: ys.iter().copied().fold(f32::NEG_INFINITY, f32::max) - bounds.y0,
                };
                chars.push((text.len(), line_no, rect));
                text.push(ch.char().unwrap_or('\u{FFFD}'));
            }
            text.push('\n');
            line_no += 1;
        }
    }
    Ok(PageText { size, text, chars })
}

/// The text of every page, with character positions.
pub fn page_texts(path: &Path) -> Result<Vec<PageText>> {
    let doc = open_pdf(path)?;
    (0..doc.page_count().map_err(mu)?).map(|i| read_page_text(&doc, i)).collect()
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactOptions {
    /// Drop the document info dictionary and XMP metadata (author, title, tools…).
    pub remove_metadata: bool,
}

impl Default for RedactOptions {
    fn default() -> Self {
        Self { remove_metadata: true }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactReport {
    pub areas: usize,
    pub pages: usize,
    /// Characters found under the boxes when the saved file was read back: always 0
    /// in a returned report, since anything else is an error.
    pub verified_chars_left: usize,
}

/// Writes a copy of `src` to `out` with everything under `areas` removed, then
/// verifies the copy.
pub fn redact_file(src: &Path, out: &Path, areas: &[Area], options: RedactOptions) -> Result<RedactReport> {
    if areas.is_empty() {
        return Err(Error::NothingToRedact);
    }
    if same_file(src, out) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }

    let doc = open_pdf(src)?;
    let page_count = doc.page_count().map_err(mu)? as u32;
    let mut pages: Vec<u32> = areas.iter().map(|a| a.page).collect();
    pages.sort_unstable();
    pages.dedup();

    let apply = PdfRedactOptions {
        black_boxes: true,
        image_method: PdfRedactImageMethod::Pixels,
        line_art: PdfRedactLineArtMethod::RemoveIfCovered,
        text: PdfRedactTextMethod::Remove,
    };
    for &n in &pages {
        if n == 0 || n > page_count {
            return Err(Error::PageOutOfRange { path: src.to_owned(), page: n, count: page_count });
        }
        let mut page = PdfPage::try_from(doc.load_page(n as i32 - 1).map_err(mu)?).map_err(mu)?;
        let bounds = page.bounds().map_err(mu)?;
        for a in areas.iter().filter(|a| a.page == n) {
            let r = a.rect();
            let rect = Rect { x0: r.x0 + bounds.x0, y0: r.y0 + bounds.y0, x1: r.x1 + bounds.x0, y1: r.y1 + bounds.y0 };
            page.add_redact_annotation(rect).map_err(mu)?;
        }
        page.apply_redactions_with_options(apply).map_err(mu)?;
    }

    if options.remove_metadata {
        doc.trailer().map_err(mu)?.dict_delete("Info").map_err(mu)?;
        doc.catalog().map_err(mu)?.dict_delete("Metadata").map_err(mu)?;
    }

    // Full rewrite with garbage collection, so objects the redaction orphaned
    // (old content streams, replaced images) don't stay in the file.
    let tmp = out.with_extension("pdf.partial");
    let tmp_str = tmp.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", tmp.display())))?;
    let mut write = PdfWriteOptions::default();
    write.set_incremental(false).set_garbage_level(4).set_clean(true).set_compress(true);
    doc.save_with_options(tmp_str, write).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        mu(e)
    })?;
    drop(doc);

    let left = match chars_under(&tmp, areas) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if left > 0 {
        let _ = fs::remove_file(&tmp);
        return Err(Error::RedactionIncomplete(left));
    }
    fs::rename(&tmp, out).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        Error::Write { path: out.to_owned(), source }
    })?;
    Ok(RedactReport { areas: areas.len(), pages: pages.len(), verified_chars_left: 0 })
}

/// Counts characters whose centre lies inside one of `areas` (read from the saved file).
fn chars_under(path: &PathBuf, areas: &[Area]) -> Result<usize> {
    let doc = open_pdf(path)?;
    let mut count = 0;
    let mut pages: Vec<u32> = areas.iter().map(|a| a.page).collect();
    pages.dedup();
    for n in pages {
        let text = read_page_text(&doc, n as i32 - 1)?;
        let boxes: Vec<&Area> = areas.iter().filter(|a| a.page == n).collect();
        count += text
            .chars
            .iter()
            .filter(|(_, _, r)| {
                let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
                boxes.iter().any(|a| a.contains(cx, cy))
            })
            .count();
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{temp_dir, write_text_pdf};

    #[test]
    fn finds_text_positions() {
        let dir = temp_dir("text");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["Name: Jane Roe", "IBAN GB29 NWBK 6016 1331 9268 19"]);
        let pages = page_texts(&src).unwrap();
        assert_eq!(pages.len(), 1);
        let p = &pages[0];
        let start = p.text.find("Jane Roe").expect("text is extracted");
        let areas = p.areas(start, start + "Jane Roe".len());
        assert_eq!(areas.len(), 1);
        let a = areas[0];
        assert!(a.x1 > a.x0 && a.y1 > a.y0);
        assert!(a.y0 < 200.0, "line 1 is near the top of the page (y grows downwards)");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removes_text_under_boxes_and_keeps_the_rest() {
        let dir = temp_dir("apply");
        let src = dir.join("in.pdf");
        let out = dir.join("out.pdf");
        write_text_pdf(&src, &["Name: Jane Roe", "Keep this line"]);
        let before = &page_texts(&src).unwrap()[0];
        let start = before.text.find("Jane Roe").unwrap();
        let areas = before.areas(start, start + 8);

        let report = redact_file(&src, &out, &areas, RedactOptions::default()).unwrap();
        assert_eq!(report.areas, 1);

        let after = &page_texts(&out).unwrap()[0];
        assert!(!after.text.contains("Jane"), "redacted text is gone: {:?}", after.text);
        assert!(after.text.contains("Keep this line"));
        assert!(after.text.contains("Name:"));
        assert!(page_texts(&src).unwrap()[0].text.contains("Jane Roe"), "original untouched");
        assert!(!out.with_extension("pdf.partial").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removes_metadata_by_default() {
        let dir = temp_dir("meta");
        let src = dir.join("in.pdf");
        let out = dir.join("out.pdf");
        write_text_pdf(&src, &["Secret"]);
        let area = page_texts(&src).unwrap()[0].areas(0, 6);
        redact_file(&src, &out, &area, RedactOptions::default()).unwrap();
        let bytes = fs::read(&out).unwrap();
        let has = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        assert!(has("%PDF"));
        assert!(!has("Test Author"), "info dictionary was dropped");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_unsafe_requests() {
        let dir = temp_dir("refuse");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["Secret"]);
        let area = Area { page: 1, x0: 0.0, y0: 0.0, x1: 10.0, y1: 10.0 };
        assert!(matches!(redact_file(&src, &src, &[area], RedactOptions::default()), Err(Error::OutputIsSource(_))));
        assert!(matches!(redact_file(&src, &dir.join("o.pdf"), &[], RedactOptions::default()), Err(Error::NothingToRedact)));
        let far = Area { page: 9, ..area };
        assert!(matches!(
            redact_file(&src, &dir.join("o.pdf"), &[far], RedactOptions::default()),
            Err(Error::PageOutOfRange { .. })
        ));
        fs::remove_dir_all(dir).unwrap();
    }
}
