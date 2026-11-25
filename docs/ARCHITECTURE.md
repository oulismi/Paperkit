# Architecture

## Stack

| Layer      | Choice                                   |
|------------|------------------------------------------|
| App shell  | Tauri 2 (Rust backend, web frontend)     |
| UI         | React + TypeScript, pdf.js for previews  |
| PDF engine | `lopdf` (MIT) for page operations; MuPDF or PDFium for redaction/rendering — **decision pending** (licensing) |
| AI runtime | ONNX Runtime (CPU)                       |
| NER model  | GLiNER (small/multilingual) for names, addresses, orgs |
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

## Open decisions

- [ ] Open source or commercial? → MuPDF (AGPL) vs PDFium (Apache-2.0)
- [ ] Document languages → choice of NER and OCR models
- [ ] Office → PDF support (requires bundling LibreOffice, ~300 MB) — v1 or later?

## Roadmap

1. **MVP** — drag & drop, thumbnails, split, merge, reorder/rotate ✅ · PDF ↔ images (todo) · keep bookmarks (todo)
2. **Redact** — manual + search redaction, rule detection, AI model, OCR
3. **Edit** — annotations, add text/images, forms
4. **Convert** — PDF → DOCX, optional Office → PDF
