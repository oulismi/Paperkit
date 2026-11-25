//! Exact-format rules. Each one is a regex plus, where the format has one, a
//! checksum that throws out look-alikes (an IBAN must pass mod-97, a card number
//! the Luhn check).

use std::sync::LazyLock;

use regex::Regex;

use crate::{Category, Finding, Source};

pub struct Rule {
    pub name: &'static str,
    pub find: fn(&str) -> Vec<Finding>,
}

/// In priority order: when matches overlap, the earlier rule keeps the text.
pub const ALL: &[Rule] = &[
    Rule { name: "email", find: email },
    Rule { name: "iban", find: iban },
    Rule { name: "card", find: card },
    Rule { name: "labelled-id", find: labelled_id },
    Rule { name: "ssn", find: us_ssn },
    Rule { name: "date", find: date },
    Rule { name: "phone", find: phone },
    Rule { name: "ipv4", find: ipv4 },
];

fn finding(start: usize, end: usize, label: &str, category: Category) -> Finding {
    Finding { start, end, label: label.to_owned(), category, source: Source::Rule }
}

macro_rules! regex {
    ($re:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($re).expect("valid rule regex"));
        &*RE
    }};
}

/// Regex has no lookaround; this checks a match isn't glued to more letters or digits.
fn standalone(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

pub fn email(text: &str) -> Vec<Finding> {
    regex!(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,}\b")
        .find_iter(text)
        .map(|m| finding(m.start(), m.end(), "Email", Category::Contact))
        .collect()
}

/// Two letters, two check digits, then 11–30 letters/digits, in groups or not.
pub fn iban(text: &str) -> Vec<Finding> {
    regex!(r"\b[A-Z]{2}[0-9]{2}(?:[ ]?[A-Z0-9]){11,30}\b")
        .find_iter(text)
        .filter_map(|m| {
            // The regex is greedy and may swallow a following word ("…19 TAX"):
            // try the longest prefix that passes the checksum.
            let compact_ends: Vec<usize> = m
                .as_str()
                .char_indices()
                .filter(|(_, c)| *c != ' ')
                .map(|(i, c)| i + c.len_utf8())
                .collect();
            compact_ends.iter().rev().find_map(|&end| {
                let candidate = &m.as_str()[..end];
                let compact: String = candidate.chars().filter(|c| *c != ' ').collect();
                (compact.len() >= 15 && iban_checksum_ok(&compact) && standalone(text, m.start(), m.start() + end))
                    .then(|| finding(m.start(), m.start() + end, "IBAN", Category::Ids))
            })
        })
        .collect()
}

/// ISO 13616: move the first four characters to the end, letters → 10..35, mod 97 == 1.
pub fn iban_checksum_ok(iban: &str) -> bool {
    let rearranged = iban[4..].chars().chain(iban[..4].chars());
    let mut rem: u32 = 0;
    for c in rearranged {
        let v = match c.to_digit(36) {
            Some(v) => v,
            None => return false,
        };
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    }
    rem == 1
}

/// 13–19 digits, optionally grouped by spaces or dashes, passing Luhn.
pub fn card(text: &str) -> Vec<Finding> {
    regex!(r"\b[0-9](?:[ -]?[0-9]){12,18}\b")
        .find_iter(text)
        .filter(|m| {
            let digits: Vec<u32> = m.as_str().chars().filter_map(|c| c.to_digit(10)).collect();
            luhn_ok(&digits)
        })
        .map(|m| finding(m.start(), m.end(), "Payment card", Category::Ids))
        .collect()
}

pub fn luhn_ok(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| if i % 2 == 1 { if d * 2 > 9 { d * 2 - 9 } else { d * 2 } } else { d })
        .sum();
    !digits.is_empty() && sum % 10 == 0
}

