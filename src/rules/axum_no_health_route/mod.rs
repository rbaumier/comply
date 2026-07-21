//! axum-no-health-route

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-no-health-route",
    description: "A `Router` served via `axum::serve` registers no `/health` route — load balancers and orchestrators have no liveness signal.",
    remediation: "Register a health-check route (e.g. \
                  `.route(\"/health\", get(|| async { \"ok\" }))`) on the `Router` \
                  before serving it so platforms can probe liveness.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["deployment", "axum"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
