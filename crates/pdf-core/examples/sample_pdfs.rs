//! Writes sample PDFs with made-up content for trying the app:
//! `cargo run -p pdf-core --example sample_pdfs -- tests/fixtures`

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

fn sample(title: &str, pages: u32, landscape_page: Option<u32>) -> Document {
    let mut doc = Document::with_version("1.5");
    let tree = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let mut kids: Vec<Object> = vec![];
    for n in 1..=pages {
        let (w, h) = if Some(n) == landscape_page { (842, 595) } else { (595, 842) };
        let text = |size: i64, x: i64, y: i64, s: String| {
            vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), size.into()]),
                Operation::new("Td", vec![x.into(), y.into()]),
                Operation::new("Tj", vec![Object::string_literal(s)]),
                Operation::new("ET", vec![]),
            ]
        };
        let mut ops = text(28, 60, h - 100, title.to_owned());
        ops.extend(text(160, w / 2 - 60, h / 2 - 40, n.to_string()));
        ops.extend(text(12, 60, 50, format!("Sample document - page {n} of {pages} - fictional content")));
        let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
        kids.push(
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => tree, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
            })
            .into(),
        );
    }
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => pages as i64,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", catalog);
    doc.compress();
    doc
}

/// One page of running text with fictional personal data, for trying Redact.
fn contract() -> Document {
    let lines = [
        "EMPLOYMENT AGREEMENT                         Ref. no. HR-2026-0417",
        "",
        "This Agreement is made on 15 March 2026 between Northwind Logistics Ltd",
        "(Employer) and Elena Marlowe-Kade (Employee), residing at",
        "42 Harbour Lane, Brightwater.",
        "",
        "1. Position. The Employee is engaged as Senior Analyst, reporting to",
        "Jonas Whitfield, commencing on the date above.",
        "",
        "2. Remuneration. Salary is paid monthly to account",
        "GB29 NWBK 6016 1331 9268 19. Tax ID 771-204-559.",
        "",
        "3. Contact. Notices to the Employee: e.marlowe@mailbox.example,",
        "tel. +44 7700 900 123.",
        "",
        "4. Confidentiality. The Employee shall not disclose any proprietary",
        "information of the Employer during or after employment.",
        "",
        "Sample document - all names, numbers and addresses are fictional.",
    ];
    let mut doc = Document::with_version("1.5");
    let tree = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let mut ops = vec![];
    for (i, line) in lines.iter().enumerate() {
        ops.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![60.into(), (770 - 22 * i as i64).into()]),
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
    let info = doc.add_object(dictionary! { "Author" => Object::string_literal("HR Department (fictional)") });
    doc.trailer.set("Root", catalog);
    doc.trailer.set("Info", info);
    doc.compress();
    doc
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "tests/fixtures".into()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, title, pages, landscape) in [
        ("Report_Q1.pdf", "Quarterly Report Q1", 6, Some(4)),
        ("Annex_A.pdf", "Annex A", 3, None),
    ] {
        let path = dir.join(name);
        sample(title, pages, landscape).save(&path).unwrap();
        println!("wrote {}", path.display());
    }
    let path = dir.join("Employment_Contract.pdf");
    contract().save(&path).unwrap();
    println!("wrote {}", path.display());
}
