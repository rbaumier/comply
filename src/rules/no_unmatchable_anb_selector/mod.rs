mod css;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unmatchable-anb-selector",
    description: "Disallow unmatchable An+B selectors.",
    remediation: "An An+B selector whose formula is `0` (e.g. `:nth-child(0)`, `:nth-child(0n)`, `:nth-child(0n+0)`) can never match an element; remove it or use a formula that selects at least one element.",
    severity: Severity::Error,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/CSS/:nth-child"),
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
