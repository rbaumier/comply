//! html-no-undeferred-third-party — `<script src="https://...">` without
//! `defer` or `async` blocks HTML parsing for a cross-origin fetch.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-undeferred-third-party",
    description: "Third-party `<script>` without `defer`/`async` blocks parsing.",
    remediation: "Add `defer` or `async` to external `<script>` tags, or load \
                  them via `next/script` with `strategy=\"lazyOnload\"`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

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
