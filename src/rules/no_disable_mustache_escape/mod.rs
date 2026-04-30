//! no-disable-mustache-escape

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-disable-mustache-escape",
    description: "Disabling template engine HTML escaping (`escapeMarkup = false`) opens XSS vectors.",
    remediation: "Keep HTML escaping enabled. If raw HTML is truly needed, sanitize it explicitly before rendering.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
