//! Conversions: PDF → images / text / Word / searchable PDF, and images → PDF.
//!
//! Every function writes new files and never touches its sources. Page numbers
//! are 1-based. `progress(done, total)` is called after each page.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use mupdf::{ColorParams, Colorspace, Document, DocumentWriter, Image, Matrix, Rect};
use serde::{Deserialize, Serialize};

use crate::redact::{mu, open_pdf, page_text, read_page_text, Ocr, OcrPdf};
use crate::{assemble_files, same_file, Error, OutputSpec, PageRef, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", path.display())))
}

fn check_pages(pages: &[u32], count: u32, path: &Path) -> Result<()> {
    if pages.is_empty() {
        return Err(Error::NoPages);
    }
    match pages.iter().find(|&&p| p == 0 || p > count) {
        Some(&page) => Err(Error::PageOutOfRange { path: path.to_owned(), page, count }),
        None => Ok(()),
    }
}

/// Writes atomically: to `<out>.partial`, then renamed.
fn write_file(out: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let partial = out.with_extension(format!("{}.partial", out.extension().and_then(|e| e.to_str()).unwrap_or("")));
    let result = write(&partial).and_then(|()| fs::rename(&partial, out).map_err(|source| Error::Write { path: out.to_owned(), source }));
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// File names [`to_images`] uses: `<stem>_p<N>.<ext>`, zero-padded to the
/// page count's width so they sort in order.
pub fn image_names(stem: &str, pages: &[u32], page_count: u32, format: ImageFormat) -> Vec<String> {
    let width = page_count.to_string().len();
    pages.iter().map(|n| format!("{stem}_p{n:0width$}.{}", format.extension())).collect()
}

/// One image per page, named by [`image_names`] in `out_dir`. Returns the paths.
pub fn to_images(
    src: &Path,
    pages: &[u32],
    out_dir: &Path,
    stem: &str,
    format: ImageFormat,
    dpi: u32,
    progress: &dyn Fn(u32, u32),
) -> Result<Vec<PathBuf>> {
    let doc = open_pdf(src)?;
    let count = doc.page_count().map_err(mu)? as u32;
    check_pages(pages, count, src)?;
    let names = image_names(stem, pages, count, format);
    let scale = dpi.clamp(36, 600) as f32 / 72.0;
    let mut written = Vec::new();
    for (i, &n) in pages.iter().enumerate() {
        let page = doc.load_page(n as i32 - 1).map_err(mu)?;
        let pixmap = page.to_pixmap(&Matrix::new_scale(scale, scale), &Colorspace::device_rgb(), false, true).map_err(mu)?;
        let (w, h) = (pixmap.width(), pixmap.height());
        // Copy row by row: rows may be padded beyond width × 3 bytes.
        let stride = pixmap.stride() as usize;
        let rgb: Vec<u8> = pixmap.samples().chunks(stride).take(h as usize).flat_map(|row| &row[..w as usize * 3]).copied().collect();
        let image = image::RgbImage::from_raw(w, h, rgb).ok_or_else(|| Error::Mupdf("unexpected pixmap layout".into()))?;
        let out = out_dir.join(&names[i]);
        write_file(&out, |tmp| {
            let file = fs::File::create(tmp).map_err(|source| Error::Write { path: out.clone(), source })?;
            let mut file = std::io::BufWriter::new(file);
            let result = match format {
                ImageFormat::Png => image.write_with_encoder(image::codecs::png::PngEncoder::new(&mut file)),
                ImageFormat::Jpeg => image.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 90)),
            };
            result.map_err(|e| Error::Mupdf(format!("could not encode image: {e}")))?;
            file.flush().map_err(|source| Error::Write { path: out.clone(), source })
        })?;
        written.push(out);
        progress(i as u32 + 1, pages.len() as u32);
    }
    Ok(written)
}

/// Plain text, pages separated by a form feed. Scanned pages are read with OCR when `ocr` is given.
pub fn to_text(src: &Path, pages: &[u32], out: &Path, ocr: Option<&Ocr>, progress: &dyn Fn(u32, u32)) -> Result<()> {
    if same_file(src, out) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }
    let doc = open_pdf(src)?;
    let count = doc.page_count().map_err(mu)? as u32;
    check_pages(pages, count, src)?;
    let mut text = String::new();
    for (i, &n) in pages.iter().enumerate() {
        if i > 0 {
            text.push('\u{000C}');
        }
        text.push_str(page_text(&doc, n as i32 - 1, ocr)?.text.trim_end());
        text.push('\n');
        progress(i as u32 + 1, pages.len() as u32);
    }
    write_file(out, |tmp| fs::write(tmp, text).map_err(|source| Error::Write { path: out.to_owned(), source }))
}

