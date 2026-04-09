//! max-file-lines — caps every source file at 200 lines.
//!
//! Applies to TS, TSX, JS, and Rust. All four languages share the same
//! text-only backend (`text.rs`) since the check is just a line count.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "max-file-lines",
    description: "Files longer than 200 lines carry too many responsibilities.",
    remediation: "File exceeds 200 lines. Split by responsibility — extract \
                  helpers into a separate module.",
    severity: Severity::Error,
    doc_url: None,
};

/// Register the rule with every supported language.
pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
        ],
    }
}
