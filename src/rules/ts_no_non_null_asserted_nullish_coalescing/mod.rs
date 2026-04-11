//! ts-no-non-null-asserted-nullish-coalescing — flag `x! ?? y`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-asserted-nullish-coalescing",
    description: "`x! ?? y` is contradictory — `!` asserts non-null, `??` handles null.",
    remediation: "Remove the `!` (let `??` do its job) or remove the `??` (if the value is never null).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