/// A Word document (MuPDF's DOCX writer: text and layout, roughly). Scanned
/// pages are OCRed first when `ocr` is given, so their text comes through.
pub fn to_docx(src: &Path, pages: &[u32], out: &Path, ocr: Option<&Ocr>, progress: &dyn Fn(u32, u32)) -> Result<()> {
    let doc = open_pdf(src)?;
    let count = doc.page_count().map_err(mu)? as u32;
    check_pages(pages, count, src)?;
    write_file(out, |tmp| {
        let mut writer = DocumentWriter::new(path_str(tmp)?, "docx", "").map_err(mu)?;
        for (i, &n) in pages.iter().enumerate() {
            let index = n as i32 - 1;
            let scanned = ocr.is_some() && read_page_text(&doc, index)?.text.trim().is_empty();
            // For a scanned page, write the OCR'd version: image plus a text layer.
            let mut ocr_pdf = None;
            let (page_doc, page_index) = if scanned {
                let mut pdf = OcrPdf::new(ocr.expect("checked above"))?;
                pdf.add(&doc, index)?;
                let path = path_str(pdf.finish())?.to_owned();
                ocr_pdf = Some(pdf);
                (Document::open(&path).map_err(mu)?, 0)
            } else {
                (Document::open(path_str(src)?).map_err(mu)?, index)
            };
            let page = page_doc.load_page(page_index).map_err(mu)?;
            let device = writer.begin_page(page.bounds().map_err(mu)?).map_err(mu)?;
            page.run(&device, &Matrix::IDENTITY).map_err(mu)?;
            writer.end_page(device).map_err(mu)?;
            drop(page);
            drop(page_doc);
            drop(ocr_pdf);
            progress(i as u32 + 1, pages.len() as u32);
        }
        Ok(())
    })
}

