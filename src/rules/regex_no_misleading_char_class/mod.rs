//! regex-no-misleading-char-class

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-misleading-char-class",
    description: "Character class contains multi-codepoint graphemes that will be split.",
    remediation: "Emoji with ZWJ or chars above U+FFFF inside `[...]` are split into individual code points. Use alternation `(?:a|b)` instead of `[ab]` for multi-codepoint sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: ALL_TEXT_LANGUAGES
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
