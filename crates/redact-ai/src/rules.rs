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
    Rule { name: "national-id", find: national_id },
    Rule { name: "card", find: card },
    Rule { name: "labelled-id", find: labelled_id },
    Rule { name: "ssn", find: us_ssn },
    Rule { name: "date", find: date },
    Rule { name: "phone", find: phone },
    Rule { name: "ipv4", find: ipv4 },
];

fn finding(start: usize, end: usize, label: &str, category: Category) -> Finding {
    Finding { start, end, label: label.to_owned(), category, source: Source::Rule, confidence: None }
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

/// A number that follows a label saying what it is, in English, French, Dutch
/// or Greek: "Tax ID 771-204-559", "N° de sécurité sociale 1 84 12 76 451 089 46",
/// "BSN: 111222333", "ΑΦΜ: 094014201". Only the number is reported. A label is
/// strong evidence, so no checksum is required here (redaction favours recall).
pub fn labelled_id(text: &str) -> Vec<Finding> {
    let re = regex!(
        r"(?ix)
        \b(?P<label>
            # English
            tax\s*(?:id|no\.?|number)|tin|vat(?:\s*(?:id|no\.?|number))?|ssn|nin|
            passport(?:\s*(?:no\.?|number))?|id(?:\s*card)?\s*(?:no\.?|number)|identity\s*card|
            ref(?:erence)?\.?\s*(?:no\.?|number)|case\s*(?:no\.?|number)|file\s*(?:no\.?|number)|
            patient\s*(?:id|no\.?|number)|account\s*(?:no\.?|number)|policy\s*(?:no\.?|number)|
            member(?:ship)?\s*(?:id|no\.?|number)|employee\s*(?:id|no\.?|number)|
            # French
            num[ée]ro\s+fiscal|n[°o]\s*fiscal|nir|
            (?:n[°o]|num[ée]ro)\s*(?:de\s+)?s[ée]curit[ée]\s+sociale|
            (?:n[°o]|num[ée]ro)\s*(?:de\s+|d')?(?:dossier|passeport|client|contrat|police|r[ée]f[ée]rence|compte|patient)|
            passeport(?:\s*n[°o])?|carte\s+d'identit[ée](?:\s*n[°o])?|
            (?:n[°o]\s*)?tva(?:\s+intracommunautaire)?|siren|siret|r[ée]f\.|
            # Dutch
            bsn|burgerservicenummer|sofinummer|paspoort(?:nummer)?|identiteitskaart(?:nummer)?|
            id-?kaart(?:nummer)?|dossiernummer|klantnummer|polisnummer|patiëntnummer|kenmerk|
            btw[-\s]?(?:nummer|nr\.?|id)|rekeningnummer|kvk(?:[-\s]?nummer)?|
            # Greek
            ΑΦΜ|Α\.Φ\.Μ\.?|ΑΜΚΑ|Α\.Μ\.Κ\.Α\.?|ΑΔΤ|Α\.Δ\.Τ\.?|
            αρ(?:ιθμ[οό]ς|\.)\s*(?:ταυτ[οό]τητας|διαβατηρ[ιί]ου|μητρ[ωώ]ου|πρωτοκ[οό]λλου|λογαριασμο[υύ]|φακ[εέ]λου)|
            αρ\.\s*πρωτ\.?
        )
        [\s:\#.-]{0,3}
        # The number: letters, digits, / and -; a space only when a digit follows.
        # Greek capitals too: Greek ID cards read like ΑΚ 123456.
        (?P<id>[A-ZΑ-Ω0-9](?:[A-ZΑ-Ω0-9/-]|[\ ][0-9]){3,})"
    );
    re.captures_iter(text)
        .filter_map(|c| {
            let id = c.name("id")?;
            let has_digit = id.as_str().chars().any(|ch| ch.is_ascii_digit());
            let label = c.name("label")?.as_str();
            (has_digit && standalone(text, id.start(), id.end()))
                .then(|| finding(id.start(), id.end(), id_label(label), Category::Ids))
        })
        .collect()
}

fn id_label(raw: &str) -> &'static str {
    let l = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| l.contains(n));
    if has(&["αφμ", "α.φ.μ"]) {
        "Tax ID (ΑΦΜ)"
    } else if has(&["αμκα", "α.μ.κ.α"]) {
        "Social security (ΑΜΚΑ)"
    } else if has(&["αδτ", "α.δ.τ", "ταυτ"]) {
        "ID card number"
    } else if has(&["bsn", "burgerservice", "sofi"]) {
        "BSN"
    } else if has(&["sécurité sociale", "securite sociale"]) || l == "nir" {
        "Social security number"
    } else if l.starts_with("tax") || l == "tin" || has(&["fiscal"]) {
        "Tax ID"
    } else if has(&["vat", "tva", "btw"]) {
        "VAT number"
    } else if has(&["passport", "passeport", "paspoort", "διαβατ"]) {
        "Passport number"
    } else if has(&["identity", "identit", "id card", "kaart"]) {
        "ID card number"
    } else if has(&["patient", "patiënt"]) {
        "Patient ID"
    } else if has(&["account", "compte", "rekening", "λογαριασμ"]) {
        "Account number"
    } else if has(&["case", "dossier", "file", "φακ"]) {
        "Case number"
    } else if has(&["ref", "kenmerk", "πρωτ"]) {
        "Reference number"
    } else if has(&["siren", "siret", "kvk"]) {
        "Company number"
    } else {
        "ID number"
    }
}

/// National numbers with a checksum strong enough to report without a label:
/// French NIR (15 digits, mod-97 key) and Greek ΑΜΚΑ (11 digits: birth date
/// DDMMYY + Luhn). Dutch BSN and Greek ΑΦΜ are only 9 digits, so too many
/// ordinary numbers pass their checks; those are found through their labels.
pub fn national_id(text: &str) -> Vec<Finding> {
    let nir = regex!(r"\b[12][ ]?[0-9]{2}[ ]?[0-9]{2}[ ]?(?:[0-9]{2}|2[AB])[ ]?[0-9]{3}[ ]?[0-9]{3}[ ]?[0-9]{2}\b");
    let amka = regex!(r"\b[0-9]{11}\b");
    let nirs = nir
        .find_iter(text)
        .filter(|m| nir_ok(m.as_str()))
        .map(|m| finding(m.start(), m.end(), "Social security number (FR)", Category::Ids));
    let amkas = amka
        .find_iter(text)
        .filter(|m| amka_ok(m.as_str()))
        .map(|m| finding(m.start(), m.end(), "Social security (ΑΜΚΑ)", Category::Ids));
    nirs.chain(amkas).collect()
}

/// French NIR: key = 97 − (first 13 digits mod 97); Corsica 2A/2B count as 19/18.
pub fn nir_ok(nir: &str) -> bool {
    let compact: String = nir.chars().filter(|c| *c != ' ').collect::<String>().to_uppercase();
    if compact.len() != 15 {
        return false;
    }
    let body = compact[..13].replace("2A", "19").replace("2B", "18");
    let (Ok(n), Ok(key)) = (body.parse::<u64>(), compact[13..].parse::<u64>()) else { return false };
    97 - n % 97 == key
}

/// Greek ΑΜΚΑ: DDMMYY birth date followed by 5 digits, valid under Luhn.
pub fn amka_ok(amka: &str) -> bool {
    let d: Vec<u32> = amka.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() != 11 {
        return false;
    }
    let (day, month) = (d[0] * 10 + d[1], d[2] * 10 + d[3]);
    (1..=31).contains(&day) && (1..=12).contains(&month) && luhn_ok(&d)
}

/// Dutch BSN "elfproef": 9·d1 + 8·d2 + … + 2·d8 − d9 must be a multiple of 11.
pub fn bsn_ok(bsn: &str) -> bool {
    let d: Vec<i64> = bsn.chars().filter_map(|c| c.to_digit(10)).map(i64::from).collect();
    if d.len() != 9 {
        return false;
    }
    let sum: i64 = (0..8).map(|i| d[i] * (9 - i as i64)).sum::<i64>() - d[8];
    sum != 0 && sum % 11 == 0
}

/// Greek ΑΦΜ: Σ d_i·2^(8−i) over the first 8 digits, mod 11, mod 10, equals the 9th.
pub fn afm_ok(afm: &str) -> bool {
    let d: Vec<u32> = afm.chars().filter_map(|c| c.to_digit(10)).collect();
    d.len() == 9 && (0..8).map(|i| d[i] << (8 - i)).sum::<u32>() % 11 % 10 == d[8]
}

pub fn us_ssn(text: &str) -> Vec<Finding> {
    regex!(r"\b[0-9]{3}-[0-9]{2}-[0-9]{4}\b")
        .find_iter(text)
        .filter(|m| !m.as_str().starts_with("000") && !m.as_str().starts_with("666") && !m.as_str().starts_with('9'))
        .map(|m| finding(m.start(), m.end(), "Social security number", Category::Ids))
        .collect()
}

/// Month names (full and common abbreviations) in English, French, Dutch and
/// Greek. Greek is matched by stem, so "Μαρτίου" (of March, used in dates) and
/// "Μάρτιος" are both found.
const MONTHS: &str = r"(?:
    jan(?:uary|uari|vier)?|feb(?:ruary|ruari)?|f[ée]vr(?:ier)?|mar(?:ch)?|mars|maart|apr(?:il)?|avr(?:il)?|
    may|mai|mei|june?|juin|juni|july?|juil(?:let)?|juli|aug(?:ust|ustus)?|ao[uû]t|
    sep(?:t(?:ember|embre)?)?|oct(?:ober|obre)?|okt(?:ober)?|nov(?:ember|embre)?|d[ée]c(?:ember|embre)?|
    (?:ιανουαρ|φεβρουαρ|μ[αά]ρτ|απρ[ιί]λ|μα[ΐϊιί]|ιο[υύ]ν|ιο[υύ]λ|α[υύ]γο[υύ]στ|σεπτ[εέ]μβρ|οκτ[ωώ]βρ|ν[οό][εέ]μβρ|δεκ[εέ]μβρ)\p{Greek}*
)";