/// A copy where every page without a text layer gets one by OCR, so the PDF can
/// be searched and its text copied. Pages that already have text are kept as
/// they are. Returns the page numbers that were OCRed.
pub fn searchable_pdf(src: &Path, out: &Path, ocr: &Ocr, progress: &dyn Fn(u32, u32)) -> Result<Vec<u32>> {
    if same_file(src, out) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }
    let doc = open_pdf(src)?;
    let count = doc.page_count().map_err(mu)? as u32;
    let mut scanned = Vec::new();
    let mut ocr_pdf = OcrPdf::new(ocr)?;
    let mut pages = Vec::new();
    for n in 1..=count {
        if read_page_text(&doc, n as i32 - 1)?.text.trim().is_empty() {
            ocr_pdf.add(&doc, n as i32 - 1)?;
            scanned.push(n);
            pages.push(PageRef { source: 1, page: scanned.len() as u32, rotate: 0 });
        } else {
            pages.push(PageRef { source: 0, page: n, rotate: 0 });
        }
        progress(n, count);
    }
    drop(doc);
    if scanned.is_empty() {
        return Ok(scanned);
    }
    let ocr_path = ocr_pdf.finish().to_owned();
    assemble_files(&[src.to_owned(), ocr_path], &[OutputSpec { path: out.to_owned(), pages }])?;
    Ok(scanned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageFit {
    /// The page is the image's own size (at its stored resolution).
    Image,
    /// A4 portrait or landscape (following the image), image centred, 10 mm margins.
    A4,
}

/// One PDF page per image (JPG, PNG, TIFF, BMP, GIF…), in the given order.
pub fn images_to_pdf(images: &[PathBuf], out: &Path, fit: PageFit, progress: &dyn Fn(u32, u32)) -> Result<()> {
    if images.is_empty() {
        return Err(Error::NoPages);
    }
    if images.iter().any(|i| same_file(i, out)) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }
    write_file(out, |tmp| {
        let mut writer = DocumentWriter::new(path_str(tmp)?, "pdf", "compress,compress-images").map_err(mu)?;
        for (i, path) in images.iter().enumerate() {
            let image = Image::from_file(path_str(path)?).map_err(|e| Error::Mupdf(format!("could not read {}: {e}", path.display())))?;
            let (xres, yres) = image.resolution();
            let res = |r: i32| if (36..=2400).contains(&r) { r as f32 } else { 96.0 };
            // Image size in points.
            let (w, h) = (image.width() as f32 * 72.0 / res(xres), image.height() as f32 * 72.0 / res(yres));
            let (page, placed) = match fit {
                PageFit::Image => {
                    let r = Rect { x0: 0.0, y0: 0.0, x1: w, y1: h };
                    (r, r)
                }
                PageFit::A4 => {
                    let (pw, ph) = if w > h { (842.0, 595.0) } else { (595.0, 842.0) };
                    let margin = 28.35; // 10 mm
                    let scale = ((pw - 2.0 * margin) / w).min((ph - 2.0 * margin) / h).min(1.0);
                    let (iw, ih) = (w * scale, h * scale);
                    let (x0, y0) = ((pw - iw) / 2.0, (ph - ih) / 2.0);
                    (Rect { x0: 0.0, y0: 0.0, x1: pw, y1: ph }, Rect { x0, y0, x1: x0 + iw, y1: y0 + ih })
                }
            };
            let device = writer.begin_page(page).map_err(mu)?;
            let ctm = Matrix { a: placed.x1 - placed.x0, b: 0.0, c: 0.0, d: placed.y1 - placed.y0, e: placed.x0, f: placed.y0 };
            device.fill_image(&image, &ctm, 1.0, ColorParams::default()).map_err(mu)?;
            writer.end_page(device).map_err(mu)?;
            progress(i as u32 + 1, images.len() as u32);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redact::page_texts;
    use crate::tests_support::{temp_dir, write_text_pdf};

    fn no_progress(_: u32, _: u32) {}

    fn tessdata() -> Option<Ocr> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/resources/tessdata");
        dir.join("eng.traineddata").exists().then(|| Ocr { datadir: dir, languages: "eng".into() })
    }

    #[test]
    fn pages_to_png_and_jpeg() {
        let dir = temp_dir("convert-images");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["Hello"]);
        let png = to_images(&src, &[1], &dir, "out", ImageFormat::Png, 72, &no_progress).unwrap();
        let jpg = to_images(&src, &[1], &dir, "out", ImageFormat::Jpeg, 144, &no_progress).unwrap();
        assert_eq!(png[0].file_name().unwrap(), "out_p1.png");
        let (w, h) = image::image_dimensions(&png[0]).unwrap();
        assert_eq!((w, h), (595, 842), "A4 at 72 dpi");
        assert_eq!(image::image_dimensions(&jpg[0]).unwrap(), (1190, 1684), "A4 at 144 dpi");
        assert!(matches!(to_images(&src, &[2], &dir, "x", ImageFormat::Png, 72, &no_progress), Err(Error::PageOutOfRange { .. })));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_and_word() {
        let dir = temp_dir("convert-text");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["First line", "Second line"]);
        let txt = dir.join("out.txt");
        to_text(&src, &[1], &txt, None, &no_progress).unwrap();
        assert_eq!(fs::read_to_string(&txt).unwrap(), "First line\nSecond line\n");

        let docx = dir.join("out.docx");
        to_docx(&src, &[1], &docx, None, &no_progress).unwrap();
        let bytes = fs::read(&docx).unwrap();
        assert_eq!(&bytes[..2], b"PK", "a DOCX is a zip file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn images_become_pdf_pages() {
        let dir = temp_dir("convert-img2pdf");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["Picture me"]);
        let imgs = to_images(&src, &[1], &dir, "pic", ImageFormat::Png, 72, &no_progress).unwrap();
        let out = dir.join("out.pdf");
        images_to_pdf(&[imgs[0].clone(), imgs[0].clone()], &out, PageFit::A4, &no_progress).unwrap();
        let pages = page_texts(&out, None).unwrap();
        assert_eq!(pages.len(), 2);
        assert!((pages[0].size.width - 595.0).abs() < 1.0 && (pages[0].size.height - 842.0).abs() < 1.0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scanned_pages_become_searchable() {
        let Some(ocr) = tessdata() else {
            eprintln!("skipped: run `node scripts/fetch-tessdata.mjs` first");
            return;
        };
        let dir = temp_dir("convert-searchable");
        let src = dir.join("in.pdf");
        write_text_pdf(&src, &["Searchable words here"]);
        let imgs = to_images(&src, &[1], &dir, "scan", ImageFormat::Png, 200, &no_progress).unwrap();
        let scanned = dir.join("scanned.pdf");
        images_to_pdf(&imgs, &scanned, PageFit::A4, &no_progress).unwrap();
        // Mixed document: a text page, then the scanned page.
        let mixed = dir.join("mixed.pdf");
        assemble_files(
            &[src.clone(), scanned.clone()],
            &[OutputSpec { path: mixed.clone(), pages: vec![PageRef { source: 0, page: 1, rotate: 0 }, PageRef { source: 1, page: 1, rotate: 0 }] }],
        )
        .unwrap();

        let out = dir.join("out.pdf");
        assert_eq!(searchable_pdf(&mixed, &out, &ocr, &no_progress).unwrap(), [2]);
        let pages = page_texts(&out, None).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages[1].text.contains("Searchable"), "page 2 has a text layer now: {:?}", pages[1].text);
        assert_eq!(pages[0].text.trim(), "Searchable words here", "page 1 is unchanged");

        let docx = dir.join("scan.docx");
        to_docx(&scanned, &[1], &docx, Some(&ocr), &no_progress).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
