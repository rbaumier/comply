//! justify-inaction

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::files::Language;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "justify-inaction",
    description: "Empty `catch {}`, `else {}`, or early `return;` without an explaining comment.",
    remediation: "Add a comment on the preceding line explaining why the block is intentionally empty or why the early return is correct. Silent inaction hides bugs — make the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .copied()
            .chain(std::iter::once(Language::Rust))
            .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
