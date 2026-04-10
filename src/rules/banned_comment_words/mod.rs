//! banned-comment-words — flag dismissive filler words in code comments.
//!
//! Words like "obviously", "simply", "just", "basically" are red flags in
//! comments. They paper over complexity without explaining it. The
//! coding-standards skill says: "If it's obvious, no comment is needed; if
//! it needs `simply`, it's not simple." Strip the filler and either delete
//! the comment or rewrite it to explain the actual subtlety.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "banned-comment-words",
    description: "Dismissive filler words in comments hide complexity instead of explaining it.",
    remediation: "Remove the filler word and rewrite the comment to explain the actual \
                  subtlety. If the line is genuinely obvious, delete the comment instead. \
                  Banned: obviously, simply, just, basically, clearly, trivially.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Rust,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef { meta: META, backends }
}
