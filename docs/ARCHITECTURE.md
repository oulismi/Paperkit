# Architecture

## Stack

| Layer      | Choice                                   |
|------------|------------------------------------------|
| App shell  | Tauri 2 (Rust backend, web frontend)     |
| UI         | React + TypeScript, pdf.js for previews  |
| PDF engine | MuPDF (AGPL) for redaction and text positions; `lopdf` (MIT) for page operations |
| AI runtime | ONNX Runtime (CPU)                       |
| NER model  | GLiNER multilingual v2.1 (Apache-2.0), ONNX fp16, via `gline-rs` + ONNX Runtime |
| OCR        | Tesseract or PaddleOCR (ONNX)            |

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
3. `redact_apply`: MuPDF redaction annotations are added and applied (text
   removed, image pixels blanked, covered line art removed, black boxes drawn),
   metadata dropped, and the file fully rewritten with garbage collection to a
   temp file. It is read back: any character under a box → the file is deleted
   and an error returned. Otherwise it is renamed into place.

AI detection (`redact_ai_scan`) runs after the rules, in the background: the
page text is cut into chunks of whole lines (~160 words), GLiNER looks for
"person", "address" and any description the user types, and text a rule
already matched is left to the rule. The model is found bundled with the app
or in the app-data folder; otherwise the UI offers a one-time download, pinned
to one revision and checked by SHA-256 (`commands/model.rs`).

Building MuPDF on Windows needs LLVM (`.cargo/config.toml`) and a patched copy
of the `mupdf` crate (`third_party/README.md`).

## Open decisions

- [x] Open source or commercial? → **open source, AGPL-3.0-or-later, MuPDF** (decided 2026-11-25)
- [x] Document languages → English, French, Dutch, Greek (multilingual GLiNER; OCR to pick)
- [ ] Office → PDF support (requires bundling LibreOffice, ~300 MB) — v1 or later?

## Roadmap

1. **MVP** — drag & drop, thumbnails, split, merge, reorder/rotate ✅ · PDF ↔ images (todo) · keep bookmarks (todo)
2. **Redact** — manual + search redaction ✅, rule detection ✅, AI model ✅, OCR (todo)
3. **Edit** — annotations, add text/images, forms
4. **Convert** — PDF → DOCX, optional Office → PDF
