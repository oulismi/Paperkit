//! Edits: add text, images (signatures), whiteout boxes and highlights; fill
//! form fields. Always written to a new file; the source is never changed.
//!
//! Positions are in page space, like [`crate::redact::Area`]: points, origin at
//! the top-left of the visible page, y downwards, rotation applied. Content is
//! drawn through the inverse of the page transform, so it lands where the user
//! put it and reads upright, rotated pages included.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mupdf::pdf::{InsertFontOptions, InsertImageOptions, PageImageSource, PdfDocument, PdfPage, WidgetType};
use mupdf::{Matrix, Rect};
use serde::{Deserialize, Serialize};

use crate::redact::{mu, open_pdf, Area};
use crate::{same_file, Error, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EditOp {
    /// Text whose first line's top-left corner is at (x, y). `\n` starts a new line.
    #[serde(rename_all = "camelCase")]
    Text { page: u32, x: f32, y: f32, text: String, size: f32, color: [f32; 3] },
    /// An image file (PNG, JPG…) stretched to the box: a logo, a scanned signature.
    Image { area: Area, path: PathBuf },
    /// An opaque white box. It *hides* what is under it; the content stays in the
    /// file. Use redaction to remove content.
    Whiteout { area: Area },
    /// A highlight annotation (other readers show it and can remove it).
    Highlight { area: Area, color: [f32; 3] },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    pub page: u32,
    pub name: String,
    /// "text", "checkbox", "radio", "choice", "signature" or "button".
    pub kind: &'static str,
    pub value: String,
    pub read_only: bool,
    pub area: Area,
}

fn page_space(page: &PdfPage) -> Result<(Rect, Matrix)> {
    let bounds = page.bounds().map_err(mu)?;
    let inverse = page.ctm().map_err(mu)?.invert().ok_or_else(|| Error::Mupdf("page transform can't be inverted".into()))?;
    Ok((bounds, inverse))
}

fn to_fitz(a: &Area, bounds: &Rect) -> Rect {
    Rect { x0: a.x0 + bounds.x0, y0: a.y0 + bounds.y0, x1: a.x1 + bounds.x0, y1: a.y1 + bounds.y0 }
}

/// `q a b c d e f cm`: subsequent drawing uses MuPDF page space (y downwards).
fn enter_page_space(inv: &Matrix) -> String {
    format!("q {} {} {} {} {} {} cm\n", inv.a, inv.b, inv.c, inv.d, inv.e, inv.f)
}

fn kind_name(kind: WidgetType) -> &'static str {
    match kind {
        WidgetType::Text => "text",
        WidgetType::Checkbox => "checkbox",
        WidgetType::RadioButton => "radio",
        WidgetType::Combobox | WidgetType::Listbox => "choice",
        WidgetType::Signature => "signature",
        _ => "button",
    }
}

