//! no-xml-external-entity

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-xml-external-entity",
    description: "XML parsers without XXE protection are vulnerable to external entity attacks.",
    remediation: "Disable external entities: set `noent: false` or `externalEntities: false` when configuring XML parsers.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
