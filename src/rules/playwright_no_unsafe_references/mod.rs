//! playwright-no-unsafe-references — flag `page.evaluate()` with only a function argument (no explicit args).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-unsafe-references",
    description: "`page.evaluate()` runs in the browser — outer-scope variables are not available unless passed as the second argument.",
    remediation: "Pass captured variables as the second argument to \
                  `page.evaluate((arg) => { ... }, arg)`. Variables from \
                  the Node.js scope are not serialized into the browser \
                  context automatically — they will be `undefined` at \
                  runtime.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

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
