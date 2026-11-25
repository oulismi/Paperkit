//! On-device named-entity recognition with GLiNER (multilingual, ONNX).
//!
//! GLiNER takes free-text labels ("person", "address", or anything the user
//! types, like "patient name") and returns the spans that match them. The model
//! runs on the CPU through ONNX Runtime; nothing leaves the computer.

use std::path::Path;

use gliner::model::input::text::TextInput;
use gliner::model::params::Parameters;
use gliner::model::pipeline::span::SpanMode;
use gliner::model::GLiNER;
use orp::params::RuntimeParameters;

use crate::{Category, Finding, Source};

/// What to look for: the label given to the model, and how to file its results.
#[derive(Debug, Clone)]
pub struct Target {
    pub label: String,
    pub display: String,
    pub category: Category,
}

impl Target {
    /// The built-in targets: people and addresses.
    pub fn defaults() -> Vec<Target> {
        vec![
            Target { label: "person".into(), display: "Person".into(), category: Category::Names },
            Target { label: "address".into(), display: "Address".into(), category: Category::Addresses },
        ]
    }

    /// Something the user typed in "Also look for".
    pub fn custom(text: &str) -> Target {
        let text = text.trim();
        let mut display = text.to_owned();
        if let Some(first) = display.get(..1) {
            display.replace_range(..1, &first.to_uppercase());
        }
        Target { label: text.to_lowercase(), display, category: Category::Custom }
    }
}

/// Words per chunk sent to the model. The model reads at most 384 sub-word
/// tokens; 160 words keeps well under that for all four target languages.
const CHUNK_WORDS: usize = 160;
/// Chunks per inference call: bounds memory use on long documents.
const BATCH: usize = 8;

pub struct NerModel {
    model: GLiNER<SpanMode>,
}

impl NerModel {
    /// Loads the model. `onnx` is the model file, `tokenizer` its tokenizer.json.
    pub fn load(onnx: &Path, tokenizer: &Path) -> Result<Self, String> {
        // 0.5 (the model's standard): at 0.4 generic words such as "Employee"
        // came back as people.
        let params = Parameters::default().with_threshold(0.5);
        let model = GLiNER::<SpanMode>::new(params, RuntimeParameters::default(), tokenizer, onnx).map_err(|e| e.to_string())?;
        Ok(Self { model })
    }

    /// Finds `targets` in `text`. Offsets in the findings are byte offsets into `text`.
    pub fn detect(&self, text: &str, targets: &[Target]) -> Result<Vec<Finding>, String> {
        if targets.is_empty() || text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let labels: Vec<String> = targets.iter().map(|t| t.label.clone()).collect();
        let chunks = chunk(text, CHUNK_WORDS);
        let mut found = Vec::new();
        for batch in chunks.chunks(BATCH) {
            let texts: Vec<String> = batch.iter().map(|&(s, e)| text[s..e].to_owned()).collect();
            let input = TextInput::new(texts, labels.clone()).map_err(|e| e.to_string())?;
            let output = self.model.inference(input).map_err(|e| e.to_string())?;
            for (spans, &(chunk_start, _)) in output.spans.iter().zip(batch) {
                for span in spans {
                    let Some(target) = targets.iter().find(|t| t.label == span.class()) else { continue };
                    let (s, e) = span.offsets();
                    found.push(Finding {
                        start: chunk_start + s,
                        end: chunk_start + e,
                        label: target.display.clone(),
                        category: target.category,
                        source: Source::Ai,
                        confidence: Some(span.probability()),
                    });
                }
            }
        }
        Ok(found)
    }
}

/// Splits `text` into byte ranges of whole lines holding about `words` words
/// each (a single longer line becomes its own chunk). Entities rarely cross a
/// line break and never cross a chunk boundary, so lines are kept together.
fn chunk(text: &str, words: usize) -> Vec<(usize, usize)> {
    let mut chunks = Vec::new();
    let (mut start, mut count) = (0, 0);
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        let n = line.split_whitespace().count();
        if count > 0 && count + n > words {
            chunks.push((start, pos));
            start = pos;
            count = 0;
        }
        count += n;
        pos += line.len();
    }
    if count > 0 {
        chunks.push((start, pos));
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_whole_lines() {
        let text = "a b c\nd e\nf g h i\n";
        assert_eq!(chunk(text, 5), [(0, 10), (10, 18)]);
        assert_eq!(chunk(text, 100), [(0, 18)]);
        assert_eq!(chunk("", 5), []);
    }

    #[test]
    fn custom_targets_are_capitalised_for_display() {
        let t = Target::custom("  patient name ");
        assert_eq!(t.label, "patient name");
        assert_eq!(t.display, "Patient name");
    }

    /// Needs the model files; run with
    /// `PAPERKIT_MODEL_DIR=<dir with onnx/model_fp16.onnx and tokenizer.json> cargo test -p redact-ai --features ner -- --ignored`
    #[test]
    #[ignore]
    fn finds_people_and_addresses_in_four_languages() {
        let dir = std::path::PathBuf::from(std::env::var("PAPERKIT_MODEL_DIR").expect("PAPERKIT_MODEL_DIR"));
        let model = NerModel::load(&dir.join("onnx/model_fp16.onnx"), &dir.join("tokenizer.json")).unwrap();
        let cases = [
            "The employee Elena Marlowe-Kade lives at 42 Harbour Lane, Brightwater.",
            "Le salarié Jean Dupont habite au 12 rue de la Paix, 75002 Paris.",
            "De werknemer Pieter de Vries woont aan de Keizersgracht 123 in Amsterdam.",
            "Ο υπάλληλος Γιώργος Παπαδόπουλος μένει στην οδό Ερμού 25, Αθήνα.",
        ];
        for text in cases {
            let found = model.detect(text, &Target::defaults()).unwrap();
            let names: Vec<&str> = found.iter().filter(|f| f.category == Category::Names).map(|f| &text[f.start..f.end]).collect();
            println!("{text}\n  → {:?}", found.iter().map(|f| (&text[f.start..f.end], &f.label, f.confidence)).collect::<Vec<_>>());
            assert!(!names.is_empty(), "no person found in: {text}");
            assert!(found.iter().any(|f| f.category == Category::Addresses && f.confidence.unwrap() > 0.8), "no address found in: {text}");
        }
    }
}
