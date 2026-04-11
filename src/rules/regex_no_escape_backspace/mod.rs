//! regex-no-escape-backspace

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-escape-backspace",
    description: "`[\\b]` in a regex matches the backspace character, not a word boundary — this is almost always a mistake.",
    remediation: "Use `\\b` outside a character class for a word boundary. If you truly need backspace, add a comment explaining the intent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY_AND_RUST
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
