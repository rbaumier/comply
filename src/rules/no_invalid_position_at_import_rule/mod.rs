mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-position-at-import-rule",
    description: "Disallow the use of `@import` at-rules in invalid positions.",
    remediation: "Move every `@import` before all other at-rules and style rules; only `@charset` and `@layer` may precede it.",
    severity: Severity::Error,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/CSS/@import"),
    categories: &["css"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(css::Check)))],
    }
}
