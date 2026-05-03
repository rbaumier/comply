mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-max-words",
    description: "Comment sentence exceeds 25 words.",
    remediation: "Split long comment sentences — one idea per sentence keeps the intent scannable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}

pub(crate) const MAX_WORDS_PER_SENTENCE: usize = 25;

/// Strip comment markers (`//`, `/*`, `*/`, `///`, `//!`, `/**`) and return the
/// inner body. Operates on the raw slice emitted by tree-sitter.
pub(crate) fn strip_markers(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches("///")
            .trim_start_matches("//!")
            .trim_start_matches("//")
            .trim_start_matches("/**")
            .trim_start_matches("/*")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim_end_matches("*/")
            .trim();
        if !trimmed.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(trimmed);
        }
    }
    out
}

/// True if any sentence in `body` has more than `MAX_WORDS_PER_SENTENCE` words.
pub(crate) fn has_long_sentence(body: &str) -> bool {
    body.split(['.', '!', '?'])
        .map(|s| s.split_whitespace().count())
        .any(|n| n > MAX_WORDS_PER_SENTENCE)
}
