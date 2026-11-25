//! End to end on the sample contract: detect, redact everything found, and check
//! that none of it survives anywhere in the saved file.

use std::path::PathBuf;

use pdf_core::redact::{page_texts, redact_file, RedactOptions};

#[test]
fn every_detected_item_is_gone_from_the_output() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/Employment_Contract.pdf");
    let out = std::env::temp_dir().join(format!("paperkit-contract-{}.pdf", std::process::id()));

    let pages = page_texts(&src, None).unwrap();
    let mut secrets = Vec::new();
    let mut areas = Vec::new();
    for page in &pages {
        for f in redact_ai::detect(&page.text) {
            secrets.push(page.text[f.start..f.end].to_owned());
            areas.extend(page.areas(f.start, f.end));
        }
    }
    for expected in ["HR-2026-0417", "15 March 2026", "GB29 NWBK 6016 1331 9268 19", "771-204-559", "e.marlowe@mailbox.example", "+44 7700 900 123"] {
        assert!(secrets.iter().any(|s| s == expected), "detected {expected}: {secrets:?}");
    }

    redact_file(&src, &out, &areas, RedactOptions::default()).unwrap();

    let after: String = page_texts(&out, None).unwrap().iter().map(|p| p.text.as_str()).collect();
    for s in &secrets {
        assert!(!after.contains(s.as_str()), "{s} is still in the redacted text");
    }
    assert!(after.contains("Northwind Logistics Ltd"), "unflagged text stays");

    // Not just the text layer: the raw bytes must not hold the secrets either
    // (content streams are compressed, so also check the decompressed objects).
    let mut doc = lopdf::Document::load(&out).unwrap();
    doc.decompress();
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw);
    for s in &secrets {
        assert!(!raw.contains(s.as_str()), "{s} is still in the file bytes");
    }
    assert!(!raw.contains("HR Department"), "metadata removed");
    std::fs::remove_file(out).unwrap();
}

/// The same contract as a scan (image only): read with OCR in all four app
/// languages, detect, redact, and check with OCR that nothing detected survives.
#[test]
fn scanned_contract_is_read_with_ocr_and_redacted() {
    let tessdata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/resources/tessdata");
    if !tessdata.join("ell.traineddata").exists() {
        eprintln!("skipped: run `node scripts/fetch-tessdata.mjs` first");
        return;
    }
    let ocr = pdf_core::redact::Ocr { datadir: tessdata, languages: "eng+fra+nld+ell".into() };
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/Scanned_Contract.pdf");
    let out = std::env::temp_dir().join(format!("paperkit-scanned-{}.pdf", std::process::id()));

    let pages = page_texts(&src, Some(&ocr)).unwrap();
    assert!(pages[0].ocr);
    let mut secrets = Vec::new();
    let mut areas = Vec::new();
    for page in &pages {
        for f in redact_ai::detect(&page.text) {
            secrets.push(page.text[f.start..f.end].to_owned());
            areas.extend(page.areas(f.start, f.end));
        }
    }
    for expected in ["e.marlowe@mailbox.example", "GB29 NWBK 6016 1331 9268 19", "771-204-559", "15 March 2026"] {
        assert!(secrets.iter().any(|s| s == expected), "OCR + rules found {expected}: {secrets:?}");
    }

    let options = RedactOptions { ocr: Some(ocr.clone()), ..RedactOptions::default() };
    redact_file(&src, &out, &areas, options).unwrap();
    let after: String = page_texts(&out, Some(&ocr)).unwrap().iter().map(|p| p.text.as_str()).collect();
    for s in &secrets {
        assert!(!after.contains(s.as_str()), "{s} is still readable after redaction");
    }
    assert!(after.contains("Northwind"), "the rest of the page is still readable");
    std::fs::remove_file(out).unwrap();
}
