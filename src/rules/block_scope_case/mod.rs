//! block-scope-case

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "block-scope-case",
    description: "`case` clause contains a lexical declaration that is not wrapped in a block.",
    remediation: "Wrap the case body in braces: `case X: { const y = ...; break; }`. Otherwise the binding leaks into sibling cases and can trigger TDZ errors.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S1301"),
    categories: &["code-quality"],

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
