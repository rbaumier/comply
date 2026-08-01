//! unknown-shape-prefer-schema

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "unknown-shape-prefer-schema",
    description: "Validating the shape of a value explicitly typed `unknown` with \
                  hand-written property checks (`typeof v.x`, `v.x === 'tag'`, \
                  `'key' in v`) is a hand-rolled schema — use a schema validator \
                  (zod, valibot, …).",
    remediation: "Declare the shape once and parse the value at the boundary, e.g. \
                  `const Bubble = z.object({ type: z.literal('text'), text: \
                  z.string() }); Bubble.safeParse(value)`. The parsed result is \
                  typed, so the guard and its call sites disappear.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: true,
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
