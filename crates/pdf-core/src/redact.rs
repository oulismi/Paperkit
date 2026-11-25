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
use mupdf::{Document, DocumentWriter, Matrix, Rect, TextPageFlags};
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
    /// The page had no text layer and this text was read by OCR.
    pub ocr: bool,
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

pub(crate) fn mu(e: mupdf::Error) -> Error {
    Error::Mupdf(e.to_string())
}

pub(crate) fn open_pdf(path: &Path) -> Result<PdfDocument> {
    let s = path.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", path.display())))?;
    let doc = PdfDocument::open(s).map_err(|e| Error::Mupdf(format!("could not open {}: {e}", path.display())))?;
    if doc.needs_password().map_err(mu)? {
        return Err(Error::Encrypted(path.to_owned()));
    }
    Ok(doc)
}

pub(crate) fn read_page_text(doc: &Document, index: i32) -> Result<PageText> {
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
    Ok(PageText { size, text, chars, ocr: false })
}

/// Settings for reading scanned pages with Tesseract (built into MuPDF).
#[derive(Debug, Clone)]
pub struct Ocr {
    /// Folder holding `<lang>.traineddata` files.
    pub datadir: PathBuf,
    /// Tesseract language codes joined by `+`, e.g. `eng+fra+nld+ell`.
    pub languages: String,
}

/// Rendering resolution for OCR, in dots per inch.
const OCR_DPI: u32 = 300;
/// OCR gives tight boxes around the ink; widen them a little (points) so a
/// redaction also covers anti-aliased edges and the parts of letters OCR trims.
const OCR_PAD: f32 = 1.5;

/// Reads a page's text layer; if it has none and `ocr` is given, reads the
/// page image with OCR instead. Positions are in the same page space either way.
pub(crate) fn page_text(doc: &Document, index: i32, ocr: Option<&Ocr>) -> Result<PageText> {
    let text = read_page_text(doc, index)?;
    match ocr {
        Some(ocr) if text.text.trim().is_empty() => ocr_page(doc, index, ocr),
        _ => Ok(text),
    }
}

/// A temporary PDF made by MuPDF's PDF-OCR writer: each page it is given is
/// rasterised and run through Tesseract, giving an image page with an invisible
/// text layer positioned like the original. The file is deleted on drop.
///
/// MuPDF's Tesseract/Leptonica can't be used from two threads at once ("Attempt
/// to use Leptonica from 2 threads at once!"), so an `OcrPdf` holds a
/// process-wide lock for its whole life: OCR runs one document at a time.
pub(crate) struct OcrPdf {
    path: PathBuf,
    writer: Option<DocumentWriter>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

static OCR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl OcrPdf {
    pub(crate) fn new(ocr: &Ocr) -> Result<Self> {
        // A panic elsewhere while holding the lock leaves nothing to clean up.
        let lock = OCR_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("paperkit-ocr-{}-{n}.pdf", std::process::id()));
        let path_str = path.to_str().ok_or_else(|| Error::Mupdf("unsupported temp path".into()))?;
        // MuPDF's option strings are comma-separated, so a comma in the path would break them.
        let datadir = ocr
            .datadir
            .to_str()
            .filter(|d| !d.contains(','))
            .ok_or_else(|| Error::Mupdf("unsupported OCR data path".into()))?;
        let options = format!("ocr-language={},ocr-datadir={datadir},resolution={OCR_DPI},compression=flate", ocr.languages);
        let writer = DocumentWriter::with_ocr(path_str, &options).map_err(mu)?;
        Ok(Self { path, writer: Some(writer), _lock: lock })
    }

    /// OCRs page `index` of `doc` as the next page of this file.
    pub(crate) fn add(&mut self, doc: &Document, index: i32) -> Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| Error::Mupdf("OCR file already finished".into()))?;
        let page = doc.load_page(index).map_err(mu)?;
        let device = writer.begin_page(page.bounds().map_err(mu)?).map_err(mu)?;
        page.run(&device, &Matrix::IDENTITY).map_err(mu)?;
        writer.end_page(device).map_err(mu)
    }

    /// Finishes the file and returns its path (valid while `self` lives).
    pub(crate) fn finish(&mut self) -> &Path {
        self.writer = None; // dropping the writer writes the file
        &self.path
    }
}

impl Drop for OcrPdf {
    fn drop(&mut self) {
        self.writer = None;
        let _ = fs::remove_file(&self.path);
    }
}

/// Reads one page with OCR: text and character positions in page space.
fn ocr_page(doc: &Document, index: i32, ocr: &Ocr) -> Result<PageText> {
    let mut pdf = OcrPdf::new(ocr)?;
    pdf.add(doc, index)?;
    let path = pdf.finish().to_str().ok_or_else(|| Error::Mupdf("unsupported temp path".into()))?.to_owned();
    let scanned = Document::open(&path).map_err(mu)?;
    let mut text = read_page_text(&scanned, 0)?;
    text.size.page = index as u32 + 1;
    text.ocr = true;
    for (_, _, r) in &mut text.chars {
        *r = Rect { x0: r.x0 - OCR_PAD, y0: r.y0 - OCR_PAD, x1: r.x1 + OCR_PAD, y1: r.y1 + OCR_PAD };
    }
    Ok(text)
}

