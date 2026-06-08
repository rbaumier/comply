//! ts-prefer-nullish-coalescing — `||` → `??` when the LHS may be a falsy non-nullish value.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-nullish-coalescing",
    description: "`a || b` treats every falsy value (`0`, `\"\"`, `false`) as missing. `a ?? b` only triggers on `null` / `undefined`.",
    remediation: "Use `??` when you mean \"fall back when null or undefined\". Keep `||` when you genuinely want any falsy value to short-circuit (rare).",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-nullish-coalescing/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
