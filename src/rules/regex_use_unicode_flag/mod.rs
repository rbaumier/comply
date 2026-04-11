//! regex-use-unicode-flag

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-use-unicode-flag",
    description: "Unicode property escapes (`\\p{...}` / `\\P{...}`) require the `u` or `v` flag.",
    remediation: "Add the `u` flag to the regex: `/\\p{Letter}/u`. Without it, `\\p` is not interpreted as a Unicode property escape.",
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
