//! zod-refine-requires-path — object-level `.refine()` must attach its
//! error to a specific field via `path: [...]`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-refine-requires-path",
    description: "`z.object().refine()` without `path:` attaches the error to the whole object, not a specific field.",
    remediation: "Add `path: ['fieldName']` to the refine options so form errors appear on the correct field.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
