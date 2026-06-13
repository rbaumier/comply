//! zod-no-any — use z.unknown() over z.any().

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-any",
    description: "`z.any()` disables validation and type narrowing.",
    remediation: "Replace `z.any()` with `z.unknown()`. The runtime \
                  behavior is the same (everything accepted) but the \
                  TypeScript type is `unknown`, forcing downstream code \
                  to narrow before using the value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
