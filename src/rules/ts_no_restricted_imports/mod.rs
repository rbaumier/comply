//! ts-no-restricted-imports — disallow imports whose module specifier
//! matches a user-configured pattern list.
//!
//! Opt-in via `comply.toml`:
//!
//! ```toml
//! [rules.ts-no-restricted-imports]
//! patterns = ["@banned/*", "lodash"]
//! ```
//!
//! Absent or empty list → rule is a no-op.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-imports",
    description: "Disallow imports whose module specifier matches a configured pattern list.",
    remediation: "Replace the restricted import with the recommended alternative, or remove the pattern from `[rules.ts-no-restricted-imports] patterns` in `comply.toml`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-imports"),
    categories: &["typescript"],

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