/// Each page's size in page space (rotation applied), without reading any text.
pub fn page_sizes(path: &Path) -> Result<Vec<PageSize>> {
    let doc = open_pdf(path)?;
    (0..doc.page_count().map_err(mu)?)
        .map(|i| {
            let b = doc.load_page(i).map_err(mu)?.bounds().map_err(mu)?;
            Ok(PageSize { page: i as u32 + 1, width: b.x1 - b.x0, height: b.y1 - b.y0 })
        })
        .collect()
}

/// The text of every page, with character positions. Pages without a text
/// layer are read with OCR when `ocr` is given.
pub fn page_texts(path: &Path, ocr: Option<&Ocr>) -> Result<Vec<PageText>> {
    let doc = open_pdf(path)?;
    (0..doc.page_count().map_err(mu)?).map(|i| page_text(&doc, i, ocr)).collect()
}

#[derive(Debug, Clone)]
pub struct RedactOptions {
    /// Drop the document info dictionary and XMP metadata (author, title, tools…).
    pub remove_metadata: bool,
    /// Also used to verify scanned pages: the output is read back with OCR.
    pub ocr: Option<Ocr>,
}

impl Default for RedactOptions {
    fn default() -> Self {
        Self { remove_metadata: true, ocr: None }
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
/// verifies it.
///
/// Two passes, so the check means something on scanned pages too:
/// 1. remove the content under the boxes *without* painting them black, save,
///    and read that copy back (with OCR for pages that have no text layer): no
///    character may remain under a box. A black box drawn on top would hide
///    leftover image pixels from OCR, and OCR reads the box itself as junk.
/// 2. paint the black boxes onto the verified copy and write the output.
pub fn redact_file(src: &Path, out: &Path, areas: &[Area], options: RedactOptions) -> Result<RedactReport> {
    if areas.is_empty() {
        return Err(Error::NothingToRedact);
    }
    if same_file(src, out) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }
    let mut pages: Vec<u32> = areas.iter().map(|a| a.page).collect();
    pages.sort_unstable();
    pages.dedup();

    let removed = out.with_extension("pdf.removed");
    let partial = out.with_extension("pdf.partial");
    let cleanup = || {
        let _ = fs::remove_file(&removed);
        let _ = fs::remove_file(&partial);
    };
    let result = (|| {
        // Pass 1: remove, no boxes.
        let doc = open_pdf(src)?;
        apply_redactions(&doc, src, areas, &pages, false)?;
        if options.remove_metadata {
            doc.trailer().map_err(mu)?.dict_delete("Info").map_err(mu)?;
            doc.catalog().map_err(mu)?.dict_delete("Metadata").map_err(mu)?;
        }
        save(&doc, &removed)?;
        drop(doc);

        let left = chars_under(&removed, areas, options.ocr.as_ref())?;
        if left > 0 {
            return Err(Error::RedactionIncomplete(left));
        }

        // Pass 2: black boxes on the verified copy (nothing is left to remove).
        let doc = open_pdf(&removed)?;
        apply_redactions(&doc, &removed, areas, &pages, true)?;
        save(&doc, &partial)?;
        drop(doc);
        fs::rename(&partial, out).map_err(|source| Error::Write { path: out.to_owned(), source })
    })();
    cleanup();
    result?;
    Ok(RedactReport { areas: areas.len(), pages: pages.len(), verified_chars_left: 0 })
}

fn apply_redactions(doc: &PdfDocument, path: &Path, areas: &[Area], pages: &[u32], black_boxes: bool) -> Result<()> {
    let page_count = doc.page_count().map_err(mu)? as u32;
    let apply = PdfRedactOptions {
        black_boxes,
        image_method: PdfRedactImageMethod::Pixels,
        line_art: PdfRedactLineArtMethod::RemoveIfCovered,
        text: PdfRedactTextMethod::Remove,
    };
    for &n in pages {
        if n == 0 || n > page_count {
            return Err(Error::PageOutOfRange { path: path.to_owned(), page: n, count: page_count });
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
    Ok(())
}

/// Full rewrite with garbage collection, so objects the redaction orphaned (old
/// content streams, replaced images) don't stay in the file.
fn save(doc: &PdfDocument, path: &Path) -> Result<()> {
    let path_str = path.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", path.display())))?;
    let mut write = PdfWriteOptions::default();
    write.set_incremental(false).set_garbage_level(4).set_clean(true).set_compress(true);
    doc.save_with_options(path_str, write).map_err(mu)
}

/// Counts characters whose centre lies inside one of `areas` (read from the
/// saved file; scanned pages are read with OCR when `ocr` is given).
fn chars_under(path: &Path, areas: &[Area], ocr: Option<&Ocr>) -> Result<usize> {
    let doc = open_pdf(path)?;
    let mut count = 0;
    let mut pages: Vec<u32> = areas.iter().map(|a| a.page).collect();
    pages.sort_unstable();
    pages.dedup();
    for n in pages {
        let text = page_text(&doc, n as i32 - 1, ocr)?;
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
        let pages = page_texts(&src, None).unwrap();
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
        let before = &page_texts(&src, None).unwrap()[0];
        let start = before.text.find("Jane Roe").unwrap();
        let areas = before.areas(start, start + 8);

        let report = redact_file(&src, &out, &areas, RedactOptions::default()).unwrap();
        assert_eq!(report.areas, 1);

        let after = &page_texts(&out, None).unwrap()[0];
        assert!(!after.text.contains("Jane"), "redacted text is gone: {:?}", after.text);
        assert!(after.text.contains("Keep this line"));
        assert!(after.text.contains("Name:"));
        assert!(page_texts(&src, None).unwrap()[0].text.contains("Jane Roe"), "original untouched");
        assert!(!out.with_extension("pdf.partial").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removes_metadata_by_default() {
        let dir = temp_dir("meta");
        let src = dir.join("in.pdf");
        let out = dir.join("out.pdf");
        write_text_pdf(&src, &["Secret"]);
        let area = page_texts(&src, None).unwrap()[0].areas(0, 6);
        redact_file(&src, &out, &area, RedactOptions::default()).unwrap();
        let bytes = fs::read(&out).unwrap();
        let has = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        assert!(has("%PDF"));
        assert!(!has("Test Author"), "info dictionary was dropped");
        fs::remove_dir_all(dir).unwrap();
    }

    /// The Tesseract data the app bundles (fetched by scripts/fetch-tessdata.mjs).
    fn tessdata() -> Option<Ocr> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/resources/tessdata");
        dir.join("eng.traineddata").exists().then(|| Ocr { datadir: dir, languages: "eng".into() })
    }

    /// A "scanned" copy of a text PDF: the page as a 200 dpi image, no text layer.
    fn scan(src: &Path, out: &Path) {
        use mupdf::{ColorParams, Colorspace, Image};
        let doc = Document::open(src.to_str().unwrap()).unwrap();
        let page = doc.load_page(0).unwrap();
        let bounds = page.bounds().unwrap();
        let pixmap = page.to_pixmap(&Matrix::new_scale(200.0 / 72.0, 200.0 / 72.0), &Colorspace::device_rgb(), false, false).unwrap();
        let image = Image::from_pixmap(&pixmap).unwrap();
        let mut writer = DocumentWriter::new(out.to_str().unwrap(), "pdf", "").unwrap();
        let device = writer.begin_page(bounds).unwrap();
        let (w, h) = (bounds.x1 - bounds.x0, bounds.y1 - bounds.y0);
        device.fill_image(&image, &Matrix::new_scale(w, h), 1.0, ColorParams::default()).unwrap();
        writer.end_page(device).unwrap();
    }

    #[test]
    fn reads_and_redacts_scanned_pages_with_ocr() {
        let Some(ocr) = tessdata() else {
            eprintln!("skipped: run `node scripts/fetch-tessdata.mjs` first");
            return;
        };
        let dir = temp_dir("ocr");
        let (text_pdf, scanned, out) = (dir.join("text.pdf"), dir.join("scan.pdf"), dir.join("out.pdf"));
        write_text_pdf(&text_pdf, &["Name: Jane Roe", "Keep this line"]);
        scan(&text_pdf, &scanned);

        assert!(page_texts(&scanned, None).unwrap()[0].text.trim().is_empty(), "no text layer");
        let page = &page_texts(&scanned, Some(&ocr)).unwrap()[0];
        assert!(page.ocr);
        let start = page.text.find("Jane Roe").unwrap_or_else(|| panic!("OCR text: {:?}", page.text));
        let areas = page.areas(start, start + 8);

        // OCR boxes land where the text really is (same place as in the text PDF).
        let original = &page_texts(&text_pdf, None).unwrap()[0];
        let o = original.areas(original.text.find("Jane Roe").unwrap(), original.text.find("Jane Roe").unwrap() + 8)[0];
        // OCR boxes hug the ink while text-layer boxes span the font's full height,
        // so compare centres.
        let centre = |a: &Area| ((a.x0 + a.x1) / 2.0, (a.y0 + a.y1) / 2.0);
        let (c, d) = (centre(&areas[0]), centre(&o));
        assert!((c.0 - d.0).abs() < 3.0 && (c.1 - d.1).abs() < 3.0, "{:?} vs {:?}", areas[0], o);

        let options = RedactOptions { ocr: Some(ocr.clone()), ..RedactOptions::default() };
        redact_file(&scanned, &out, &areas, options).unwrap();
        let after = &page_texts(&out, Some(&ocr)).unwrap()[0];
        assert!(!after.text.contains("Jane"), "pixels under the box are gone: {:?}", after.text);
        assert!(after.text.contains("Keep this line"));
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
