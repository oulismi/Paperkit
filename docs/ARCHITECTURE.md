# Architecture

## Stack

| Layer      | Choice                                   |
|------------|------------------------------------------|
| App shell  | Tauri 2 (Rust backend, web frontend)     |
| UI         | React + TypeScript, pdf.js for previews  |
| PDF engine | MuPDF (AGPL) for redaction and text positions; `lopdf` (MIT) for page operations |
| AI runtime | ONNX Runtime (CPU)                       |
| NER model  | GLiNER multilingual v2.1 (Apache-2.0), ONNX fp16, via `gline-rs` + ONNX Runtime |
| OCR        | Tesseract (built into MuPDF), tessdata_fast eng/fra/nld/ell, bundled |

## How the pieces talk

```
React UI (app/src)
   │  invoke("redact_scan", { path })
   ▼
Tauri commands (app/src-tauri/src/commands)
   │
   ├──► pdf-core   (crates/pdf-core)  — open, render, split, merge, apply redactions
   └──► redact-ai  (crates/redact-ai) — rules + NER model → list of suggestions
```

The UI never writes PDFs: it renders previews with pdf.js (bytes come from the
`read_pdf` command) and describes outputs to the `assemble` command.

## Page operations

Split, merge, extract, reorder, rotate and delete are one operation in `pdf-core`:
`assemble_files(sources, outputs)`, where each output is a list of
`(source, page, extra rotation)`. Pages are copied with their inherited
attributes, unused objects are pruned, files are written to a temp file and
renamed, and an output that equals a source path is refused. Bookmarks, forms
and metadata are not carried over yet.

## Redaction rules (non-negotiable)

1. AI only **suggests**; the user approves every item.
2. Applying a redaction **removes** the underlying text, images and metadata — never just draws a box.
3. Always save to a **new file**; the original is never modified.
4. After saving, re-extract text from the output to verify nothing remains under the boxes.

## Redaction

1. `redact_scan`: MuPDF extracts each page's text with a box per character;
   `redact-ai` runs format rules over it (email, phone, IBAN mod-97, card Luhn,
   labelled IDs, dates); each match becomes boxes, one per line it spans.
2. The UI groups repeats of the same text into one item; the user approves
   items, searches for more text, or draws boxes.
3. `redact_apply`, in two passes. Pass 1: MuPDF redaction annotations are
   applied *without* black boxes (text removed, image pixels blanked, covered
   line art removed), metadata dropped, the file fully rewritten with garbage
   collection, then read back (with OCR for pages without a text layer): any
   character under a box → error, nothing saved. Pass 2: the black boxes are
   painted onto the verified copy, which is renamed into place. Verifying
   before the boxes exist matters for scans: a box on top would hide leftover
   pixels from OCR (and OCR reads the box itself as junk).

Scanned pages (no text layer) are drawn into MuPDF's PDF-OCR writer at 300 dpi;
Tesseract's text comes back with positions in the page's own coordinates, so
detection, boxes and verification work unchanged. Page text is cached per file
(path + modified time) so the scan, AI pass and searches share one OCR run.

AI detection (`redact_ai_scan`) runs after the rules, in the background: the
page text is cut into chunks of whole lines (~160 words), GLiNER looks for
"person", "address" and any description the user types, and text a rule
already matched is left to the rule. The model is found bundled with the app
or in the app-data folder; otherwise the UI offers a one-time download, pinned
to one revision and checked by SHA-256 (`commands/model.rs`).

Building MuPDF on Windows needs LLVM (`.cargo/config.toml`) and a patched copy
of the `mupdf` crate (`third_party/README.md`).

## Convert

`pdf-core::convert`: images via MuPDF rendering + the `image` crate (PNG/JPEG);
text via the same page-text path as Redact (OCR for scans); DOCX via MuPDF's
DOCX writer (scanned pages are OCRed first so their text comes through);
searchable PDF = scanned pages replaced by MuPDF PDF-OCR pages (image + invisible
text), text pages kept, stitched with `assemble`; images → PDF via MuPDF's PDF
writer. PDF/A is not offered: real compliance needs output intents, XMP and font
rules MuPDF doesn't produce. Office → PDF would need LibreOffice (optional add-on, later).

## Edit

`pdf-core::edit`: everything is drawn in page space through the inverse page
transform, so rotated pages work. Text uses an embedded Noto Sans (Type0,
Identity-H, full ToUnicode), so it stays selectable and searchable in all four
languages; glyph IDs come from the font itself. Images via MuPDF's image insert,
highlights as standard annotations, whiteout as a white rectangle (it hides, it
does not remove; the UI says so and points to Redact). Forms: MuPDF widgets.

OCR note: MuPDF's Tesseract/Leptonica can't run on two threads at once, so all
OCR goes through one process-wide lock (`OcrPdf`).

## Shell: drop bubble, tray, autostart

`app/src-tauri/src/shell.rs`. Two windows: `main` and `bubble` (transparent,
undecorated, always on top, not in the taskbar; a second page, `bubble.html`).
The bubble grows its window to show its menu, picking the direction away from
the nearest screen edges, and shrinks back to its spot. Its position is saved
(logical coordinates) in `settings.json` in the app config folder and checked
against the connected screens at start. A dropped file goes to the main window
through `open_in_main` → stored request + `paperkit://open` event →
`take_open_request` (so it also works before the main window has loaded).
While a file is dragged near the bubble, its window grows into a 260 px ring
(stepping inward near screen edges); Tauri's drag-over positions pick the
segment, and the drop runs that segment's action (centre = full list). The glass
is drawn in CSS: OS blur effects only fill rectangular windows and stutter while
dragging. The mark (a page with a redacted line) is shared by the bubble,
sidebar and app/tray icon (`app-icon.svg`, `src/components/Mark.tsx`).

AI model offer: on launch, `ai_offer` says whether Home shows the card (model
missing and not declined) or starts the download at once (the Windows installer
left `download-ai-model.flag` next to the app: `windows/hooks.nsh` asks a yes/no
question after installing; the installer itself downloads nothing).

Tray: open, show/hide bubble, start with the system (`tauri-plugin-autostart`,
launched with `--background`: tray and bubble only), quit. Closing the main
window hides it. `tauri-plugin-single-instance` turns a second launch into
"show the running one".

## Packaging

GitHub Actions + `tauri-action`; bundled assets are fetched (pinned, hashed)
before building. The offline variant adds the AI model as a resource
(`tauri.offline.conf.json`), found by `commands/model.rs` before the download
folder. Not code-signed yet; macOS builds are ad-hoc signed.

## Open decisions

- [x] Open source or commercial? → **open source, AGPL-3.0-or-later, MuPDF** (decided 2026-11-25)
- [x] Document languages → English, French, Dutch, Greek (multilingual GLiNER; OCR to pick)
- [ ] Office → PDF support (requires bundling LibreOffice, ~300 MB) — v1 or later?

## Roadmap

1. **MVP** — drag & drop, thumbnails, split, merge, reorder/rotate ✅ · PDF ↔ images ✅ (via Convert) · keep bookmarks (todo)
2. **Redact** — manual + search redaction ✅, rule detection ✅, AI model ✅, OCR ✅
3. **Edit** — add text/images, whiteout, highlights, forms ✅
4. **Convert** — PDF → DOCX/images/text/searchable PDF, images → PDF ✅; Office → PDF (optional add-on, later)