/// A number that follows a label saying what it is: "Tax ID 771-204-559",
/// "Passport no. X1234567", "ΑΦΜ: 123456789". Only the number is reported.
pub fn labelled_id(text: &str) -> Vec<Finding> {
    let re = regex!(
        r"(?ix)
        \b(?P<label>
            tax\s*(?:id|no\.?|number)|tin|vat(?:\s*(?:id|no\.?|number))?|ssn|nin|
            passport(?:\s*(?:no\.?|number))?|id(?:\s*card)?\s*(?:no\.?|number)|identity\s*card|
            ref(?:erence)?\.?\s*(?:no\.?|number)|case\s*(?:no\.?|number)|file\s*(?:no\.?|number)|
            patient\s*(?:id|no\.?|number)|account\s*(?:no\.?|number)|policy\s*(?:no\.?|number)|
            member(?:ship)?\s*(?:id|no\.?|number)|employee\s*(?:id|no\.?|number)|
            ΑΦΜ|Α\.Φ\.Μ\.|ΑΜΚΑ|Α\.Μ\.Κ\.Α\.|ΑΔΤ|Α\.Δ\.Τ\.
        )
        [\s:\#.-]{0,3}
        (?P<id>[A-Z0-9][A-Z0-9/-]{3,}[0-9A-Z])"
    );
    re.captures_iter(text)
        .filter_map(|c| {
            let id = c.name("id")?;
            let has_digit = id.as_str().chars().any(|ch| ch.is_ascii_digit());
            let label = c.name("label")?.as_str();
            (has_digit && standalone(text, id.start(), id.end()))
                .then(|| finding(id.start(), id.end(), &id_label(label), Category::Ids))
        })
        .collect()
}

fn id_label(raw: &str) -> String {
    let l = raw.to_lowercase();
    let label = if l.starts_with("tax") || l == "tin" {
        "Tax ID"
    } else if l.starts_with("vat") {
        "VAT number"
    } else if l.starts_with("passport") {
        "Passport number"
    } else if l.starts_with("ref") {
        "Reference number"
    } else if l.starts_with("case") {
        "Case number"
    } else if l.starts_with("patient") {
        "Patient ID"
    } else if l.starts_with("account") {
        "Account number"
    } else if l.contains("αφμ") || l.contains("α.φ.μ") {
        "Tax ID (ΑΦΜ)"
    } else if l.contains("αμκα") || l.contains("α.μ.κ.α") {
        "Social security (ΑΜΚΑ)"
    } else {
        "ID number"
    };
    label.to_owned()
}

pub fn us_ssn(text: &str) -> Vec<Finding> {
    regex!(r"\b[0-9]{3}-[0-9]{2}-[0-9]{4}\b")
        .find_iter(text)
        .filter(|m| !m.as_str().starts_with("000") && !m.as_str().starts_with("666") && !m.as_str().starts_with('9'))
        .map(|m| finding(m.start(), m.end(), "Social security number", Category::Ids))
        .collect()
}

pub fn date(text: &str) -> Vec<Finding> {
    let numeric = regex!(r"\b(?:[0-3]?[0-9][./-][01]?[0-9][./-](?:19|20)?[0-9]{2}|(?:19|20)[0-9]{2}-[01][0-9]-[0-3][0-9])\b");
    const MONTHS: &str = "(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";
    static WRITTEN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(&format!(
            r"(?i)\b(?:[0-3]?[0-9](?:st|nd|rd|th)?\s+{MONTHS}\.?,?\s+(?:19|20)[0-9]{{2}}|{MONTHS}\.?\s+[0-3]?[0-9](?:st|nd|rd|th)?,?\s+(?:19|20)[0-9]{{2}})\b"
        ))
        .expect("valid date regex")
    });
    numeric
        .find_iter(text)
        .chain(WRITTEN.find_iter(text))
        .map(|m| finding(m.start(), m.end(), "Date", Category::Dates))
        .collect()
}

/// International (+30 …) or grouped national numbers with 8–15 digits.
pub fn phone(text: &str) -> Vec<Finding> {
    regex!(r"(?:\+|\(|\b00)?[0-9](?:[ .()-]{0,2}[0-9]){7,16}\b")
        .find_iter(text)
        .filter(|m| {
            let s = m.as_str();
            let digits = s.chars().filter(char::is_ascii_digit).count();
            // A bare run of digits is more likely an amount or ID than a phone number.
            let formatted = s.starts_with('+') || s.starts_with("00") || s.contains([' ', '-', '(', '.']);
            (8..=15).contains(&digits) && formatted && standalone(text, m.start(), m.end())
        })
        .map(|m| finding(m.start(), m.end(), "Phone number", Category::Contact))
        .collect()
}

pub fn ipv4(text: &str) -> Vec<Finding> {
    regex!(r"\b(?:(?:25[0-5]|2[0-4][0-9]|1?[0-9]?[0-9])\.){3}(?:25[0-5]|2[0-4][0-9]|1?[0-9]?[0-9])\b")
        .find_iter(text)
        .map(|m| finding(m.start(), m.end(), "IP address", Category::Contact))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(found: Vec<Finding>, text: &str) -> Vec<String> {
        found.into_iter().map(|f| text[f.start..f.end].to_owned()).collect()
    }

    #[test]
    fn iban_needs_a_valid_checksum() {
        let t = "pay GB29 NWBK 6016 1331 9268 19 or GB29NWBK60161331926819 not GB28 NWBK 6016 1331 9268 19";
        assert_eq!(texts(iban(t), t), ["GB29 NWBK 6016 1331 9268 19", "GB29NWBK60161331926819"]);
        let gr = "IBAN GR16 0110 1250 0000 0001 2300 695";
        assert_eq!(texts(iban(gr), gr), ["GR16 0110 1250 0000 0001 2300 695"]);
    }

    #[test]
    fn iban_does_not_swallow_the_next_word() {
        let t = "GB29 NWBK 6016 1331 9268 19 TAX";
        assert_eq!(texts(iban(t), t), ["GB29 NWBK 6016 1331 9268 19"]);
    }

    #[test]
    fn cards_need_luhn() {
        let t = "4111-1111-1111-1111 and 4111 1111 1111 1112";
        assert_eq!(texts(card(t), t), ["4111-1111-1111-1111"]);
    }

    #[test]
    fn labelled_ids_report_only_the_number() {
        let t = "Tax ID 771-204-559, Passport no. X1234567, ΑΦΜ: 094014201, Case number ABCD";
        assert_eq!(texts(labelled_id(t), t), ["771-204-559", "X1234567", "094014201"]);
    }

    #[test]
    fn dates_numeric_and_written() {
        let t = "on 15 March 2026, 03/04/2025, 2026-11-25 and March 3rd, 2024; not 45/99/1";
        assert_eq!(texts(date(t), t), ["03/04/2025", "2026-11-25", "15 March 2026", "March 3rd, 2024"]);
    }

    #[test]
    fn phones_need_formatting() {
        let t = "tel. +44 7700 900 123, (210) 555-0199, 0030 210 1234567, total 123456789";
        assert_eq!(texts(phone(t), t), ["+44 7700 900 123", "(210) 555-0199", "0030 210 1234567"]);
    }

    #[test]
    fn emails_and_ips() {
        let t = "write to a.b+c@sub.example.org from 192.168.1.20 not 999.1.1.1";
        assert_eq!(texts(email(t), t), ["a.b+c@sub.example.org"]);
        assert_eq!(texts(ipv4(t), t), ["192.168.1.20"]);
    }
}
