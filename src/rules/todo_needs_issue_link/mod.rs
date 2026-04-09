//! todo-needs-issue-link — every TODO/FIXME must reference an issue.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "todo-needs-issue-link",
    description: "TODO/FIXME without a tracked reference rots into silent tech debt.",
    remediation: "Add an issue reference after TODO/FIXME — `#123`, `GH-123`, \
                  a ticket key (`ABC-123`), or a full URL.",
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