/// The form fields of a PDF, page by page.
pub fn form_fields(src: &Path) -> Result<Vec<FormField>> {
    let doc = open_pdf(src)?;
    let mut fields = Vec::new();
    for index in 0..doc.page_count().map_err(mu)? {
        let page = PdfPage::try_from(doc.load_page(index).map_err(mu)?).map_err(mu)?;
        let bounds = page.bounds().map_err(mu)?;
        for widget in page.widgets() {
            let Some(name) = widget.name().map_err(mu)? else { continue };
            let r = widget.annotation().bounds().map_err(mu)?;
            fields.push(FormField {
                page: index as u32 + 1,
                name,
                kind: kind_name(widget.r#type().map_err(mu)?),
                value: widget.value().map_err(mu)?.unwrap_or_default(),
                read_only: widget.is_readonly().map_err(mu)?,
                area: Area { page: index as u32 + 1, x0: r.x0 - bounds.x0, y0: r.y0 - bounds.y0, x1: r.x1 - bounds.x0, y1: r.y1 - bounds.y0 },
            });
        }
    }
    Ok(fields)
}

/// Writes a copy of `src` to `out` with `ops` drawn and `fields` (name → value) filled.
/// `font` is the TrueType font for added text (it must cover the text's characters).
pub fn edit_file(src: &Path, out: &Path, ops: &[EditOp], fields: &HashMap<String, String>, font: &Path) -> Result<()> {
    if ops.is_empty() && fields.is_empty() {
        return Err(Error::NothingToDo);
    }
    if same_file(src, out) {
        return Err(Error::OutputIsSource(out.to_owned()));
    }
    let mut doc = open_pdf(src)?;
    let count = doc.page_count().map_err(mu)? as u32;
    let font_bytes = if ops.iter().any(|o| matches!(o, EditOp::Text { .. })) {
        Some(std::fs::read(font).map_err(|e| Error::Mupdf(format!("could not read font {}: {e}", font.display())))?)
    } else {
        None
    };

    let mut pages: Vec<u32> = ops.iter().map(op_page).collect();
    pages.sort_unstable();
    pages.dedup();
    for n in pages {
        if n == 0 || n > count {
            return Err(Error::PageOutOfRange { path: src.to_owned(), page: n, count });
        }
        let mut page = PdfPage::try_from(doc.load_page(n as i32 - 1).map_err(mu)?).map_err(mu)?;
        let (bounds, inv) = page_space(&page)?;
        // Whiteouts first so text and images can sit on top of them.
        let mut ordered: Vec<&EditOp> = ops.iter().filter(|o| op_page(o) == n).collect();
        ordered.sort_by_key(|o| !matches!(o, EditOp::Whiteout { .. }));
        for op in ordered {
            match op {
                EditOp::Whiteout { area } => {
                    let r = to_fitz(area, &bounds);
                    let stream = format!("{}1 1 1 rg {} {} {} {} re f\nQ\n", enter_page_space(&inv), r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0);
                    page.insert_contents(&mut doc, stream.as_bytes(), true).map_err(mu)?;
                }
                EditOp::Highlight { area, color } => {
                    let mut annot = page.add_highlight_annotation(to_fitz(area, &bounds).quad()).map_err(mu)?;
                    annot.set_color(mupdf::color::AnnotationColor::Rgb { red: color[0], green: color[1], blue: color[2] }).map_err(mu)?;
                }
                EditOp::Image { area, path } => {
                    let data = std::fs::read(path).map_err(|e| Error::Mupdf(format!("could not read {}: {e}", path.display())))?;
                    let source = PageImageSource::Bytes { data: &data, format_hint: None };
                    page.insert_image(&mut doc, to_fitz(area, &bounds), source, InsertImageOptions::default()).map_err(mu)?;
                }
                EditOp::Text { x, y, text, size, color, .. } => {
                    let bytes = font_bytes.as_deref().expect("read above");
                    let mut options = InsertFontOptions::new("PaperKitSans");
                    options.fontfile = Some(bytes);
                    options.simple = false;
                    let (name, _, info) = page.insert_font(&mut doc, &options).map_err(mu)?;
                    // The font is embedded whole (Identity-H), but the wrapper's glyph map
                    // only covers codes 0–255, so look glyphs up in the font itself.
                    let face = mupdf::Font::from_bytes("PaperKitSans", bytes).map_err(mu)?;
                    let fallback = face.encode_character('?' as i32).unwrap_or(0);
                    let gid = |c: char| face.encode_character(c as i32).ok().filter(|&g| g > 0).unwrap_or(fallback);
                    let ascender = if (0.5..=1.5).contains(&info.ascender) { info.ascender } else { 0.9 };
                    let mut stream = enter_page_space(&inv);
                    stream.push_str(&format!("BT /{name} {size} Tf {} {} {} rg\n", color[0], color[1], color[2]));
                    for (i, line) in text.lines().enumerate() {
                        let baseline = bounds.y0 + y + size * (ascender + 1.25 * i as f32);
                        let hex: String = line.chars().map(|c| format!("{:04X}", gid(c))).collect();
                        // y is flipped in page space; the text matrix flips it back so glyphs stand upright.
                        stream.push_str(&format!("1 0 0 -1 {} {baseline} Tm <{hex}> Tj\n", bounds.x0 + x));
                    }
                    stream.push_str("ET\nQ\n");
                    page.insert_contents(&mut doc, stream.as_bytes(), true).map_err(mu)?;
                }
            }
        }
    }

    for index in 0..count as i32 {
        if fields.is_empty() {
            break;
        }
        let page = PdfPage::try_from(doc.load_page(index).map_err(mu)?).map_err(mu)?;
        for mut widget in page.widgets() {
            let Some(name) = widget.name().map_err(mu)? else { continue };
            if let Some(value) = fields.get(&name) {
                widget.set_value(&mut doc, value, true).map_err(mu)?;
                widget.update().map_err(mu)?;
            }
        }
    }

    let partial = out.with_extension("pdf.partial");
    let result = save(&doc, &partial).and_then(|()| std::fs::rename(&partial, out).map_err(|source| Error::Write { path: out.to_owned(), source }));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn op_page(op: &EditOp) -> u32 {
    match op {
        EditOp::Text { page, .. } => *page,
        EditOp::Image { area, .. } | EditOp::Whiteout { area } | EditOp::Highlight { area, .. } => area.page,
    }
}

fn save(doc: &PdfDocument, path: &Path) -> Result<()> {
    let path_str = path.to_str().ok_or_else(|| Error::Mupdf(format!("unsupported path: {}", path.display())))?;
    let mut write = mupdf::pdf::PdfWriteOptions::default();
    write.set_garbage_level(3).set_compress(true);
    doc.save_with_options(path_str, write).map_err(mu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{to_images, ImageFormat};
    use crate::redact::page_texts;
    use crate::tests_support::{temp_dir, write_form_pdf, write_text_pdf};

    fn font() -> Option<PathBuf> {
        let f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/resources/fonts/NotoSans-Regular.ttf");
        f.exists().then_some(f)
    }

    #[test]
    fn adds_text_in_any_of_the_four_languages() {
        let Some(font) = font() else {
            eprintln!("skipped: run `node scripts/fetch-fonts.mjs` first");
            return;
        };
        let dir = temp_dir("edit-text");
        let (src, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
        write_text_pdf(&src, &["Original line"]);
        let text = "Approved — café, één, Γειά σας\nSecond line";
        let op = EditOp::Text { page: 1, x: 100.0, y: 300.0, text: text.into(), size: 14.0, color: [0.0, 0.0, 0.6] };
        edit_file(&src, &out, &[op], &HashMap::new(), &font).unwrap();

        let page = &page_texts(&out, None).unwrap()[0];
        assert!(page.text.contains("Approved — café, één, Γειά σας"), "{:?}", page.text);
        assert!(page.text.contains("Second line"));
        assert!(page.text.contains("Original line"));
        let start = page.text.find("Approved").unwrap();
        let a = page.areas(start, start + 8)[0];
        assert!((a.x0 - 100.0).abs() < 3.0 && a.y0 > 295.0 && a.y1 < 325.0, "placed where asked: {a:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn adds_images_whiteouts_and_highlights() {
        let dir = temp_dir("edit-shapes");
        let (src, out) = (dir.join("in.pdf"), dir.join("out.pdf"));
        write_text_pdf(&src, &["Hide this", "Mark this"]);
        let img = to_images(&src, &[1], &dir, "sig", ImageFormat::Png, 36, &|_, _| {}).unwrap().remove(0);
        let texts = page_texts(&src, None).unwrap();
        let hide = texts[0].areas(0, 9)[0];
        let mark = texts[0].areas(10, 19)[0];
        let ops = [
            EditOp::Whiteout { area: hide },
            EditOp::Highlight { area: mark, color: [1.0, 0.9, 0.2] },
            EditOp::Image { area: Area { page: 1, x0: 300.0, y0: 600.0, x1: 400.0, y1: 700.0 }, path: img },
        ];
        edit_file(&src, &out, &ops, &HashMap::new(), Path::new("unused.ttf")).unwrap();

        {
            let doc = mupdf::pdf::PdfDocument::open(out.to_str().unwrap()).unwrap();
            let page = PdfPage::try_from(doc.load_page(0).unwrap()).unwrap();
            assert_eq!(page.annotations().count(), 1, "one highlight");
            assert_eq!(page.images().unwrap().len(), 1, "one image");
        } // closed before the folder is deleted (Windows locks open files)
        // Whiteout hides but does not remove: the text is still in the file.
        assert!(page_texts(&out, None).unwrap()[0].text.contains("Hide this"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_and_fills_form_fields() {
        let dir = temp_dir("edit-form");
        let (src, out) = (dir.join("form.pdf"), dir.join("out.pdf"));
        write_form_pdf(&src);
        let fields = form_fields(&src).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!((fields[0].name.as_str(), fields[0].kind, fields[0].value.as_str()), ("full_name", "text", ""));

        let values = HashMap::from([("full_name".to_owned(), "Jane Roe".to_owned())]);
        edit_file(&src, &out, &[], &values, Path::new("unused.ttf")).unwrap();
        assert_eq!(form_fields(&out).unwrap()[0].value, "Jane Roe");
        assert!(matches!(edit_file(&src, &out, &[], &HashMap::new(), Path::new("x")), Err(Error::NothingToDo)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
