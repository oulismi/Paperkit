# PaperKit (working name)

Offline desktop PDF toolkit for Windows, macOS and Linux.
Drag & drop a PDF, then **Split · Merge · Redact (on-device AI) · Edit · Convert**.
Nothing is uploaded — all processing, including the AI, runs on the user's computer.

UI mockups: https://claude.ai/artifact/WKtaRobDQMAGFCPSnijk46

## Project layout

```
paperkit/
├── app/                     Desktop app (Tauri 2)
│   ├── src/                 Frontend — React + TypeScript
│   │   ├── app/             App shell, routing, layout, sidebar
│   │   ├── components/      Shared UI (buttons, drop zone, page thumbnails…)
│   │   ├── features/        One folder per tool
│   │   │   ├── home/        Drag & drop start screen, recent files
│   │   │   ├── split-merge/ Page organizer: split, merge, extract, reorder
│   │   │   ├── redact/      Redaction review UI (AI suggestions, approve/reject)
│   │   │   ├── edit/        Annotations, text/images, forms, page rotate/delete
│   │   │   └── convert/     PDF ↔ Word / images / text / PDF/A
│   │   ├── lib/             Typed wrappers around Tauri commands, pdf.js helpers
│   │   └── styles/          Design tokens, global CSS
│   ├── public/              Static assets
│   └── src-tauri/           Rust side of the app
│       ├── src/commands/    Tauri commands the UI calls (split, merge, redact…)
│       ├── capabilities/    Tauri permission files
│       └── icons/           App icons for all platforms
├── crates/                  Reusable Rust libraries (no UI code)
│   ├── pdf-core/            PDF operations: split, merge, redact, render, convert
│   └── redact-ai/           Sensitive-data detection
│       └── src/rules/       Regex/checksum rules (email, phone, IBAN, cards, IDs)
├── models/                  ONNX models (NER, OCR) — downloaded, not committed
├── scripts/                 Dev/build helpers (model download, packaging)
├── docs/                    Architecture, roadmap, decisions
├── tests/
│   ├── fixtures/            Sample PDFs (fake data only!)
│   └── e2e/                 End-to-end UI tests
└── .github/workflows/       CI: build + test on Windows, macOS, Linux
```

## Status

Working now:
- Drag & drop or browse PDFs, page thumbnails, reorder by dragging, rotate, remove
- **Split** (every N pages, custom ranges, at selected pages), **Merge**, **Extract**
- **Redact**:
  - format rules with checksums (English, French, Dutch, Greek): emails, phones, IBANs, cards,
    national IDs (FR NIR, GR ΑΜΚΑ), labelled IDs (BSN, ΑΦΜ, tax/VAT/passport/case numbers…), dates
  - on-device AI (GLiNER multilingual) for names, addresses and anything described in words
    ("patient name"); downloaded once on first use (~596 MB), then fully offline
  - find-and-redact text, draw boxes, review and approve; saves a new copy with the content
    really removed (MuPDF), then verifies it

Next: OCR for scanned pages; an "offline" installer with the AI model bundled. Edit and Convert
are placeholders.

## Prerequisites

- Node.js 20+
- Rust (stable) via https://rustup.rs
- LLVM (libclang, clang-cl) to build MuPDF — Windows: `winget install LLVM.LLVM`
  (see `.cargo/config.toml`); the first build compiles MuPDF and takes a few minutes
- Windows: Visual Studio **2022** Build Tools, C++ workload (ONNX Runtime needs its C++ library)
- Tauri OS dependencies: https://tauri.app/start/prerequisites/
  (Windows: WebView2 + "Desktop development with C++" build tools)

## Run it

```sh
cd app
npm install
npm run tauri dev        # opens the desktop app with hot reload
```

Sample PDFs with fictional content are in `tests/fixtures/`; regenerate them with
`cargo run -p pdf-core --example sample_pdfs -- tests/fixtures`.

## Test

```sh
cargo test -p pdf-core -p redact-ai   # PDF engine, redaction, detection rules

# AI model test (downloads ~596 MB into models/ once):
node scripts/fetch-model.mjs
PAPERKIT_MODEL_DIR=models/gliner_multi-v2.1 cargo test -p redact-ai --features ner --release -- --ignored
cd app && npm test       # split planning logic
npm run typecheck
```

Installers: `cd app && npm run tauri build` (output under `target/release/bundle/`).

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the technical plan.

## License

AGPL-3.0-or-later (see [LICENSE](LICENSE)), as required by MuPDF.
