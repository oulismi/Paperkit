//! PDF page operations for PaperKit.
//!
//! Everything the page organizer does (merge, split, extract, reorder, rotate,
//! delete) is one operation: build a new PDF from a list of [`PageRef`]s, each
//! naming a page of a source file and an extra rotation. See [`assemble`].
//!
//! Redaction lives in [`redact`] and uses MuPDF.
//!
//! Source files are never modified: outputs are written to a temporary file next
//! to the destination and renamed into place, and an output path that equals a
//! source path is refused.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

pub mod convert;
pub mod edit;
pub mod redact;

/// Page attributes a page may inherit from its ancestors in the page tree
/// (PDF 32000-1, 7.7.3.4). They are copied onto each page before the page is
/// moved into a new tree, otherwise it would lose them.
const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not open {path}: {source}")]
    Load { path: PathBuf, source: lopdf::Error },
    #[error("{0} is password-protected; protected PDFs are not supported yet")]
    Encrypted(PathBuf),
    #[error("{path} has {count} pages; page {page} does not exist")]
    PageOutOfRange { path: PathBuf, page: u32, count: u32 },
    #[error("source index {0} does not exist")]
    SourceOutOfRange(usize),
    #[error("rotation must be a multiple of 90 degrees, got {0}")]
    InvalidRotation(i32),
    #[error("an output has no pages")]
    NoPages,
    #[error("refusing to overwrite a source file: {0}")]
    OutputIsSource(PathBuf),
    #[error("there is nothing to redact")]
    NothingToRedact,
    #[error("there are no changes to save")]
    NothingToDo,
    #[error("redaction check failed: {0} character(s) were still under the boxes, so nothing was saved")]
    RedactionIncomplete(usize),
    #[error("{0}")]
    Mupdf(String),
    #[error("malformed PDF: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: std::io::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

/// One page of the output: page `page` (1-based) of source `source`, turned
/// clockwise by `rotate` degrees on top of whatever rotation it already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRef {
    pub source: usize,
    pub page: u32,
    #[serde(default)]
    pub rotate: i32,
}

/// One file to produce: where to write it and which pages it holds, in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpec {
    pub path: PathBuf,
    pub pages: Vec<PageRef>,
}

/// Opens a PDF and refuses ones that stay encrypted after loading (lopdf opens
/// files protected by an empty user password on its own).
pub fn open(path: &Path) -> Result<Document> {
    let doc = Document::load(path).map_err(|source| Error::Load { path: path.to_owned(), source })?;
    if doc.is_encrypted() {
        return Err(Error::Encrypted(path.to_owned()));
    }
    Ok(doc)
}

pub fn page_count(path: &Path) -> Result<u32> {
    Ok(open(path)?.get_pages().len() as u32)
}

/// Builds every output from the given source files. Each source is parsed once.
/// All page references are checked before anything is written.
pub fn assemble_files(sources: &[PathBuf], outputs: &[OutputSpec]) -> Result<()> {
    for out in outputs {
        if out.pages.is_empty() {
            return Err(Error::NoPages);
        }
        if sources.iter().any(|src| same_file(src, &out.path)) {
            return Err(Error::OutputIsSource(out.path.clone()));
        }
    }

    let docs = sources.iter().map(|p| open(p)).collect::<Result<Vec<_>>>()?;
    for out in outputs {
        validate(&docs, sources, &out.pages)?;
    }
    for out in outputs {
        let mut doc = assemble(&docs, &out.pages)?;
        write_atomic(&mut doc, &out.path)?;
    }
    Ok(())
}

/// Whether `out` names the existing file `src` (comparing resolved paths, so
/// `.\a.pdf` and `C:\dir\a.pdf` match). `out` may not exist yet.
pub(crate) fn same_file(src: &Path, out: &Path) -> bool {
    let Ok(src) = fs::canonicalize(src) else { return false };
    let resolved = out.parent().map(|d| if d.as_os_str().is_empty() { Path::new(".") } else { d });
    match resolved.and_then(|dir| fs::canonicalize(dir).ok()).zip(out.file_name()) {
        Some((dir, name)) => dir.join(name) == src,
        None => false,
    }
}

