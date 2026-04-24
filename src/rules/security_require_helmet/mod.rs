//! security-require-helmet

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "security-require-helmet",
    description: "Express apps must install `helmet()` for default security headers.",
    remediation: "Add `app.use(helmet())` after creating the Express app.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
