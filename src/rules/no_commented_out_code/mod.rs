//! no-commented-out-code — delete dead comments, git history keeps originals.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-commented-out-code",
    description: "Commented-out code is unreviewable, unreachable, and rots.",
    remediation: "Delete the commented-out code. Git history preserves the \
                  original if you need to recover it.",
    severity: Severity::Warning,
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
    RuleDef {
        meta: META,
        backends,
    }
}
