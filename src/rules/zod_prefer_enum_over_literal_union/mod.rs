//! zod-prefer-enum-over-literal-union — prefer `z.enum([...])` when a
//! `z.union([...])` is built entirely from `z.literal('...')` string
//! literals. `z.enum` is shorter, produces better error messages, and
//! gives a narrow string-literal union on the TypeScript side without
//! the manual `z.literal` wrapping.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-enum-over-literal-union",
    description: "`z.union([z.literal('a'), z.literal('b')])` with only string literals should use `z.enum([...])`.",
    remediation: "Use z.enum(['a', 'b']) instead of z.union with literals",
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
