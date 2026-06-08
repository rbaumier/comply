//! prefer-json-parse-buffer — prefer reading a JSON file as a buffer.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-json-parse-buffer",
    description: "Prefer reading a JSON file as a buffer.",
    remediation: "Remove the `'utf-8'` / `'utf8'` encoding argument from \
                  `fs.readFileSync()` when the result is passed to `JSON.parse()`. \
                  `JSON.parse()` accepts a `Buffer` directly, which avoids an \
                  intermediate string allocation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
