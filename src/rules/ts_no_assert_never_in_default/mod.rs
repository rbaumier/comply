//! ts-no-assert-never-in-default — `switch` over a discriminated union with a
//! `default: throw new Error(...)` branch silently goes stale when a new
//! variant is added. Use an exhaustive check (`assertNever(x)` /
//! `const _: never = x;`) so the type-checker catches missing branches.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-assert-never-in-default",
    description: "`switch { default: throw }` without an exhaustive `never` check goes stale when union variants are added.",
    remediation: "Replace `default: throw new Error(...)` with `default: return assertNever(x);` \
                  (or `const _exhaustive: never = x;`) so TypeScript flags the missing case at compile time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
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
