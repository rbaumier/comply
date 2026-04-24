//! api-put-vs-patch — flag handlers registered via `app.put(...)` / router
//! PUT methods whose body references `Partial<...>`. PUT must replace the
//! full resource; partial updates belong on PATCH. Mixing the two breaks
//! idempotency guarantees clients rely on.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-put-vs-patch",
    description: "PUT handlers with Partial<> payloads should use PATCH instead.",
    remediation:
        "If the handler accepts fields-provided-only semantics, register it with `.patch(...)`. Keep `.put(...)` for full-resource replacement.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