pub fn date(text: &str) -> Vec<Finding> {
    let numeric = regex!(r"\b(?:[0-3]?[0-9][./-][01]?[0-9][./-](?:19|20)?[0-9]{2}|(?:19|20)[0-9]{2}-[01][0-9]-[0-3][0-9])\b");
    static WRITTEN: LazyLock<Regex> = LazyLock::new(|| {
        // Day first ("15 March 2026", "1er mars 2026", "15 Μαρτίου 2026") or month first ("March 3rd, 2024").
        Regex::new(&format!(
            r"(?ix)\b(?:
                [0-3]?[0-9](?:st|nd|rd|th|er|e|η|ης)?\.?\s+{MONTHS}\.?,?\s+(?:19|20)[0-9]{{2}}
              | {MONTHS}\.?\s+[0-3]?[0-9](?:st|nd|rd|th)?,?\s+(?:19|20)[0-9]{{2}}
            )\b"
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
    fn labelled_ids_in_french_dutch_and_greek() {
        let t = "N° de sécurité sociale : 1 84 12 76 451 089 46. BSN: 111222333. Kenmerk 2026/0417. \
                 ΑΦΜ 094014201 και Αρ. Ταυτότητας ΑΚ123456. Numéro de dossier D-2026-17.";
        let found: Vec<(String, String)> = labelled_id(t).into_iter().map(|f| (t[f.start..f.end].to_owned(), f.label)).collect();
        let pairs: Vec<(&str, &str)> = found.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("1 84 12 76 451 089 46", "Social security number"),
                ("111222333", "BSN"),
                ("2026/0417", "Reference number"),
                ("094014201", "Tax ID (ΑΦΜ)"),
                ("ΑΚ123456", "ID card number"),
                ("D-2026-17", "Case number"),
            ]
        );
    }

    #[test]
    fn national_numbers_need_their_checksums() {
        let t = "NIR 1 84 12 76 451 089 46 or 184127645108946; bad 184127645108947. ΑΜΚΑ 01018012342 vs 01018012343, 45018012345";
        let got = texts(national_id(t), t);
        assert_eq!(got, ["1 84 12 76 451 089 46", "184127645108946", "01018012342"]);
    }

    #[test]
    fn checksums_for_labelled_numbers() {
        assert!(bsn_ok("111222333"));
        assert!(!bsn_ok("111222334"));
        assert!(afm_ok("094014201"));
        assert!(!afm_ok("094014202"));
    }

    #[test]
    fn dates_in_four_languages() {
        let t = "le 1er mars 2026, op 15 maart 2026, στις 15 Μαρτίου 2026, 3 Αυγούστου 2025, 12 février 2024";
        assert_eq!(texts(date(t), t), ["1er mars 2026", "15 maart 2026", "15 Μαρτίου 2026", "3 Αυγούστου 2025", "12 février 2024"]);
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
