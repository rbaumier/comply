//! i18n-enforce-default-message — `<FormattedMessage>` requires `defaultMessage`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-enforce-default-message",
    description: "`<FormattedMessage>` without `defaultMessage` displays the raw key when no translation exists.",
    remediation: "Add a `defaultMessage` prop that the user can read in the absence of a localised string.",
    severity: Severity::Warning,
    doc_url: Some("https://formatjs.io/docs/tooling/linter#enforce-default-message"),
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
