//! prefer-to-have-length — suggest `toHaveLength(n)` over `toBe(n)` / `toEqual(n)` on `.length`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-to-have-length",
    description: "Use `toHaveLength(n)` instead of asserting on `.length` with `toBe`/`toEqual`.",
    remediation: "Use expect(x).toHaveLength(n) instead",
    severity: Severity::Warning,
    doc_url: Some("https://jestjs.io/docs/expect#tohavelengthnumber"),
    categories: &["testing"],
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
