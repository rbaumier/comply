//! ui-tabular-nums-on-data — JSX elements displaying numeric data
//! (counters, prices, metrics) should use `tabular-nums`.

mod css;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-tabular-nums-on-data",
    description: "Elements rendering numeric data (counters, prices, metrics) should use `tabular-nums` so digits don't shift width between ticks.",
    remediation: "Add `font-variant-numeric: tabular-nums` (or Tailwind `tabular-nums`) to the element's className.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

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
            (Language::Css, Backend::TreeSitter(Box::new(css::Check))),
        ],
    }
}