fn validate(docs: &[Document], paths: &[PathBuf], pages: &[PageRef]) -> Result<()> {
    for r in pages {
        let doc = docs.get(r.source).ok_or(Error::SourceOutOfRange(r.source))?;
        let count = doc.get_pages().len() as u32;
        if r.page == 0 || r.page > count {
            let path = paths.get(r.source).cloned().unwrap_or_default();
            return Err(Error::PageOutOfRange { path, page: r.page, count });
        }
        if r.rotate % 90 != 0 {
            return Err(Error::InvalidRotation(r.rotate));
        }
    }
    Ok(())
}

/// Builds a new document holding `pages`, in order, taken from `sources`.
///
/// Bookmarks, form fields and document metadata of the sources are not carried
/// over yet. A page listed twice is duplicated.
pub fn assemble(sources: &[Document], pages: &[PageRef]) -> Result<Document> {
    if pages.is_empty() {
        return Err(Error::NoPages);
    }
    let mut out = Document::with_version("1.7");
    // source index -> (page number -> object id in `out`)
    let mut imported: HashMap<usize, HashMap<u32, ObjectId>> = HashMap::new();

    for r in pages {
        if imported.contains_key(&r.source) {
            continue;
        }
        let src = sources.get(r.source).ok_or(Error::SourceOutOfRange(r.source))?;
        let mut doc = src.clone();
        doc.renumber_objects_with(out.max_id + 1);
        let page_ids = doc.get_pages();
        for &id in page_ids.values() {
            flatten_inherited(&mut doc, id)?;
        }
        if doc.version > out.version {
            out.version = doc.version.clone();
        }
        out.max_id = out.max_id.max(doc.max_id);
        out.objects.extend(doc.objects);
        imported.insert(r.source, page_ids.into_iter().collect());
    }

    let tree_id = out.new_object_id();
    let mut used = HashSet::new();
    let mut kids = Vec::with_capacity(pages.len());
    for r in pages {
        if r.rotate % 90 != 0 {
            return Err(Error::InvalidRotation(r.rotate));
        }
        let original = *imported[&r.source].get(&r.page).ok_or_else(|| Error::PageOutOfRange {
            path: PathBuf::new(),
            page: r.page,
            count: imported[&r.source].len() as u32,
        })?;
        let mut page = out.get_dictionary(original)?.clone();
        let rotation = (current_rotation(&page) + r.rotate).rem_euclid(360);
        page.set("Parent", tree_id);
        if rotation == 0 {
            page.remove(b"Rotate");
        } else {
            page.set("Rotate", rotation as i64);
        }
        let id = if used.insert(original) {
            out.objects.insert(original, Object::Dictionary(page));
            original
        } else {
            out.add_object(page)
        };
        kids.push(Object::Reference(id));
    }

    out.objects.insert(
        tree_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => kids.len() as i64, "Kids" => kids }),
    );
    let catalog = out.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree_id });
    out.trailer.set("Root", catalog);
    out.prune_objects();
    out.compress();
    Ok(out)
}

