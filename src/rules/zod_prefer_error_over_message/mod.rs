//! zod-prefer-error-over-message — Zod v4's error-customization key is
//! bidirectional: `z.*`/`.refine` take `error`, `ctx.addIssue` takes `message`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-error-over-message",
    description: "Zod v4's error-customization key is bidirectional: `z.*` calls and `.refine` \
                  options take `error` (a string `message` there is the deprecated v3 spelling), \
                  while `ctx.addIssue({ ... })` takes `message` (an `error` key there is silently \
                  dropped).",
    remediation: "In a `z.*`/`.refine` call rename `message` to `error` (e.g. \
                  `z.string({ error: '...' })`); in `ctx.addIssue({ ... })` rename `error` to \
                  `message`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],

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
