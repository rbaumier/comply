//! prefer-called-exactly-once-with — collapse `toHaveBeenCalledTimes(1)` +
//! `toHaveBeenCalledWith(...)` into the single matcher
//! `toHaveBeenCalledExactlyOnceWith(...)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-called-exactly-once-with",
    description: "Prefer `toHaveBeenCalledExactlyOnceWith(args)` over separate `toHaveBeenCalledTimes(1)` + `toHaveBeenCalledWith(args)` assertions.",
    remediation: "Use toHaveBeenCalledExactlyOnceWith(args) instead of separate assertions",
    severity: Severity::Warning,
    doc_url: Some("https://vitest.dev/api/expect.html#tohavebeencalledexactlyoncewith"),
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
