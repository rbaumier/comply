mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-manual-list-join",
    description: "Array joined with a locale-specific separator instead of Intl.ListFormat.",
    remediation: "Use `new Intl.ListFormat(locale, { type: 'conjunction' }).format(list)` so separators translate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
