//! new-for-builtins — enforce `new` for builtins that need it, disallow for Symbol/BigInt.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "new-for-builtins",
    description: "Enforce `new` for constructors and disallow it for `Symbol`/`BigInt`.",
    remediation: "Use `new Map()` instead of `Map()` for constructors that \
                  require it. Conversely, use `Symbol()` and `BigInt()` without \
                  `new` — they are factory functions, not constructors.",
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
