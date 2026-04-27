//! elysia-deploy-no-health

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-no-health",
    description: "Elysia server exposes `.listen()` without a `/health` endpoint — load balancers and orchestrators have no liveness signal.",
    remediation: "Add `.get('/health', () => ({ status: 'ok' }))` (or similar) so platforms can probe readiness.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["deployment", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
