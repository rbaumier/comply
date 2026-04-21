mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-manual-pluralization",
    description: "Manual `count === 1 ? singular : plural` ignores CLDR plural rules for non-English languages.",
    remediation: "Use `t('key', { count })` — i18next applies CLDR plural rules automatically.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
