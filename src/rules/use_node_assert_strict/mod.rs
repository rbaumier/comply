//! use-node-assert-strict — promote `node:assert/strict` over `node:assert`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-node-assert-strict",
    description: "Importing `node:assert` uses the loose assertion API; `node:assert/strict` is preferred.",
    remediation: "Import from `node:assert/strict` instead of `node:assert`.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-node-assert-strict/"),
    categories: &["node"],

    skip_in_test_dir: false,
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
