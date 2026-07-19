//! escape-case — require uppercase hex digits in escape sequences.
//!
//! Skipped in test directories: NLP/tokenizer fixtures capture verbatim model
//! output as `\uXXXX` escapes copied from upstream references, where the hex
//! casing is cosmetic (`\u0e1e` and `\u0E1E` decode to the same code point) and
//! normalizing it diverges from the source. Real source still flags.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "escape-case",
    description: "Use uppercase characters for the value of escape sequences.",
    remediation: "Replace lowercase hex digits in escape sequences with uppercase: \
                  `\\xff` -> `\\xFF`, `\\u00ff` -> `\\u00FF`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],

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
