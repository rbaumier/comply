//! axum-hardcoded-bind-port

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-hardcoded-bind-port",
    description: "An axum service binds every interface on a port written into the source — a platform that injects the port through the environment cannot publish this service.",
    remediation: "Read the port from the environment (`std::env::var(\"PORT\")`) and bind that, keeping the literal only as the fallback the local run needs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["deployment", "axum"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
