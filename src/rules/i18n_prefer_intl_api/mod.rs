mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-prefer-intl-api",
    description: "`.toLocaleDateString()` without an explicit locale uses the environment default, which varies by machine.",
    remediation: "Pass `i18n.language` as the first argument or use `Intl.DateTimeFormat(locale).format(date)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
