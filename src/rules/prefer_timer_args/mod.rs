//! Prefer passing timer arguments directly instead of wrapping in arrow function.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-timer-args",
    description: "Prefer `setTimeout(fn, delay, arg)` over `setTimeout(() => fn(arg), delay)`.",
    remediation: "Pass arguments directly to setTimeout/setInterval: `setTimeout(fn, delay, arg1, arg2)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["e18e", "modernization"],
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
