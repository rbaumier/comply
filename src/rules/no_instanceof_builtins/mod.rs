//! no-instanceof-builtins — flag `x instanceof Array` and other builtins.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-instanceof-builtins",
    description: "Avoid `instanceof` for built-in types — it fails across realms.",
    remediation: "Use `Array.isArray(x)` instead of `x instanceof Array`. \
                  For errors, check the `name` property or use `Error.isError()`. \
                  `instanceof` breaks across iframes, VMs, and module boundaries.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
