//! no-verb-in-rest-url — REST URLs are resources, not verbs.

mod rust;
mod typescript;
mod verb_url_match;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-verb-in-rest-url",
    description: "REST URLs should identify resources, not actions.",
    remediation: "Replace verb-in-URL patterns with HTTP semantics: \
                  `POST /api/orders` to create, `GET /api/orders/:id` to \
                  read, `PATCH /api/orders/:id` to update, \
                  `DELETE /api/orders/:id` to remove.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};
pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
