mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-key-requires-domain-prefix",
    description: "t() key is missing a domain prefix (`domain.key`).",
    remediation: "Namespace every key under a domain so locale files stay organised: `auth.login.title`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
