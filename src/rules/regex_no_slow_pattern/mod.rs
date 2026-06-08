//! regex-no-slow-pattern

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-slow-pattern",
    description: "Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).",
    remediation: "Refactor to avoid nested quantifiers like `(a+)+`, `(a*)*`, `(a+)*`, `(.*)*`. Use atomic groups, possessive quantifiers, or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "regex"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
