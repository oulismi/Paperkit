//! Finds sensitive data in plain text.
//!
//! Today this is a set of exact-format rules ([`rules`]): emails, phone numbers,
//! IBANs, payment cards, labelled ID numbers and dates, each checked with its
//! checksum where one exists. Names and addresses need the on-device NER model,
//! which is the next step.
//!
//! Detection only *suggests*. The user reviews every finding before anything is
//! removed from a document.

pub mod rules;

use serde::Serialize;

/// The groups the review screen filters by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Names,
    Addresses,
    Contact,
    Ids,
    Dates,
    Custom,
}

/// How a finding was made, shown to the user next to each suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A format rule matched (and its checksum, if it has one, passed).
    Rule,
    /// The user searched for this text.
    Search,
}

/// One piece of sensitive text: `text[start..end]` (byte offsets).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub start: usize,
    pub end: usize,
    /// What it is, for people: "Email", "IBAN", "Tax ID"…
    pub label: String,
    pub category: Category,
    pub source: Source,
}

/// Runs every rule over `text` and returns non-overlapping findings in text order.
/// Where two rules match overlapping text, the earlier rule in [`rules::ALL`]
/// wins (an IBAN is not also reported as a phone number).
pub fn detect(text: &str) -> Vec<Finding> {
    let mut found: Vec<Finding> = Vec::new();
    for rule in rules::ALL {
        for f in (rule.find)(text) {
            if !found.iter().any(|g| f.start < g.end && g.start < f.end) {
                found.push(f);
            }
        }
    }
    found.sort_by_key(|f| f.start);
    found
}

/// Every occurrence of `needle` in `text`, ignoring case. For "Find & redact text"
/// and the "Also look for" box.
pub fn search(text: &str, needle: &str) -> Vec<Finding> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let pattern = format!("(?i){}", regex::escape(needle));
    let re = regex::Regex::new(&pattern).expect("escaped pattern is valid");
    re.find_iter(text)
        .map(|m| Finding {
            start: m.start(),
            end: m.end(),
            label: format!("\u{201c}{needle}\u{201d}"),
            category: Category::Custom,
            source: Source::Search,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(&str, String)> {
        detect(text).into_iter().map(|f| (&text[f.start..f.end], f.label)).collect()
    }

    #[test]
    fn finds_the_mockup_contract_items() {
        let text = "Ref. no. HR-2026-0417\n\
            This Agreement is made on 15 March 2026 between Northwind Logistics Ltd.\n\
            Salary is paid monthly to account GB29 NWBK 6016 1331 9268 19. Tax ID 771-204-559.\n\
            Notices: e.marlowe@mailbox.example, tel. +44 7700 900 123.";
        let got = found(text);
        let texts: Vec<&str> = got.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            texts,
            [
                "HR-2026-0417",
                "15 March 2026",
                "GB29 NWBK 6016 1331 9268 19",
                "771-204-559",
                "e.marlowe@mailbox.example",
                "+44 7700 900 123"
            ]
        );
        assert_eq!(got[2].1, "IBAN");
        assert_eq!(got[3].1, "Tax ID");
    }

    #[test]
    fn earlier_rules_win_overlaps() {
        // A card number is also a long digit run a phone rule could match.
        let got = found("Card 4111 1111 1111 1111 on file");
        assert_eq!(got, [("4111 1111 1111 1111", "Payment card".to_owned())]);
    }

    #[test]
    fn search_ignores_case_and_escapes() {
        let text = "Case A.1 and case a.1, not caseXa11";
        let hits = search(text, "case a.1");
        assert_eq!(hits.len(), 2);
        assert!(search(text, "   ").is_empty());
    }
}
