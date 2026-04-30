mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-english-key",
    description: "t() key looks like a full English sentence, not an identifier.",
    remediation: "Use an identifier-style key such as `domain.key` and put the English copy in the locale file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
