//! i18n-enforce-message-id — `<FormattedMessage>` requires explicit `id`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-enforce-message-id",
    description: "`<FormattedMessage>` / `formatMessage` without an explicit `id` produces non-deterministic keys at build time and breaks translation tooling.",
    remediation: "Add an explicit `id` prop (or `id` property to the descriptor object). Hashing on `defaultMessage` only is fragile to whitespace edits.",
    severity: Severity::Warning,
    doc_url: Some("https://formatjs.io/docs/tooling/linter#enforce-id"),
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
