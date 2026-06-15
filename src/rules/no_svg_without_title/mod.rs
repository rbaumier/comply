//! no-svg-without-title

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-svg-without-title",
    description: "`<svg>` elements must have an accessible name.",
    remediation: "Add a non-empty `<title>` as the first child of the `<svg>`, or give it an accessible name via `aria-label`/`aria-labelledby`. If the SVG is purely decorative, mark it `aria-hidden=\"true\"` or give it a non-image role (e.g. `role=\"presentation\"`).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
