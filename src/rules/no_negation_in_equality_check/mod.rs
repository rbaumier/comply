//! no-negation-in-equality-check — flag `!x === y` (precedence bug).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-negation-in-equality-check",
    description: "Negated expression in equality check is a precedence bug.",
    remediation: "`!x === y` is parsed as `(!x) === y`, not `!(x === y)`. \
                  Use `x !== y` or wrap explicitly: `!(x === y)`.",
    severity: Severity::Error,
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
