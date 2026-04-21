mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-concat-translation-key",
    description: "Dynamic `t()` keys built with concatenation or template literals can't be statically extracted.",
    remediation: "Use full static key strings: `t('section.home')` instead of `t('section.' + name)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