/// Copies inherited attributes from the page's ancestors onto the page itself.
fn flatten_inherited(doc: &mut Document, page_id: ObjectId) -> Result<()> {
    let mut inherited: Vec<(&[u8], Object)> = Vec::new();
    let page = doc.get_dictionary(page_id)?;
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut seen = HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            break; // malformed tree with a cycle
        }
        let Ok(node) = doc.get_dictionary(id) else { break };
        for key in INHERITABLE {
            if !page.has(key) && !inherited.iter().any(|(k, _)| *k == key) {
                if let Ok(value) = node.get(key) {
                    inherited.push((key, value.clone()));
                }
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    let page = doc.get_dictionary_mut(page_id)?;
    for (key, value) in inherited {
        page.set(key, value);
    }
    Ok(())
}

fn current_rotation(page: &Dictionary) -> i32 {
    page.get(b"Rotate").and_then(Object::as_i64).map(|r| r as i32).unwrap_or(0)
}

fn write_atomic(doc: &mut Document, path: &Path) -> Result<()> {
    let werr = |source| Error::Write { path: path.to_owned(), source };
    let tmp = path.with_extension("pdf.partial");
    let mut file = fs::File::create(&tmp).map_err(werr)?;
    doc.save_to(&mut file).map_err(werr)?;
    file.sync_all().map_err(werr)?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        werr(e)
    })
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::Stream;

    /// A fresh folder per call, so tests running in parallel never share one.
    pub fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("pdf-core-test-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A one-page A4 PDF with each of `lines` as a line of Helvetica text, top to
    /// bottom, and an info dictionary naming "Test Author".
    pub fn write_text_pdf(path: &Path, lines: &[&str]) {
        let mut doc = Document::with_version("1.5");
        let tree = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let mut ops = vec![];
        for (i, line) in lines.iter().enumerate() {
            ops.extend([
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 14.into()]),
                Operation::new("Td", vec![72.into(), (770 - 30 * i as i64).into()]),
                Operation::new("Tj", vec![Object::string_literal(*line)]),
                Operation::new("ET", vec![]),
            ]);
        }
        let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => tree, "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        doc.objects.insert(tree, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
        let info = doc.add_object(dictionary! { "Author" => Object::string_literal("Test Author") });
        doc.trailer.set("Root", catalog);
        doc.trailer.set("Info", info);
        doc.save(path).unwrap();
    }

    /// A one-page PDF with an empty text form field named `full_name`.
    pub fn write_form_pdf(path: &Path) {
        let mut doc = Document::with_version("1.5");
        let tree = doc.new_object_id();
        let page_id = doc.new_object_id();
        let helv = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let field = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx",
            "T" => Object::string_literal("full_name"), "V" => Object::string_literal(""),
            "Rect" => vec![100.into(), 700.into(), 400.into(), 724.into()],
            "P" => page_id, "F" => 4, "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        });
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page", "Parent" => tree,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Annots" => vec![field.into()],
            }),
        );
        doc.objects.insert(tree, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
        let acroform = dictionary! {
            "Fields" => vec![field.into()],
            "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => helv } },
        };
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree, "AcroForm" => acroform });
        doc.trailer.set("Root", catalog);
        doc.save(path).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::temp_dir;
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::Stream;

    /// A document whose pages each draw their own label ("A1", "A2", …).
    /// MediaBox and Resources live on the Pages node, so pages inherit them.
    fn sample(label: &str, pages: u32, tree_rotate: Option<i64>) -> Document {
        let mut doc = Document::with_version("1.5");
        let tree_id = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let mut kids = vec![];
        for n in 1..=pages {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("{label}{n}"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            kids.push(doc.add_object(dictionary! { "Type" => "Page", "Parent" => tree_id, "Contents" => stream }).into());
        }
        let mut tree = dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => pages as i64,
            "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        if let Some(r) = tree_rotate {
            tree.set("Rotate", r);
        }
        doc.objects.insert(tree_id, Object::Dictionary(tree));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    /// Round-trips through bytes so tests see what a reader of the file sees.
    fn reload(mut doc: Document) -> Document {
        let mut bytes = vec![];
        doc.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    fn labels(doc: &Document) -> Vec<String> {
        doc.get_pages()
            .values()
            .map(|&id| {
                let text = String::from_utf8_lossy(&doc.get_page_content(id)).into_owned();
                let start = text.find('(').unwrap() + 1;
                text[start..start + text[start..].find(')').unwrap()].to_owned()
            })
            .collect()
    }

    fn page_dicts(doc: &Document) -> Vec<Dictionary> {
        doc.get_pages().values().map(|&id| doc.get_dictionary(id).unwrap().clone()).collect()
    }

    fn p(source: usize, page: u32) -> PageRef {
        PageRef { source, page, rotate: 0 }
    }

    #[test]
    fn merges_in_the_given_order() {
        let docs = [sample("A", 3, None), sample("B", 2, None)];
        let out = reload(assemble(&docs, &[p(1, 2), p(0, 1), p(0, 3), p(1, 1)]).unwrap());
        assert_eq!(labels(&out), ["B2", "A1", "A3", "B1"]);
    }

    #[test]
    fn pages_keep_inherited_attributes() {
        let docs = [sample("A", 2, None)];
        let out = reload(assemble(&docs, &[p(0, 2)]).unwrap());
        let page = &page_dicts(&out)[0];
        assert!(page.has(b"MediaBox"));
        assert!(page.has(b"Resources"));
    }

    #[test]
    fn rotation_adds_to_existing_rotation() {
        let docs = [sample("A", 2, Some(90))];
        let pages = [PageRef { source: 0, page: 1, rotate: 90 }, PageRef { source: 0, page: 2, rotate: -90 }];
        let out = reload(assemble(&docs, &pages).unwrap());
        let dicts = page_dicts(&out);
        assert_eq!(current_rotation(&dicts[0]), 180);
        assert_eq!(current_rotation(&dicts[1]), 0);
    }

    #[test]
    fn duplicated_pages_become_separate_objects() {
        let docs = [sample("A", 1, None)];
        let out = reload(assemble(&docs, &[p(0, 1), p(0, 1)]).unwrap());
        let ids: Vec<_> = out.get_pages().into_values().collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(labels(&out), ["A1", "A1"]);
    }

    #[test]
    fn unused_pages_are_dropped_from_the_file() {
        let docs = [sample("A", 5, None)];
        let out = reload(assemble(&docs, &[p(0, 4)]).unwrap());
        // Content streams only: the saved file's own cross-reference stream doesn't count.
        let streams = out
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| s.dict.get(b"Type").and_then(Object::as_name).ok() != Some(b"XRef".as_slice()))
            .count();
        assert_eq!(streams, 1, "only page 4's content stream should remain");
    }

    #[test]
    fn rejects_bad_input() {
        let docs = [sample("A", 2, None)];
        assert!(matches!(assemble(&docs, &[]), Err(Error::NoPages)));
        assert!(matches!(assemble(&docs, &[p(1, 1)]), Err(Error::SourceOutOfRange(1))));
        assert!(matches!(assemble(&docs, &[p(0, 3)]), Err(Error::PageOutOfRange { .. })));
        let tilted = PageRef { source: 0, page: 1, rotate: 45 };
        assert!(matches!(assemble(&docs, &[tilted]), Err(Error::InvalidRotation(45))));
    }

    #[test]
    fn splits_one_file_into_several() {
        let dir = temp_dir("split");
        let src = dir.join("in.pdf");
        sample("A", 4, None).save(&src).unwrap();
        let outputs = [
            OutputSpec { path: dir.join("part1.pdf"), pages: vec![p(0, 1), p(0, 2)] },
            OutputSpec { path: dir.join("part2.pdf"), pages: vec![p(0, 3), p(0, 4)] },
        ];
        assemble_files(&[src.clone()], &outputs).unwrap();
        assert_eq!(labels(&open(&dir.join("part1.pdf")).unwrap()), ["A1", "A2"]);
        assert_eq!(labels(&open(&dir.join("part2.pdf")).unwrap()), ["A3", "A4"]);
        assert_eq!(page_count(&src).unwrap(), 4, "source is untouched");
        assert!(!dir.join("part1.pdf.partial").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_a_source() {
        let dir = temp_dir("overwrite");
        let src = dir.join("in.pdf");
        sample("A", 2, None).save(&src).unwrap();
        let outputs = [OutputSpec { path: src.clone(), pages: vec![p(0, 1)] }];
        assert!(matches!(assemble_files(&[src.clone()], &outputs), Err(Error::OutputIsSource(_))));
        assert_eq!(page_count(&src).unwrap(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checks_every_output_before_writing_any() {
        let dir = temp_dir("validate");
        let src = dir.join("in.pdf");
        sample("A", 2, None).save(&src).unwrap();
        let outputs = [
            OutputSpec { path: dir.join("ok.pdf"), pages: vec![p(0, 1)] },
            OutputSpec { path: dir.join("bad.pdf"), pages: vec![p(0, 9)] },
        ];
        assert!(assemble_files(&[src], &outputs).is_err());
        assert!(!dir.join("ok.pdf").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
