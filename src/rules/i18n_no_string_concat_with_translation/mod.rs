mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-string-concat-with-translation",
    description: "Concatenating `t()` results breaks word order in RTL and agglutinative languages.",
    remediation: "Use interpolation: `t('greeting', { name })` instead of `t('hello') + ' ' + name`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
