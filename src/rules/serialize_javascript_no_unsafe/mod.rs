//! serialize-javascript-no-unsafe — flag `serialize(x, { unsafe: true })`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "serialize-javascript-no-unsafe",
    description: "`serialize(value, { unsafe: true })` disables HTML escaping (XSS risk).",
    remediation: "Don't use unsafe option in serialize-javascript, it disables HTML escaping.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/yahoo/serialize-javascript#user-content-options"),
    categories: &["security"],

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
