//! html-no-script-without-defer — `<script src="...">` without `defer` or
//! `async` blocks the parser and delays First Contentful Paint.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-script-without-defer",
    description: "`<script src=\"...\">` without `defer` or `async` blocks HTML parsing.",
    remediation: "Add `defer` (preserves execution order, runs after parse) for DOM-dependent \
                  scripts, or `async` (runs as soon as it loads) for independent scripts. \
                  Without either, the browser stops parsing HTML to fetch and execute the \
                  script, delaying First Contentful Paint.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
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
